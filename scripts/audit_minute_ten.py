#!/usr/bin/env python3
"""Read-only, bounded minute-10 coverage audit and blinded packet projection."""
import argparse
from collections import Counter
from datetime import date, datetime, timedelta, timezone
from decimal import Decimal, InvalidOperation
import json
from pathlib import Path
import re

from build_quote_study import ASSETS, ROOT, Firestore, digest, encode, instant as archive_instant, midnight


def instant(value):
    # Rust captures nanoseconds. Python 3.9 accepts only up to microseconds.
    # Round availability upward so discarded precision never exposes data early.
    match = re.search(r"\.(\d{7,})(?=Z|[+-]\d\d:\d\d$)", value)
    rounded = False
    if match:
        fraction = match.group(1)
        value = value[:match.start(1)] + fraction[:6] + value[match.end(1):]
        rounded = any(c != '0' for c in fraction[6:])
    return archive_instant(value) + timedelta(microseconds=int(rounded))


def payload(doc):
    return json.loads(doc['fields']['payload']['stringValue'])


def valid_ticker(asset, ticker):
    return isinstance(ticker, str) and re.fullmatch('KX' + asset + r'15M-[A-Za-z0-9-]+', ticker)


def book_levels(levels):
    if not isinstance(levels, list) or not levels or len(levels) > 10:
        raise ValueError('missing_or_invalid_depth')
    prices = set()
    normalized = []
    for level in levels:
        if not isinstance(level, list) or len(level) != 2:
            raise ValueError('invalid_level')
        price, size = (Decimal(str(v)) for v in level)
        if (not price.is_finite() or not size.is_finite() or not 0 < price < 1
                or size <= 0 or price in prices):
            raise ValueError('invalid_level')
        prices.add(price)
        normalized.append([str(price), str(size)])
    return sorted(normalized, key=lambda v: Decimal(v[0]), reverse=True)


def observation(doc, asset, opened):
    """Validate only the prospective capture. Outcomes are never an input."""
    if doc is None:
        return None, 'missing_snapshot'
    try:
        row = payload(doc)
        if (doc['name'] != f'{ROOT}/minute_ten_v1/s{opened}_{asset}'
                or row['schema_version'] != 1 or row['asset'] != asset
                or instant(row['open_time']).timestamp() != opened
                or instant(row['cutoff']).timestamp() != opened + 600):
            return None, 'identity_mismatch'
        if row['status'] != 'available':
            return None, 'capture_' + str(row['status'])
        started, received, created = [instant(v) for v in
                                     (row['request_started_at'], row['received_at'], doc['createTime'])]
        if not opened + 600 <= started.timestamp() <= received.timestamp() < opened + 660:
            return None, 'invalid_capture_timing'
        obs = row['observation']
        if not valid_ticker(asset, obs['ticker']):
            return None, 'ticker_mismatch'
        target = Decimal(str(obs['target']))
        if not target.is_finite() or target <= 0:
            return None, 'invalid_target'
        yes, no = book_levels(obs['yes_bids']), book_levels(obs['no_bids'])
        bid, ask = Decimal(yes[0][0]), 1 - Decimal(no[0][0])
        if bid > ask:
            return None, 'crossed_book'
        # Legacy capture combines sequential metadata/book requests. Earliest durable
        # visibility is bounded conservatively by BOTH receipt and document creation.
        return {'ticker': obs['ticker'], 'target_usd': str(target), 'yes_bids_usd': yes,
                'no_bids_usd': no, 'yes_bid_usd': str(bid), 'yes_ask_usd': str(ask),
                'yes_midpoint_usd': str((bid + ask) / 2), 'spread_usd': str(ask - bid),
                'received_at': received.isoformat(), 'available_at': max(received, created).isoformat(),
                'receipt_delay_seconds': (received.timestamp() - opened - 600),
                'component_timing': 'unknown; sequential requests, combined receipt only',
                'source_age_seconds': None}, None
    except ValueError as err:
        reason = str(err)
        return None, reason if reason in ('missing_or_invalid_depth', 'invalid_level') else 'invalid_snapshot'
    except (KeyError, TypeError, InvalidOperation):
        return None, 'invalid_snapshot'


