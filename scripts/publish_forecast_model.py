#!/usr/bin/env python3
"""Publish exactly one immutable, hashed private forecast artifact. No IAM changes."""
import argparse
import json
import os
from pathlib import Path
import re
import subprocess
import urllib.parse
import urllib.request
from export_forecast_data import BUCKET, digest, ns


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('fit_directory', type=Path)
    parser.add_argument('--receipt', required=True, type=Path)
    args = parser.parse_args()
    model = json.loads((args.fit_directory / 'model.json').read_text())
    report = json.loads((args.fit_directory / 'report.json').read_text())
    name = 'forecast_models/v1/' + digest(model) + '.json'
    assert re.fullmatch(r'forecast_models/v1/[a-f0-9]{64}\.json', name)
    assert report['model_id'] == name and report['paper_order_eligible'] is False
    assert ns(model['training_max_label_available_at']) <= ns(model['train_before'])
    assert ns(model['calibration_max_label_available_at']) <= ns(model['calibrate_before'])
    body = json.dumps(model, sort_keys=True, separators=(',', ':'), allow_nan=False).encode()
    assert len(body) <= 65536
    assert not args.receipt.exists(), 'Refusing to overwrite publication receipt'
    root = Path(__file__).resolve().parents[1]
    assert root not in args.receipt.resolve().parents, 'Keep operational receipt outside repository'
    token = subprocess.check_output(['gcloud', 'auth', 'print-access-token'], text=True).strip()
    query = urllib.parse.urlencode({'uploadType':'media','name':name,'ifGenerationMatch':'0'})
    request = urllib.request.Request('https://storage.googleapis.com/upload/storage/v1/b/' + BUCKET + '/o?' + query, data=body,
                                     headers={'Authorization':'Bearer '+token,'Content-Type':'application/json'})
    with urllib.request.urlopen(request, timeout=30) as response:
        meta = json.load(response)
    assert meta['name'] == name and ns(meta['timeCreated']) >= ns(model['calibrate_before'])
    # Read exactly the returned generation and verify its canonical content hash.
    query = urllib.parse.urlencode({'alt':'media','generation':meta['generation']})
    request = urllib.request.Request('https://storage.googleapis.com/storage/v1/b/' + BUCKET + '/o/' + urllib.parse.quote(name,safe='') + '?' + query, headers={'Authorization':'Bearer '+token})
    with urllib.request.urlopen(request, timeout=30) as response:
        assert digest(json.load(response)) == digest(model)
    receipt = {k:meta[k] for k in ('name','generation','timeCreated','bucket')}
    with os.fdopen(os.open(args.receipt, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600), 'w') as output:
        output.write(json.dumps(receipt,indent=2)+'\n')
    print(json.dumps({'status':'published_and_generation_verified',**receipt}))


if __name__ == '__main__':
    main()
