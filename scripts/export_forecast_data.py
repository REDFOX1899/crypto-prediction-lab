#!/usr/bin/env python3
"""Bounded private training extract. No provider collection or cloud writes.

Only immutable committed opening packets and separately versioned official labels
enter the normalized dataset. Current mutable label versions become available no
sooner than their last durable update; old label versions are not reconstructed.
"""
import argparse
from concurrent.futures import ThreadPoolExecutor
from datetime import date, datetime, timedelta, timezone
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import urllib.parse
import urllib.request
from build_quote_study import Firestore, ROOT

BUCKET = 'crypto-prediction-lab-research-308289525742'
ASSETS = ('BTC', 'ETH', 'SOL')


def stamp(seconds):
    return datetime.fromtimestamp(seconds, timezone.utc).isoformat().replace('+00:00', 'Z')


def ns(text):
    match = re.fullmatch(r'(.*T\d\d:\d\d:\d\d)(?:\.(\d{1,9}))?(Z|[+-]\d\d:\d\d)', text)
    if not match:
        raise ValueError('Invalid timestamp')
    base = datetime.fromisoformat(match[1] + match[3].replace('Z', '+00:00'))
    return int(base.timestamp()) * 10**9 + int((match[2] or '').ljust(9, '0'))


def digest(value):
    return hashlib.sha256(json.dumps(value, sort_keys=True, separators=(',', ':'), allow_nan=False).encode()).hexdigest()