def model_packet(doc, asset, opened, cutoff):
    """Return a feature allowlist as of cutoff, with a content hash independent of labels."""
    if asset not in ASSETS or opened % 900:
        raise ValueError('Unsupported asset or window')
    cutoff = instant(cutoff)
    if not opened + 600 <= cutoff.timestamp() < opened + 900:
        raise ValueError('Cutoff must be from minute 10 until market close')
    obs, reason = observation(doc, asset, opened)
    packet = {'schema_version': 1, 'asset': asset, 'open_ts': opened,
              'cutoff': cutoff.isoformat(), 'status': 'unavailable', 'reason': reason, 'features': None}
    if obs is not None:
        if instant(obs['available_at']) > cutoff:
            packet['reason'] = 'not_yet_available'
        elif cutoff.timestamp() - instant(obs['received_at']).timestamp() > 60:
            packet['reason'] = 'stale_observation'
        else:
            packet.update(status='available', reason=None, features=obs)
    return dict(packet, packet_hash=digest(packet))


def official_result(doc, asset, opened):
    if doc is None:
        return None
    try:
        market = payload(doc)['market']
        summary = json.loads(doc['fields']['summary']['stringValue'])
        ticker = market['ticker']
        if (not valid_ticker(asset, ticker)
                or doc['name'] != f'{ROOT}/contracts_{asset.lower()}/{ticker}'
                or instant(market['open_time']).timestamp() != opened
                or instant(market['close_time']).timestamp() != opened + 900
                or summary['asset'] != asset or summary['ticker'] != ticker
                or market['result'] not in ('yes', 'no') or summary['result'] != market['result']):
            return None
        return {'ticker': ticker, 'result': market['result']}
    except (ValueError, KeyError, TypeError):
        return None


def percentiles(values):
    values = sorted(values)
    if not values:
        return {'n': 0, 'p50': None, 'p95': None, 'max': None}
    def nearest(p):
        import math
        return round(values[max(0, math.ceil(len(values) * p) - 1)], 6)
    return {'n': len(values), 'p50': nearest(.5), 'p95': nearest(.95), 'max': round(values[-1], 6)}


def report(extract):
    start, end = [midnight(date.fromisoformat(extract[k])) for k in ('start_day', 'end_day_exclusive')]
    asof = instant(extract['read_time']).timestamp()
    slots = list(range(int(start.timestamp()), int(end.timestamp()), 900))
    eligible = [t for t in slots if t + 900 <= asof]
    snapshots = {d['name']: d for d in extract['snapshots']}
    archives = {}
    for doc in extract['outcomes']:
        m = payload(doc)['market']
        opened = int(instant(m['open_time']).timestamp())
        asset = json.loads(doc['fields']['summary']['stringValue'])['asset']
        key = (asset, opened)
        if key in archives:
            raise ValueError('Duplicate archived asset/window')
        archives[key] = doc
    if len(snapshots) != len(extract['snapshots']):
        raise ValueError('Duplicate snapshot')
    result = {'schema_version': 1, 'start_day': extract['start_day'],
              'end_day_exclusive': extract['end_day_exclusive'], 'read_time': extract['read_time'],
              'extract_sha256': digest(extract), 'assets': {}}
    for asset in ASSETS:
        counts = Counter(expected_full_range=len(slots), completed_slots=len(eligible))
        reasons, delays, ages = Counter(), [], []
        saved_slots = []
        daily = {}
        for opened in eligible:
            doc = snapshots.get(f'{ROOT}/minute_ten_v1/s{opened}_{asset}')
            obs, reason = observation(doc, asset, opened)
            label = official_result(archives.get((asset, opened)), asset, opened)
            day = midnight_timestamp(opened)[:10]
            bucket = daily.setdefault(day, {'completed_slots': 0, 'saved_snapshots': 0,
                                           'usable_books': 0, 'official_results': 0, 'matched_pairs': 0})
            bucket['completed_slots'] += 1
            bucket['saved_snapshots'] += int(doc is not None)
            bucket['usable_books'] += int(obs is not None)
            bucket['official_results'] += int(label is not None)
            bucket['matched_pairs'] += int(obs is not None and label is not None and obs['ticker'] == label['ticker'])
            if doc is not None:
                saved_slots.append(opened)
            counts['saved_snapshots'] += doc is not None
            counts['official_results'] += label is not None
            counts['usable_books'] += obs is not None
            exact = model_packet(doc, asset, opened, midnight_timestamp(opened + 600))
            counts['usable_at_exact_cutoff'] += exact['status'] == 'available'
            if obs:
                delays.append(obs['receipt_delay_seconds'])
                ages.append((instant(obs['available_at']) - instant(obs['received_at'])).total_seconds())
                counts['delayed_receipts'] += obs['receipt_delay_seconds'] > 0
                counts['usable_books_with_official_result'] += label is not None and label['ticker'] == obs['ticker']
                counts['unmatched_tickers'] += label is not None and label['ticker'] != obs['ticker']
                counts['usable_books_without_official_result'] += label is None
            else:
                reasons[reason] += 1
        keys = ('expected_full_range', 'completed_slots', 'saved_snapshots', 'official_results', 'usable_books',
                'usable_books_with_official_result', 'usable_at_exact_cutoff', 'delayed_receipts',
                'unmatched_tickers', 'usable_books_without_official_result')
        result['assets'][asset] = {**{k: counts[k] for k in keys}, 'book_exclusions': dict(sorted(reasons.items())), 'by_day': daily,
                                  'first_saved_open': midnight_timestamp(min(saved_slots)) if saved_slots else None,
                                  'missing_since_first_saved': sum(t >= min(saved_slots) and
                                      f'{ROOT}/minute_ten_v1/s{t}_{asset}' not in snapshots for t in eligible) if saved_slots else None,
                                  'receipt_delay_seconds': percentiles(delays),
                                  'age_at_durable_availability_seconds': percentiles(ages),
                                  'provider_source_age': 'unknown; source timestamp was not captured'}
    return result


