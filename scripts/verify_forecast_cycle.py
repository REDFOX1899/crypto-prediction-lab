#!/usr/bin/env python3
"""Trusted operator audit of an automatic prospective cycle; no collection/backfill."""
import argparse
import json
from pathlib import Path
import subprocess
import urllib.request
from export_forecast_data import digest, ns
from verify_forecast_fit import clip, close, sigmoid, vector
from verify_rules_cycle import COLLECTOR, DOCUMENTS


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--open-time', required=True, type=int)
    p.add_argument('--model-receipt', required=True, type=Path)
    p.add_argument('--output', required=True, type=Path)
    args = p.parse_args()
    assert args.open_time % 900 == 0
    root = Path(__file__).resolve().parents[1]
    assert root not in args.output.resolve().parents, 'Keep operational proof outside repository'
    receipt = json.loads(args.model_receipt.read_text())
    access = subprocess.check_output(['gcloud','auth','print-access-token'],text=True).strip()
    identity = subprocess.check_output(['gcloud','auth','print-identity-token'],text=True).strip()
    def get(url, token):
        with urllib.request.urlopen(urllib.request.Request(url,headers={'Authorization':'Bearer '+token}),timeout=25) as response:
            return json.load(response)
    originals = []
    results = []
    for asset in ('BTC','ETH','SOL'):
        source_url = DOCUMENTS + '/cycle_snapshots_v1/s%s_%s_opening' % (args.open_time,asset)
        review_url = COLLECTOR + '/research/decision?asset=%s&open_time=%s' % (asset,args.open_time)
        source, review = get(source_url,access), get(review_url,identity)
        saved = review['decision']; record = saved['record']; packet = record['packet']; proof = record['forecast_proof']; model = proof['model']
        assert record['status'] == 'forecast'
        assert source['createTime'] == source['updateTime'] == packet['source_created_at']
        assert ns(source['createTime']) <= ns(packet['cutoff']) <= ns(saved['committed_at']) < (args.open_time+40)*10**9
        assert proof['model_committed_at'] == receipt['timeCreated'] and proof['generation'] == receipt['generation']
        assert proof['object_name'] == receipt['name'] == 'forecast_models/v1/'+digest(model)+'.json'
        assert ns(proof['model_committed_at']) <= ns(packet['cutoff'])
        assert ns(model['training_max_label_available_at']) <= ns(model['train_before'])
        assert ns(model['calibration_max_label_available_at']) <= ns(model['calibrate_before']) <= ns(proof['model_committed_at'])
        assert record['packet_hash'] == proof['packet_hash'] == digest(packet)
        assert record['forecast_hash'] == digest(proof)
        x = vector(packet)
        z = sum(b*(1 if j==0 else max(-10,min(10,(x[j]-model['mean'][j])/model['scale'][j]))) for j,b in enumerate(model['coefficients']))
        close(proof['predictions'][0],clip(sigmoid(z)))
        close(proof['predictions'][1],clip(sigmoid(model['calibration'][0]+model['calibration'][1]*z)))
        assert review['link']['record']['status'] == 'eligible_for_paper_testing'
        originals.append((source_url,review_url,source,review))
        results.append({'asset':asset,'forecast':'committed_and_arithmetic_verified','model_id':proof['object_name'],
                        'source_available_at':source['createTime'],'cutoff':packet['cutoff'],'committed_at':saved['committed_at'],
                        'commit_delay_ms':(ns(saved['committed_at'])-args.open_time*10**9)/10**6,
                        'runtime_revision':record['runtime_revision'],'later_link':'timing_eligible_not_fill'})
    request = urllib.request.Request(COLLECTOR+'/run-decisions', data=json.dumps({'open_time':args.open_time}).encode(),
                                     headers={'Authorization':'Bearer '+identity,'Content-Type':'application/json'})
    with urllib.request.urlopen(request,timeout=30) as response:
        assert response.status == 200
    for source_url,review_url,source,review in originals:
        assert get(source_url,access) == source and get(review_url,identity) == review
    result = {'schema_version':1,'open_time':args.open_time,'assets':results,'duplicate_preserved_sources_forecast_values_hashes_and_links':True}
    args.output.write_text(json.dumps(result,indent=2)+'\n')
    print(json.dumps({'status':'passed','assets':3,'forecasts':'timely_immutable_and_cutoff_safe','later_links':3}))


if __name__ == '__main__':
    main()
