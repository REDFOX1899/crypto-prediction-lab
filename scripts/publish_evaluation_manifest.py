#!/usr/bin/env python3
"""Register the fixed prospective protocol in the existing private bucket; no IAM changes."""
import argparse
import json
import os
from pathlib import Path
import subprocess
import urllib.parse
import urllib.request
from export_forecast_data import BUCKET, digest, ns


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('manifest', type=Path)
    parser.add_argument('--output', required=True, type=Path)
    args = parser.parse_args()
    root = Path(__file__).resolve().parents[1]
    assert root not in args.output.resolve().parents and not args.output.exists()
    value = json.loads(args.manifest.read_text())
    assert value['protocol'] == 'prospective_opening_eval_v1'
    assert value['minimum_complete_days'] == 30 and value['minimum_coverage_basis_points'] == 9500
    name = 'prospective/experiments/v1/' + digest(value) + '.json'
    token = subprocess.check_output(['gcloud','auth','print-access-token'],text=True).strip()
    headers = {'Authorization':'Bearer '+token}
    api = 'https://storage.googleapis.com/storage/v1/b/'+BUCKET+'/o/'
    with urllib.request.urlopen(urllib.request.Request(api+urllib.parse.quote(value['model_object'],safe=''),headers=headers),timeout=25) as response:
        model = json.load(response)
    assert model['generation'] == value['model_generation'] and ns(model['timeCreated']) == ns(value['model_committed_at'])
    query = urllib.parse.urlencode({'uploadType':'media','ifGenerationMatch':'0','name':name})
    req = urllib.request.Request('https://storage.googleapis.com/upload/storage/v1/b/'+BUCKET+'/o?'+query,data=json.dumps(value,sort_keys=True,separators=(',',':')).encode(),headers={**headers,'Content-Type':'application/json'})
    with urllib.request.urlopen(req,timeout=25) as response:
        meta = json.load(response)
    assert ns(meta['timeCreated']) < ns(value['confirmatory_start'])
    query = urllib.parse.urlencode({'alt':'media','generation':meta['generation']})
    with urllib.request.urlopen(urllib.request.Request(api+urllib.parse.quote(name,safe='')+'?'+query,headers=headers),timeout=25) as response:
        assert digest(json.load(response)) == digest(value)
    receipt = {'manifest':value,'object':{'name':name,'generation':meta['generation'],'committed_at':meta['timeCreated']}}
    with os.fdopen(os.open(args.output,os.O_CREAT|os.O_EXCL|os.O_WRONLY,0o600),'w') as out:
        json.dump(receipt,out,indent=2)
    print(json.dumps({'status':'registered_and_generation_verified','object':receipt['object'],'confirmatory_start':value['confirmatory_start']}))


if __name__ == '__main__':
    main()
