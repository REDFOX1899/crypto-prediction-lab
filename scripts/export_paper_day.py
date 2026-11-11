#!/usr/bin/env python3
"""Join one frozen operator evaluation to its original decision generations and execution books."""
import argparse
import json
from pathlib import Path
import re
import subprocess
import urllib.error
import urllib.parse
import urllib.request

from run_prospective_evaluation import BUCKET, private_write
from verify_rules_cycle import DOCUMENTS


def source_urls(saved):
    ref, record = saved['object'], saved['record']
    asset, opened = record['asset'], record['open_ts']
    if asset not in ('BTC', 'ETH', 'SOL') or not isinstance(opened, int) or opened % 900:
        raise ValueError('Invalid window')
    name = f'decisions/v1/s{opened}_KX{asset}15M_market_midpoint_v1.json'
    if ref['name'] != name or not re.fullmatch('[0-9]+', ref['generation']):
        raise ValueError('Invalid decision reference')
    query = urllib.parse.urlencode({'alt': 'media', 'generation': ref['generation']})
    url = 'https://storage.googleapis.com/storage/v1/b/'+BUCKET+'/o/'+urllib.parse.quote(name, safe='')+'?'+query
    return url, f'{DOCUMENTS}/cycle_snapshots_v1/s{opened}_{asset}_execution_candidate'


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('input', type=Path, help='Private input.json from run_prospective_evaluation.py')
    parser.add_argument('output', type=Path, help='New private bundle path outside the repository')
    args = parser.parse_args()
    if Path(__file__).resolve().parents[1] in args.output.resolve().parents or args.output.exists():
        parser.error('Use a new private output outside the repository')
    if args.input.stat().st_size > 16*1024*1024:
        parser.error('Expected one day')
    data = json.loads(args.input.read_bytes())
    if len(data['decisions']) > 288:
        parser.error('Expected at most 288 decisions')
    token = subprocess.check_output(['gcloud','auth','print-access-token'], text=True).strip()

    def read(url, optional=False):
        try:
            with urllib.request.urlopen(urllib.request.Request(url, headers={'Authorization':'Bearer '+token}), timeout=20) as response:
                raw = response.read(65537)
        except urllib.error.HTTPError as error:
            if optional and error.code == 404:
                return None
            raise
        if len(raw) > 65536:
            raise ValueError('Oversized observation')
        return json.loads(raw)

    evidence = {}
    for saved in data['decisions']:
        original, candidate = source_urls(saved)
        name = saved['object']['name']
        if name in evidence:
            raise ValueError('Duplicate decision')
        evidence[name] = {'record': read(original), 'candidate': read(candidate, optional=True)}
    private_write(args.output, {'evaluation':data, 'evidence':evidence})
    print(f'Saved {len(evidence)} original decisions with available execution evidence.')


if __name__ == '__main__':
    main()