def midnight_timestamp(seconds):
    return datetime.fromtimestamp(seconds, timezone.utc).isoformat()


def fetch(start, end):
    db = Firestore()
    anchor = db.request(':runQuery', {'structuredQuery': {'from': [{'collectionId': 'minute_ten_v1'}],
                                                        'select': {'fields': []}, 'limit': 1}})
    read_time = anchor[-1]['readTime']
    snapshots, outcomes = [], []
    day = start
    while day < end:
        for asset in ASSETS:
            names = [f'{ROOT}/minute_ten_v1/s{int(midnight(day).timestamp()) + i * 900}_{asset}' for i in range(96)]
            rows = db.request(':batchGet', {'documents': names, 'readTime': read_time,
                                           'mask': {'fieldPaths': ['payload']}})
            snapshots.extend(r['found'] for r in rows if 'found' in r)
            outcomes.extend(db.archived_day(asset, day, read_time))
        day += timedelta(days=1)
    return {'schema_version': 1, 'start_day': str(start), 'end_day_exclusive': str(end),
            'read_time': read_time, 'snapshots': sorted(snapshots, key=lambda d: d['name']),
            'outcomes': sorted(outcomes, key=lambda d: d['name'])}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--start', type=date.fromisoformat, required=True)
    parser.add_argument('--end-exclusive', type=date.fromisoformat, required=True)
    parser.add_argument('--extract', type=Path, required=True, help='Ignored local extract to create or replay')
    parser.add_argument('--fetch', action='store_true', help='Read GCP into a new local file')
    parser.add_argument('--report', type=Path, required=True)
    args = parser.parse_args()
    if not 1 <= (args.end_exclusive - args.start).days <= 7:
        parser.error('Select one to seven UTC days')
    if args.fetch:
        if args.extract.exists():
            parser.error('Refusing to overwrite a frozen extract')
        data = fetch(args.start, args.end_exclusive)
        args.extract.parent.mkdir(parents=True, exist_ok=True)
        args.extract.write_text(encode(data) + '\n')
    else:
        data = json.loads(args.extract.read_text())
    if (data['start_day'], data['end_day_exclusive']) != (str(args.start), str(args.end_exclusive)):
        parser.error('Extract date range mismatch')
    output = report(data)
    args.report.write_text(json.dumps(output, indent=2) + '\n')
    print(json.dumps(output, indent=2))


if __name__ == '__main__':
    main()
