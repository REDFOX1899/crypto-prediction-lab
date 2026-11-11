#!/usr/bin/env python3
"""Trusted operator: verify a prospective cycle and duplicate preservation.

Reads this app's private source/review surfaces. Does not collect or backfill.
Outputs only aggregate evidence; cloud credentials remain in memory.
"""
import argparse
from datetime import datetime, timezone
import json
from pathlib import Path
import re
import subprocess
import urllib.request

COLLECTOR = 'https://crypto-history-collector-a53petnlfa-uc.a.run.app'
DOCUMENTS = 'https://firestore.googleapis.com/v1/projects/gitunderstand/databases/crypto-prediction-lab/documents'


def ns(value):
    match = re.fullmatch(r'(.*T\d\d:\d\d:\d\d)(?:\.(\d{1,9}))?Z', value)
    if not match:
        raise ValueError('Expected UTC RFC3339 timestamp')
    base = datetime.strptime(match.group(1), '%Y-%m-%dT%H:%M:%S').replace(tzinfo=timezone.utc)
    return int(base.timestamp()) * 1000000000 + int((match.group(2) or '').ljust(9, '0'))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--open-time', required=True, type=int)
    parser.add_argument('--output', required=True, type=Path)
    args = parser.parse_args()
    if args.open_time % 900:
        parser.error('Expected aligned quarter-hour')
    root = Path(__file__).resolve().parents[1]
    output = args.output.resolve()
    if root in output.parents and root / 'data' not in output.parents:
        parser.error('Keep local audit output in ignored data or outside the repository')
    access = subprocess.check_output(['gcloud', 'auth', 'print-access-token'], text=True).strip()
    identity = subprocess.check_output(['gcloud', 'auth', 'print-identity-token'], text=True).strip()

    def get(url, token):
        request = urllib.request.Request(url, headers={'Authorization': 'Bearer ' + token})
        with urllib.request.urlopen(request, timeout=25) as response:
            return json.load(response)

    originals = {}
    rows = []
    for asset in ('BTC', 'ETH', 'SOL'):
        path = '/cycle_snapshots_v1/s%s_%s_opening' % (args.open_time, asset)
        doc = get(DOCUMENTS + path, access)
        review_url = COLLECTOR + '/research/decision?asset=%s&open_time=%s' % (asset, args.open_time)
        review = get(review_url, identity)
        saved = review['decision']
        record = saved['record']
        packet = record['packet']
        rules = record['market_rules']
        assert record['status'] == 'forecast' and rules['status'] == 'verified_asof_cutoff'
        assert doc['createTime'] == doc['updateTime'] == packet['source_created_at'] == rules['source_created_at']
        assert rules['cutoff'] == packet['cutoff'] == record['cutoff']
        assert ns(doc['createTime']) <= ns(record['cutoff']) <= ns(saved['committed_at'])
        assert ns(saved['committed_at']) < (args.open_time + 40) * 1000000000
        raw = json.loads(doc['fields']['payload']['stringValue'])['observation']['market_rules']
        normalized = json.loads(json.dumps(raw))
        for key in ('series_changes', 'event_changes'):
            normalized[key]['data'] = [r for r in raw[key]['data'] if ns(r['at']) <= ns(record['cutoff'])]
        assert normalized == rules['evidence']
        receipts = []
        for key in ('series', 'series_changes', 'event_changes'):
            evidence = rules['evidence'][key]
            assert ns(evidence['request_started_at']) <= ns(evidence['received_at']) <= ns(packet['received_at'])
            assert ns(evidence['received_at']) <= ns(record['cutoff'])
            receipts.append(ns(evidence['received_at']))
        assert review['link']['record']['status'] == 'eligible_for_paper_testing'
        originals[asset] = (doc, review, review_url, DOCUMENTS + path)
        rows.append({'asset': asset, 'status': 'verified_asof_cutoff', 'source_available_at': doc['createTime'],
                     'cutoff': record['cutoff'], 'committed_at': saved['committed_at'],
                     'last_rule_receipt_delay_ms': (max(receipts) - args.open_time * 1000000000) / 1000000,
                     'source_delay_ms': (ns(doc['createTime']) - args.open_time * 1000000000) / 1000000,
                     'commit_delay_ms': (ns(saved['committed_at']) - args.open_time * 1000000000) / 1000000,
                     'runtime_revision': record['runtime_revision'], 'eligible_later_link': True})
    request = urllib.request.Request(COLLECTOR + '/run-decisions', data=json.dumps({'open_time': args.open_time}).encode(),
                                    headers={'Authorization': 'Bearer ' + identity, 'Content-Type': 'application/json'})
    with urllib.request.urlopen(request, timeout=30) as response:
        assert response.status == 200
    for doc, review, review_url, doc_url in originals.values():
        assert get(doc_url, access) == doc
        assert get(review_url, identity) == review
    result = {'schema_version': 1, 'open_time': args.open_time, 'assets': rows,
              'duplicate_preserved_sources_decisions_and_links': True}
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(json.dumps(result, indent=2) + '\n')
    print('Three verified pre-cutoff rule packets and timely commitments; duplicate preserved sources, decisions and links')


if __name__ == '__main__':
    main()