def fetch(start, end):
    db = Firestore()
    anchor = db.request(':runQuery', {'structuredQuery': {'from': [{'collectionId': 'cycle_snapshots_v1'}], 'select': {'fields': []}, 'limit': 1}})
    read_time = anchor[-1]['readTime']
    start_ts = int(datetime.combine(start, datetime.min.time(), timezone.utc).timestamp())
    end_ts = min(int(datetime.combine(end, datetime.min.time(), timezone.utc).timestamp()), ns(read_time) // 10**9)
    token = subprocess.check_output(['gcloud', 'auth', 'print-access-token'], text=True).strip()
    headers = {'Authorization': 'Bearer ' + token}
    objects = []
    page = ''
    base = 'https://storage.googleapis.com/storage/v1/b/' + BUCKET + '/o'
    for _ in range(4):
        query = {'prefix': 'decisions/v1/', 'startOffset': 'decisions/v1/s%d_' % start_ts,
                 'endOffset': 'decisions/v1/s%d_' % end_ts, 'maxResults': '1000',
                 'fields': 'items(name,generation,timeCreated),nextPageToken'}
        if page:
            query['pageToken'] = page
        with urllib.request.urlopen(urllib.request.Request(base + '?' + urllib.parse.urlencode(query), headers=headers), timeout=30) as response:
            listing = json.load(response)
        for item in listing.get('items', []):
            match = re.fullmatch(r'decisions/v1/s([0-9]+)_KX(BTC|ETH|SOL)15M_market_midpoint_v1.json', item['name'])
            if match and start_ts <= int(match[1]) < end_ts and ns(item['timeCreated']) <= ns(read_time):
                objects.append(item)
        page = listing.get('nextPageToken', '')
        if not page:
            break
    if page or len(objects) > 2016:
        raise ValueError('Bounded decision inventory exceeded')

    def load(item):
        url = base + '/' + urllib.parse.quote(item['name'], safe='') + '?' + urllib.parse.urlencode({'alt': 'media', 'generation': item['generation']})
        with urllib.request.urlopen(urllib.request.Request(url, headers=headers), timeout=25) as response:
            raw = response.read(65537)
        if len(raw) > 65536:
            raise ValueError('Private record exceeds limit')
        record = json.loads(raw)
        # Do not pass later rule evidence, links, raw model output or provider
        # payloads through the training feature interface.
        return {'object_name': item['name'], 'generation': item['generation'], 'committed_at': item['timeCreated'],
                'run_id': record['run_id'], 'asset': record['asset'], 'open_ts': record['open_ts'],
                'status': record['status'], 'packet': record.get('packet'), 'packet_hash': record.get('packet_hash')}

    with ThreadPoolExecutor(max_workers=6) as executor:
        decisions = list(executor.map(load, objects))
    labels = []
    day = start
    while day < end:
        left = int(datetime.combine(day, datetime.min.time(), timezone.utc).timestamp())
        for asset in ASSETS:
            query = {'from': [{'collectionId': 'contracts_' + asset.lower()}],
                     'select': {'fields': [{'fieldPath': 'summary'}, {'fieldPath': 'close_time'}]},
                     'where': {'compositeFilter': {'op': 'AND', 'filters': [
                         {'fieldFilter': {'field': {'fieldPath': 'close_time'}, 'op': 'GREATER_THAN', 'value': {'timestampValue': stamp(left)}}},
                         {'fieldFilter': {'field': {'fieldPath': 'close_time'}, 'op': 'LESS_THAN_OR_EQUAL', 'value': {'timestampValue': stamp(left + 86400)}}}]}}, 'limit': 97}
            rows = db.request(':runQuery', {'structuredQuery': query, 'readTime': read_time})
            docs = [r['document'] for r in rows if 'document' in r]
            if len(docs) > 96:
                raise ValueError('Too many labels in an asset-day')
            for doc in docs:
                summary = json.loads(doc['fields']['summary']['stringValue'])
                ticker = summary['ticker']
                opened = ns(summary['open_time']) // 10**9
                closed = ns(summary['close_time']) // 10**9
                if (summary['asset'] != asset or not re.fullmatch('KX' + asset + r'15M-[A-Za-z0-9-]+', ticker)
                        or doc['name'] != ROOT + '/contracts_' + asset.lower() + '/' + ticker
                        or closed - opened != 900 or opened % 900 or not left <= opened < left + 86400
                        or closed * 10**9 != ns(doc['fields']['close_time']['timestampValue'])):
                    raise ValueError('Official label identity mismatch')
                if summary['result'] not in ('yes', 'no'):
                    continue
                available = max((doc['updateTime'], summary['captured_at']), key=ns)
                if ns(available) > ns(read_time):
                    continue
                labels.append({'asset': asset, 'ticker': ticker, 'open_ts': opened, 'close_ts': closed,
                               'yes': summary['result'] == 'yes', 'available_at': available,
                               'source_id': doc['name'], 'source_version': doc['updateTime'],
                               'version_hash': digest({'result': summary['result'], 'source_id': doc['name'], 'version': doc['updateTime']})})
        day += timedelta(days=1)
    return {'schema_version': 1, 'read_time': read_time, 'start_ts': start_ts, 'end_ts': end_ts,
            'expected_completed_asset_windows': max(0, (end_ts - 900 - start_ts) // 900 + 1) * 3,
            'decisions': sorted(decisions, key=lambda r: (r['open_ts'], r['asset'])),
            'labels': sorted(labels, key=lambda r: (r['open_ts'], r['asset']))}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--start', required=True, type=date.fromisoformat)
    parser.add_argument('--end-exclusive', required=True, type=date.fromisoformat)
    parser.add_argument('--output', required=True, type=Path)
    args = parser.parse_args()
    if not 1 <= (args.end_exclusive - args.start).days <= 7:
        parser.error('Select one to seven UTC days')
    path = args.output.resolve()
    root = Path(__file__).resolve().parents[1]
    if root in path.parents and root / 'data' not in path.parents:
        parser.error('Use ignored data or a private path outside the repository')
    if path.exists():
        parser.error('Refusing to overwrite frozen dataset')
    value = fetch(args.start, args.end_exclusive)
    path.parent.mkdir(parents=True, exist_ok=True)
    with os.fdopen(os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600), 'w') as output:
        output.write(json.dumps(value, sort_keys=True, separators=(',', ':')) + '\n')
    labels = {(x['asset'], x['open_ts']): x for x in value['labels']}
    counts = {a: {'decisions': 0, 'forecast_packets': 0, 'matched_labels': 0} for a in ASSETS}
    for row in value['decisions']:
        c = counts[row['asset']]
        c['decisions'] += 1
        c['forecast_packets'] += row['status'] == 'forecast'
        c['matched_labels'] += row['status'] == 'forecast' and (row['asset'], row['open_ts']) in labels
    print(json.dumps({'read_time': value['read_time'], 'counts': counts, 'private_extract_written': True}))


if __name__ == '__main__':
    main()
