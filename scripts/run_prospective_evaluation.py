#!/usr/bin/env python3
"""Trusted operator client. Reports and optional replay inputs stay private."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import urllib.error
import urllib.parse
import urllib.request
from export_forecast_data import BUCKET

COLLECTOR = 'https://crypto-history-collector-308289525742.us-central1.run.app'


def private_write(path, value):
    with os.fdopen(os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600), 'w') as out:
        json.dump(value, out, indent=2, allow_nan=False)


def read_object(ref, access):
    name = ref['name']
    assert re.fullmatch(r'prospective/(?:inputs|manifests)/v1/[a-f0-9]{64}/[a-f0-9]{64}\.json', name)
    assert re.fullmatch(r'[0-9]+', ref['generation'])
    query = urllib.parse.urlencode({'alt':'media','generation':ref['generation']})
    url = 'https://storage.googleapis.com/storage/v1/b/'+BUCKET+'/o/'+urllib.parse.quote(name,safe='')+'?'+query
    with urllib.request.urlopen(urllib.request.Request(url,headers={'Authorization':'Bearer '+access}),timeout=25) as response:
        raw = response.read(65537)
    assert len(raw) <= 65536
    # Hash original stored bytes: preserves Rust's exact JSON float formatting.
    assert hashlib.sha256(raw).hexdigest() == name.rsplit('/',1)[1][:-5]
    return json.loads(raw)


def download_input(envelope, access):
    report = envelope['report']
    manifest = read_object(report['input_manifest'],access)
    assert manifest['input_hash'] == report['input_hash'] and manifest['experiment_id'] == report['experiment_id']
    assert len(manifest['parts']) <= 128
    arrays = {k:[] for k in ('decisions','labels','previous_labels','rows')}
    header = None
    for part in manifest['parts']:
        value = read_object(part['object'],access)
        if part['kind'] == 'header':
            assert header is None and len(value['items']) == 1
            header = value['items'][0]
        else:
            arrays[part['kind']].extend(value['items'])
    assert header is not None
    for key in ('decisions','labels','previous_labels'):
        assert header[key] == []
        header[key] = arrays[key]
    return header, arrays['rows'], manifest


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--day', required=True)
    parser.add_argument('--mode', choices=['day','summary','get'], default='day')
    parser.add_argument('--output', required=True, type=Path)
    parser.add_argument('--download-input', action='store_true')
    args = parser.parse_args()
    if args.mode == 'summary' and args.download_input:
        parser.error('Summary references daily reports; download input from a daily report')
    from datetime import date
    date.fromisoformat(args.day)
    root = Path(__file__).resolve().parents[1]
    destination = args.output.resolve()
    assert root not in destination.parents or root/'data' in destination.parents, 'Use private ignored output'
    assert not destination.exists(), 'Refusing to overwrite operator evidence'
    identity = subprocess.check_output(['gcloud','auth','print-identity-token'],text=True).strip()
    if args.mode == 'get':
        request = urllib.request.Request(COLLECTOR+'/research/evaluation?'+urllib.parse.urlencode({'day':args.day}),headers={'Authorization':'Bearer '+identity})
    else:
        route = '/research/evaluate-day' if args.mode=='day' else '/research/evaluate-summary'
        request = urllib.request.Request(COLLECTOR+route,data=json.dumps({'day':args.day}).encode(),headers={'Authorization':'Bearer '+identity,'Content-Type':'application/json'})
    try:
        with urllib.request.urlopen(request,timeout=50) as response:
            value = json.load(response)
    except urllib.error.HTTPError as error:
        raise SystemExit('Evaluation HTTP %s; capture windows reserve the collector (retry later if 503). No inputs printed.' % error.code)
    destination.mkdir(mode=0o700,parents=True)
    private_write(destination/'envelope.json',value)
    if args.download_input:
        access = subprocess.check_output(['gcloud','auth','print-access-token'],text=True).strip()
        data,rows,manifest = download_input(value,access)
        private_write(destination/'input.json',data)
        private_write(destination/'rows.json',rows)
        private_write(destination/'input-manifest.json',manifest)
    report = value.get('report',{})
    print(json.dumps({'status':'saved_and_readable','mode':args.mode,'report_object':value.get('object'),
        'coverage':{a:{k:r[k] for k in ('expected','timely_forecasts','scored','unscored')} for a,r in report.get('evaluation',{}).items()},
        'evaluation_status':report.get('status'),'paper_order_eligible':report.get('paper_order_eligible')}))


if __name__ == '__main__':
    main()
