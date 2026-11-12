#!/usr/bin/env python3
"""Materialize the 1,000 proposed feature slots from a bounded private book capture.

Each file has one observed REST book. The local manifest supplies an availability
timestamp observed after the individual file was flushed. This is receipt-clock
research; exchange publication time is unknown.
"""
import argparse
from collections import Counter
from datetime import datetime, timezone
from math import sqrt
from pathlib import Path
import json
import re
import time
from gemini_research import SERIES, STATS, STREAMS, WINDOWS, decimal, digest, timestamp, write_new

MAX_AGE_SECONDS = 6
ALL_STREAMS = [name for family in STREAMS.values() for name in family]


def side(levels):
    if not isinstance(levels,list) or not 1<=len(levels)<=10:
        return None
    parsed = []
    for pair in levels:
        if not isinstance(pair,list) or len(pair)!=2:
            return None
        price,qty=map(decimal,pair)
        if price is None or qty is None or not (0<price<1 and 0<qty<=10**15):
            return None
        parsed.append((price,qty))
    if len({p for p,_ in parsed})!=len(parsed):
        return None
    return sorted(parsed,reverse=True)


def book_values(book):
    yes, no = side(book.get('yes')),side(book.get('no'))
    if yes is None or no is None:
        return {}
    bid, ask = 100*yes[0][0],100*(1-no[0][0])
    if bid>=ask:
        return {}
    y, n = yes[0][1],no[0][1]
    y10,n10=sum(q for _,q in yes),sum(q for _,q in no)
    values = {
        'yes_bid_cents':bid,'yes_ask_cents':ask,'yes_mid_cents':(bid+ask)/2,'yes_spread_cents':ask-bid,
        'microprice_cents':(ask*y+bid*n)/(y+n),
        'yes_best_size':y,'no_best_size':n,
        'yes_top3_size':sum(q for _,q in yes[:3]),'no_top3_size':sum(q for _,q in no[:3]),
        'yes_top10_size':y10,'no_top10_size':n10,
        'best_imbalance':(y-n)/(y+n),'top10_imbalance':(y10-n10)/(y10+n10),
    }
    return {k:float(v) for k,v in values.items()}


def load_manifest(path, asset):
    manifest=json.loads(path.read_text())
    if not isinstance(manifest,list) or len(manifest)>5000:
        raise ValueError('Invalid or oversized manifest')
    result=[]
    names=set()
    for entry in manifest:
        name=entry['file']
        if not re.fullmatch(r'(BTC|ETH|SOL)-[0-9]{3,5}\.json',name) or name in names:
            raise ValueError('Invalid or duplicate capture filename')
        names.add(name)
        if not name.startswith(asset+'-'):
            continue
        record=json.loads((path.parent/name).read_text())
        if 'sha256' in entry and digest(record)!=entry['sha256']:
            raise ValueError('Capture content does not match its manifest')
        if record['asset']!=asset or not re.fullmatch('KX'+asset+r'15M-[A-Z0-9-]+',record['ticker']):
            raise ValueError('Record identity mismatch')
        started,received,available,close=[timestamp(v).timestamp() for v in
            [record['request_started_at'],record['received_at'],entry['observed_durable_at'],record['close']]]
        if not started<=received<=available or received>=close or started<close-900:
            raise ValueError('Record time outside contract or out of order')
        result.append({'ticker':record['ticker'],'t':received,'available':available,'close':close,'book':record['book']})
    return result


def points_at(rows,cutoff):
    eligible=[r for r in rows if r['available']<=cutoff and r['t']<=cutoff<r['close']]
    if len({(r['ticker'],r['close']) for r in eligible})>1:
        raise ValueError('Mixed active contracts')
    observations=[]
    previous=None
    for row in sorted(eligible,key=lambda r:r['t']):
        if previous and row['t']<=previous['t']:
            raise ValueError('Ambiguous repeated receipt timestamp')
        features=book_values(row['book'])
        if previous and row['t']-previous['t']<=MAX_AGE_SECONDS and previous['values'] and features:
            for change,metric in [('yes_mid_change_cents','yes_mid_cents'),('yes_spread_change_cents','yes_spread_cents'),
                                  ('yes_top10_size_change','yes_top10_size'),('no_top10_size_change','no_top10_size')]:
                features[change]=features[metric]-previous['values'][metric]
        # A prior update can become durable after the next update. Its change
        # becomes eligible only once both observations were actually available.
        change_available = max(previous['available'], row['available']) if previous else row['available']
        previous = {'t': row['t'], 'available': row['available'], 'values': features,
                    'change_available': change_available}
        observations.append(previous)
    return observations


def materialize(rows, cutoff):
    points=points_at(rows,cutoff)
    # Fix the decision grid at whole seconds at or before cutoff. For every
    # historical grid instant enforce receipt and durable availability as of it.
    end=int(cutoff)
    by_available=sorted(points,key=lambda p:(p['available'],p['t']))
    active=None; index=0; grid=[]
    for second in range(end-max(WINDOWS)+1,end+1):
        while index<len(by_available) and by_available[index]['available']<=second:
            candidate=by_available[index]; index+=1
            if active is None or candidate['t']>active['t']:
                active=candidate
        grid.append(active if active and 0<=second-active['t']<=MAX_AGE_SECONDS else None)
    output={}; reasons=Counter(); valid=0
    for stream in ALL_STREAMS:
        for seconds in WINDOWS:
            lookback=grid[-seconds:]
            samples=[p for second,p in enumerate(lookback, end-seconds+1)
                     if p and stream in p['values'] and
                     (stream not in STREAMS['consecutive_books'] or p['change_available']<=second)]
            distinct=len({p['t'] for p in samples})
            reason=None
            if stream in STREAMS['reference'] or stream in STREAMS['cross_asset_reference']:
                reason='reference_source_absent'
            elif len(samples)<seconds:
                reason='incomplete_fresh_window'
            elif distinct<2:
                reason='fewer_than_two_receipts'
            if reason:
                metrics=[None]*len(STATS); reasons[reason]+=len(metrics)
            else:
                vals=[p['values'][stream] for p in samples]
                mean=sum(vals)/seconds
                metrics=[mean,sqrt(sum((v-mean)**2 for v in vals)/seconds),min(vals),max(vals),vals[-1]-vals[0]]
                valid+=len(metrics)
            output[f'{stream}.{seconds}s']={'values':dict(zip(STATS,metrics)), 'observed_seconds':len(samples),
                                             'distinct_receipts':distinct,'reason':reason}
    return {'schema_version':1,'clock_basis':'receipt_and_local_durability','cutoff_utc':datetime.fromtimestamp(cutoff,timezone.utc).isoformat(),
            'max_age_seconds':MAX_AGE_SECONDS,'observation_count':len(points),'defined_metrics':valid,
            'missing_metrics':dict(reasons),'windows':output}


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--manifest',required=True,type=Path)
    parser.add_argument('--asset',choices=SERIES,required=True)
    parser.add_argument('--cutoff',required=True)
    parser.add_argument('--output',required=True,type=Path)
    args=parser.parse_args()
    rows=load_manifest(args.manifest,args.asset)
    cutoff=timestamp(args.cutoff).timestamp()
    start=time.perf_counter()
    result=materialize(rows,cutoff)
    elapsed_ms=(time.perf_counter()-start)*1000
    result['asset']=args.asset
    write_new(args.output,result)
    print(json.dumps({'asset':args.asset,'defined':result['defined_metrics'],'missing':result['missing_metrics'],
                      'local_feature_ms':round(elapsed_ms,2),'provider_calls':0}))

if __name__=='__main__':
    main()
