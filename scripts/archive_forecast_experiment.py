#!/usr/bin/env python3
"""Durably preserve private replay inputs and evaluation separately from agent-readable models."""
import argparse
import json
import os
from pathlib import Path
import subprocess
import urllib.parse
import urllib.request
from export_forecast_data import BUCKET, digest
from verify_forecast_fit import verify


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('dataset', type=Path)
    parser.add_argument('fit_directory', type=Path)
    parser.add_argument('--model-receipt', required=True, type=Path)
    parser.add_argument('--output', required=True, type=Path)
    args = parser.parse_args()
    root = Path(__file__).resolve().parents[1]
    assert root not in args.output.resolve().parents and not args.output.exists()
    assert args.dataset.stat().st_size <= 16*1024*1024
    data = json.loads(args.dataset.read_text())
    model = json.loads((args.fit_directory/'model.json').read_text())
    report = json.loads((args.fit_directory/'report.json').read_text())
    verify(data,model,report)
    model_receipt = json.loads(args.model_receipt.read_text())
    assert model_receipt['name'] == report['model_id']
    token = subprocess.check_output(['gcloud','auth','print-access-token'],text=True).strip()
    def create(prefix, value):
        h = digest(value); name = prefix+'/'+h+'.json'
        raw = json.dumps(value,sort_keys=True,separators=(',',':'),allow_nan=False).encode()
        assert len(raw) <= 16*1024*1024
        query = urllib.parse.urlencode({'uploadType':'media','name':name,'ifGenerationMatch':'0'})
        req = urllib.request.Request('https://storage.googleapis.com/upload/storage/v1/b/'+BUCKET+'/o?'+query,
            data=raw,headers={'Authorization':'Bearer '+token,'Content-Type':'application/json'})
        with urllib.request.urlopen(req,timeout=30) as response:
            meta = json.load(response)
        url = 'https://storage.googleapis.com/storage/v1/b/'+BUCKET+'/o/'+urllib.parse.quote(name,safe='')+'?'+urllib.parse.urlencode({'alt':'media','generation':meta['generation']})
        with urllib.request.urlopen(urllib.request.Request(url,headers={'Authorization':'Bearer '+token}),timeout=30) as response:
            assert digest(json.load(response)) == h
        return {'name':name,'sha256':h,'generation':meta['generation'],'timeCreated':meta['timeCreated'],'bytes':len(raw)}
    dataset = create('forecast_datasets/v1',data)
    evaluation = create('forecast_evaluations/v1',report)
    experiment = create('forecast_experiments/v1',{'schema_version':1,'model':model_receipt,'dataset':dataset,'evaluation':evaluation,
        'visibility':'operator_evaluator_only; never supplied to MCP or live inference','retrospective':True})
    with os.fdopen(os.open(args.output,os.O_WRONLY|os.O_CREAT|os.O_EXCL,0o600),'w') as out:
        json.dump({'dataset':dataset,'evaluation':evaluation,'experiment':experiment},out,indent=2)
    print(json.dumps({'status':'archived_and_generation_verified','private_objects':3,'dataset_bytes':dataset['bytes'],'experiment_id':experiment['name']}))


if __name__ == '__main__':
    main()
