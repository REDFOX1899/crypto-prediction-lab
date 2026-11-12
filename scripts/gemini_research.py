#!/usr/bin/env python3
"""Prepare the feature catalog and offline hourly Decisions API request bodies.

The prepare command uses a saved /api/hourly response and does not make network
requests. Its output is a blinded request plus a separate local evaluation map.
"""
import argparse
from datetime import datetime, timezone
from decimal import Decimal, InvalidOperation
import hashlib
import json
import os
from pathlib import Path
import re
import tempfile

WINDOWS = (5, 10, 20, 30, 60, 120, 300, 600)
STATS = ('mean', 'stddev', 'minimum', 'maximum', 'delta')
SERIES = {'BTC': 'KXBTCD', 'ETH': 'KXETHD', 'SOL': 'KXSOLD'}
STREAMS = {
    'book': {
        'yes_bid_cents': 'cents', 'yes_ask_cents': 'cents',
        'yes_mid_cents': 'cents', 'yes_spread_cents': 'cents', 'microprice_cents': 'cents',
        'yes_best_size': 'contracts', 'no_best_size': 'contracts',
        'yes_top3_size': 'contracts', 'no_top3_size': 'contracts',
        'yes_top10_size': 'contracts', 'no_top10_size': 'contracts',
        'best_imbalance': 'signed_fraction', 'top10_imbalance': 'signed_fraction',
    },
    'reference': {
        'ref_usd': 'USD', 'ref_gap_bps': 'bps', 'ref_abs_gap_bps': 'bps',
        'ref_change_from_open_bps': 'bps', 'ref_final_average_gap_bps': 'bps',
    },
    'cross_asset_reference': {
        'btc_ref_1m_return_bps': 'bps', 'eth_ref_1m_return_bps': 'bps', 'sol_ref_1m_return_bps': 'bps',
    },
    'consecutive_books': {
        'yes_mid_change_cents': 'cents', 'yes_spread_change_cents': 'cents',
        'yes_top10_size_change': 'contracts', 'no_top10_size_change': 'contracts',
    },
}
TARGETS = {
    'ask_up': 'the YES ask be at least 2 cents above its input snapshot value',
    'bid_down': 'the YES bid be at least 2 cents below its input snapshot value',
    'wide_spread': 'the YES bid–ask spread exceed 3 cents',
    'offer_size': 'the displayed size at the best YES offer be at least five contracts',
    'mid_up': 'the YES midpoint be strictly above its input snapshot value',
}


def catalog():
    return [{'id': f'{stream}.{seconds}s.{stat}', 'source': family, 'unit': unit,
             'window_seconds': seconds, 'statistic': stat}
            for family, streams in STREAMS.items() for stream, unit in streams.items()
            for seconds in WINDOWS for stat in STATS]


def timestamp(text):
    if not isinstance(text, str) or not re.fullmatch(
            r'\d{4}-\d\d-\d\dT\d\d:\d\d:\d\d(?:\.\d{1,9})?(?:Z|[+-]\d\d:\d\d)', text):
        raise ValueError('Timestamp with explicit timezone required')
    normalized = re.sub(r'(\.\d{6})\d+', r'\1', text).replace('Z', '+00:00')
    return datetime.fromisoformat(normalized).astimezone(timezone.utc)


def decimal(value):
    if isinstance(value, bool) or not isinstance(value, (str, float, int)):
        return None
    try:
        n = Decimal(str(value))
        return n if n.is_finite() else None
    except InvalidOperation:
        return None


def digest(value):
    payload = json.dumps(value, sort_keys=True, ensure_ascii=False, allow_nan=False, separators=(',', ':'))
    return hashlib.sha256(payload.encode()).hexdigest()


def prepare(board, asset, mode):
    cutoff = timestamp(board['fetched_at'])
    close = (int(cutoff.timestamp()) // 3600 + 1) * 3600
    remaining = close - cutoff.timestamp()
    rows = board['markets']
    if not isinstance(rows, list) or len(rows) > 1000:
        raise ValueError('Invalid market listing')
    strikes, tickers, floors = [], set(), set()
    for m in rows:
        ticker = m.get('ticker')
        if not isinstance(ticker, str) or not re.fullmatch(SERIES[asset] + r'-[A-Z0-9.-]+', ticker):
            raise ValueError('Foreign or invalid contract identity')
        if m.get('status') != 'active' or m.get('strike_type') != 'greater' or timestamp(m['close_time']).timestamp()!=close:
            raise ValueError('Mixed market rules or close')
        floor, yes, no = map(decimal, (m.get('floor_strike'), m.get('yes_ask_dollars'), m.get('no_ask_dollars')))
        if floor is None or floor <= 0 or ticker in tickers or floor in floors:
            raise ValueError('Invalid or duplicate threshold')
        tickers.add(ticker)
        floors.add(floor)
        if yes is None or no is None or not (0 < yes < 1 and 0 < no < 1 and 1-no < yes):
            continue
        bid = (1-no)*100
        ask = yes*100
        strikes.append((abs((bid+ask)/2-50),floor,ticker,bid,ask))
    chosen = sorted(strikes)[:20 if mode=='stress' else 1]
    if not chosen:
        raise ValueError('No eligible two-sided strike')
    anchor = chosen[0][1]
    chosen.sort(key=lambda row: row[1])
    context = {'asset': asset, 'horizon': 'hourly threshold', 'seconds_to_close': round(remaining,3),
               'rule': 'YES when the official expiration value is strictly above the listed floor.',
               'anchor': 'Strikes expressed in basis points relative to a central listed floor.',
               'quotes': []}
    references, questions = {}, {}
    for i, (_,floor,ticker,bid,ask) in enumerate(chosen,1):
        name = f'S{i:02}'
        context['quotes'].append({'strike':name, 'floor_offset_bps':float((floor/anchor-1)*10000),
                                  'yes_bid_cents':float(bid), 'yes_ask_cents':float(ask)})
        references[name] = {'ticker':ticker, 'floor':str(floor), 'close_epoch':close}
        if mode=='primary':
            questions['official_yes'] = {'type':'noul','instructions':f'Will {name} officially settle YES at expiry?'}
            continue
        for seconds in range(30,301,30):
            if seconds>=remaining:
                continue
            for key,target in TARGETS.items():
                questions[f'{name}_{seconds}s_{key}'] = {
                    'type':'noul', 'instructions':f'For {name}, at the checkpoint {seconds} seconds after the snapshot, will {target}?'}
    if not questions:
        raise ValueError('All requested checkpoints fall at or after the close')
    request={'model':'gemini-decision-1','input':[{'type':'text','text':json.dumps(context,sort_keys=True,allow_nan=False)}],
             'questions':questions}
    return {'schema_version':1, 'mode':mode, 'request':request,
            'evaluation_only':{'fetched_at':board['fetched_at'],'source_hash':digest(board),
                               'request_hash':digest(request),'strikes':references}}


def portfolio(baselines):
    """A shared cutoff across the three hourly snapshots, capped at 20 strikes."""
    if set(baselines)!=set(SERIES):
        raise ValueError('The shared packet requires all three assets')
    cutoff=max(timestamp(record['receipt']).timestamp() for record in baselines.values())
    close=(int(cutoff)//3600+1)*3600
    candidates={}
    snapshots={}
    for asset,record in baselines.items():
        board=record['board']
        received=timestamp(record['receipt']).timestamp()
        fetched=timestamp(board['fetched_at']).timestamp()
        if not fetched<=received<=cutoff or cutoff-fetched>30:
            raise ValueError('Stale or future cross-asset snapshot')
        envelope=prepare(board,asset,'stress')
        context=json.loads(envelope['request']['input'][0]['text'])
        refs=envelope['evaluation_only']['strikes']
        if any(ref['close_epoch']!=close for ref in refs.values()):
            raise ValueError('Mixed expiry in shared packet')
        candidates[asset]=sorted(context['quotes'], key=lambda q:(abs(q['yes_ask_cents']+q['yes_bid_cents']-100),q['strike']))
        snapshots[asset]={'references':refs,'fetch_age_seconds':round(cutoff-fetched,3)}
    selected=[]
    # Round robin from the most balanced two-sided quotes in each asset.
    for rank in range(20):
        for asset in SERIES:
            if rank<len(candidates[asset]):
                selected.append((asset,candidates[asset][rank]))
                if len(selected)==20:break
        if len(selected)==20:break
    quotes=[];references={};questions={}
    for asset,quote in selected:
        alias=asset+'_'+quote['strike']
        quotes.append({**quote,'asset':asset,'strike':alias})
        references[alias]=snapshots[asset]['references'][quote['strike']]
        for seconds in range(30,301,30):
            if seconds>=close-cutoff:continue
            for key,target in TARGETS.items():
                questions[f'{alias}_{seconds}s_{key}']={'type':'noul','instructions':
                    f'For {alias}, at the checkpoint {seconds} seconds after the shared cutoff, will {target}?'}
    if not questions:raise ValueError('No eligible shared checkpoint')
    evidence={'seconds_to_close':round(close-cutoff,3),'floor_offsets':'bps relative to a central listed floor within each asset',
              'snapshot_age_seconds':{a:snapshots[a]['fetch_age_seconds'] for a in SERIES},'quotes':quotes}
    request={'model':'gemini-decision-1','input':[{'type':'text','text':json.dumps(evidence,sort_keys=True)}],'questions':questions}
    return {'schema_version':1,'mode':'cross_asset_stress','request':request,
            'evaluation_only':{'shared_cutoff':datetime.fromtimestamp(cutoff,timezone.utc).isoformat(),
             'strikes':references,'source_hashes':{a:digest(r['board']) for a,r in baselines.items()},'request_hash':digest(request)}}

def quote_pair(m):
    ask = decimal(m.get('yes_ask_dollars'))
    no = decimal(m.get('no_ask_dollars'))
    if ask is None or no is None or not (0 < ask < 1 and 0 < no < 1 and 1 - no < ask):
        return None
    return (1 - no)*100, ask*100


def replay(baseline, followups, asset):
    """Measure observable labels; never grade a model that has not run."""
    envelope = prepare(baseline['board'], asset, 'stress')
    return grade_checkpoints(
        baseline, followups, asset, envelope['evaluation_only']['strikes'],
        envelope['request']['questions'], timestamp(baseline['board']['fetched_at']).timestamp(),
        envelope['evaluation_only']['request_hash'])


def replay_portfolio(baselines, followups):
    envelope = portfolio(baselines)
    evaluation = envelope['evaluation_only']
    by_asset = {}
    for asset in SERIES:
        prefix = asset + '_'
        questions = [q[len(prefix):] for q in envelope['request']['questions'] if q.startswith(prefix)]
        refs = {k[len(prefix):]: ref for k, ref in evaluation['strikes'].items() if k.startswith(prefix)}
        by_asset[asset] = grade_checkpoints(
            baselines[asset], followups[asset], asset, refs, questions,
            timestamp(evaluation['shared_cutoff']).timestamp(), evaluation['request_hash'])
    return {'request_hash': evaluation['request_hash'], 'question_count': len(envelope['request']['questions']),
            'observed_labels': sum(r['observed_labels'] for r in by_asset.values()), 'by_asset': by_asset,
            'provider_calls': 0}


def grade_checkpoints(baseline, followups, asset, refs, questions, start, request_hash):
    base = baseline['board']
    received = timestamp(baseline['receipt']).timestamp()
    fetched = timestamp(base['fetched_at']).timestamp()
    if not fetched <= received <= fetched + 30:
        raise ValueError('Initial snapshot is stale or received before its fetch timestamp')
    initial_receipt = max(start, received)
    anchors = {m['ticker']:quote_pair(m) for m in base['markets']}
    timeline = []
    for later in followups:
        board = later['board']
        t = timestamp(board['fetched_at']).timestamp()
        receipt = timestamp(later['receipt']).timestamp()
        if t <= initial_receipt or receipt < t or receipt-t > 15:
            continue
        observed = {}
        for m in board['markets']:
            ticker = m['ticker']
            if ticker in observed:
                raise ValueError('Duplicate ticker in subsequent snapshot')
            observed[ticker] = m
        timeline.append((t,receipt,observed))
    by_target = {k:{'yes':0,'no':0,'missing':0} for k in TARGETS}
    coverage = {}
    for key in questions:
        alias, horizon, target = key.split('_',2)
        seconds = int(horizon[:-1])
        # Select earliest receipt among equally close snapshots; never change
        # the chosen source because a better price or a quote is missing.
        if seconds not in coverage:
            eligible = [s for s in timeline if abs(s[0]-(start+seconds)) <= 5]
            coverage[seconds] = min(eligible, key=lambda s:(abs(s[0]-(start+seconds)),s[1])) if eligible else None
        selected = coverage[seconds]
        ref=refs[alias]
        future = selected[2].get(ref['ticker']) if selected else None
        label = None
        if future and future.get('status')=='active' and future.get('strike_type')=='greater' and (
                timestamp(future['close_time']).timestamp()==ref['close_epoch'] and
                decimal(future.get('floor_strike'))==Decimal(ref['floor'])):
            pair = quote_pair(future)
            if pair:
                bid,ask=pair
                first_bid,first_ask=anchors[ref['ticker']]
                label={'ask_up':ask>=first_ask+2,'bid_down':bid<=first_bid-2,
                       'wide_spread':ask-bid>3,'mid_up':bid+ask>first_bid+first_ask}.get(target)
        by_target[target]['missing' if label is None else 'yes' if label else 'no']+=1
    return {'asset':asset,'question_count':len(questions),
            'observed_labels':sum(v['yes']+v['no'] for v in by_target.values()),
            'eligible_horizons':sum(v is not None for v in coverage.values()),
            'by_target':by_target,'request_hash':request_hash,
            'provider_calls':0}

def write_new(path, value):
    # Publish a complete file atomically and reject accidental overwrites.
    path.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.NamedTemporaryFile(dir=path.parent, delete=False, mode='w', encoding='utf-8') as f:
        tmp = Path(f.name)
        try:
            json.dump(value, f, ensure_ascii=False, allow_nan=False, indent=2)
            f.write('\n')
            f.flush()
            os.fsync(f.fileno())
            os.link(tmp,path)
        finally:
            tmp.unlink(missing_ok=True)


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    cmds=parser.add_subparsers(dest='command',required=True)
    c=cmds.add_parser('catalog'); c.add_argument('--output',required=True,type=Path)
    p=cmds.add_parser('prepare')
    p.add_argument('--snapshot',required=True,type=Path)
    p.add_argument('--asset',required=True,choices=SERIES)
    p.add_argument('--mode',required=True,choices=['primary','stress'])
    p.add_argument('--output',required=True,type=Path)
    r=cmds.add_parser('replay')
    r.add_argument('--baseline',required=True,type=Path)
    r.add_argument('--followups',nargs='+',required=True,type=Path)
    r.add_argument('--asset',required=True,choices=SERIES)
    combo=cmds.add_parser('portfolio')
    for asset in SERIES:combo.add_argument('--'+asset.lower(),required=True,type=Path)
    combo.add_argument('--output',required=True,type=Path)
    grade=cmds.add_parser('replay-portfolio')
    grade.add_argument('--checkpoint-dir',required=True,type=Path)
    grade.add_argument('--output',required=True,type=Path)
    args=parser.parse_args()
    try:
        if args.command=='catalog':
            values=catalog(); write_new(args.output,values); print(f'{len(values)} feature definitions')
        elif args.command=='replay-portfolio':
            series={}
            for a in SERIES:
                paths=sorted(args.checkpoint_dir.glob(a+'-[0-9][0-9].json'))
                if not paths or paths[0].name!=a+'-00.json' or len(paths)>64 or any(p.stat().st_size>4_000_000 for p in paths):
                    raise ValueError('Missing opening snapshot or oversized checkpoint inventory')
                series[a]=[json.loads(p.read_text()) for p in paths]
            result=replay_portfolio({a:rows[0] for a,rows in series.items()}, {a:rows[1:] for a,rows in series.items()})
            write_new(args.output,result)
            print(f"{result['observed_labels']} observable labels across {result['question_count']} questions; zero model predictions")
        elif args.command=='portfolio':
            paths={a:getattr(args,a.lower()) for a in SERIES}
            if any(p.stat().st_size>4_000_000 for p in paths.values()):raise ValueError('Oversized snapshot')
            value=portfolio({a:json.loads(p.read_text()) for a,p in paths.items()})
            write_new(args.output,value)
            print(f"{len(value['request']['questions'])} cross-asset questions prepared locally; zero API calls")
        elif args.command=='replay':
            if len(args.followups)>64 or any(p.stat().st_size>4_000_000 for p in [args.baseline,*args.followups]):
                raise ValueError('Replay exceeds the bounded checkpoint inventory')
            result=replay(json.loads(args.baseline.read_text()),[json.loads(p.read_text()) for p in args.followups],args.asset)
            print(json.dumps(result,sort_keys=True))
        else:
            if args.snapshot.stat().st_size>4_000_000:
                raise ValueError('Snapshot exceeds the hourly listing bound')
            value=prepare(json.loads(args.snapshot.read_text()),args.asset,args.mode)
            write_new(args.output,value)
            print(f"{len(value['request']['questions'])} questions prepared locally; zero API calls")
    except (ValueError, KeyError, OSError, TypeError) as e:
        parser.exit(1, f'Cannot prepare research artifact: {e}\n')

if __name__=='__main__':
    main()
