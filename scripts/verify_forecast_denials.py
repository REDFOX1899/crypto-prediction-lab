#!/usr/bin/env python3
"""Operator-only adversarial scope checks against deployed MCP. Deletes exact test generations."""
import argparse
import copy
from datetime import datetime, timezone
import hashlib
import json
from pathlib import Path
import secrets
import subprocess
import urllib.error
import urllib.parse
import urllib.request
from export_forecast_data import BUCKET, digest, ns
from verify_forecast_fit import clip, sigmoid, vector

MCP = 'https://crypto-research-mcp-308289525742.us-central1.run.app/mcp'


def stamp(t):
    sec, fraction = divmod(t, 10**9)
    return datetime.fromtimestamp(sec, timezone.utc).strftime('%Y-%m-%dT%H:%M:%S') + '.%09dZ' % fraction


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('scope', type=Path)
    parser.add_argument('old_scope', type=Path)
    parser.add_argument('--output', required=True, type=Path)
    args = parser.parse_args()
    root = Path(__file__).resolve().parents[1]
    assert root not in args.output.resolve().parents
    scope = json.loads(args.scope.read_text())
    old = json.loads(args.old_scope.read_text())
    assert 'forecast_proof' in scope and 'forecast_proof' not in old
    access = subprocess.check_output(['gcloud','auth','print-access-token'],text=True).strip()
    identity = subprocess.check_output(['gcloud','auth','print-identity-token'],text=True).strip()
    def late_model(s):
        s['forecast_proof']['model_committed_at'] = stamp(ns(s['packet']['cutoff'])+1)
    def late_train(s):
        m = s['forecast_proof']['model']; m['training_max_label_available_at'] = stamp(ns(m['train_before'])+1)
    def late_cal(s):
        m = s['forecast_proof']['model']; m['calibration_max_label_available_at'] = stamp(ns(m['calibrate_before'])+1)
    def wrong_probability(s):
        s['forecast_proof']['predictions'][0] = .9999
    def current_label(s):
        s['packet']['outcome'] = 'yes'
    def old_packet(s):
        s['packet'] = old['packet']; s['run_id'] = old['run_id']
        # Correct arithmetic and recomputed hashes, but model was unavailable then.
        p = s['packet']; m = s['forecast_proof']['model']; x = vector(p)
        z = sum(b*(1 if j==0 else max(-10,min(10,(x[j]-m['mean'][j])/m['scale'][j]))) for j,b in enumerate(m['coefficients']))
        s['forecast_proof']['predictions'] = [clip(sigmoid(z)),clip(sigmoid(m['calibration'][0]+m['calibration'][1]*z))]
    results = []
    base = 'https://storage.googleapis.com/storage/v1/b/'+BUCKET+'/o/'
    for label, mutate in [('late_model_commit',late_model),('late_training_label_version',late_train),('late_calibration_label_version',late_cal),('tampered_probability',wrong_probability),('injected_current_label',current_label),('backfilled_old_packet',old_packet)]:
        s = copy.deepcopy(scope); mutate(s)
        proof = s['forecast_proof']; proof['object_name'] = 'forecast_models/v1/'+digest(proof['model'])+'.json'
        s['packet_hash'] = proof['packet_hash'] = digest(s['packet']); s['forecast_hash'] = digest(proof)
        cap = secrets.token_urlsafe(32)
        name = 'mcp/scopes/v1/'+hashlib.sha256(cap.encode()).hexdigest()+'.json'
        query = urllib.parse.urlencode({'uploadType':'media','name':name,'ifGenerationMatch':'0'})
        request = urllib.request.Request('https://storage.googleapis.com/upload/storage/v1/b/'+BUCKET+'/o?'+query,
            data=json.dumps(s,separators=(',',':')).encode(),headers={'Authorization':'Bearer '+access,'Content-Type':'application/json'})
        with urllib.request.urlopen(request,timeout=25) as response:
            meta = json.load(response)
        try:
            init = {'jsonrpc':'2.0','id':1,'method':'initialize','params':{'protocolVersion':'2026-07-28','capabilities':{},'clientInfo':{'name':'forecast-denial-verifier','version':'1'}}}
            req = urllib.request.Request(MCP,data=json.dumps(init).encode(),headers={'Authorization':'Bearer '+identity,'X-Research-Capability':cap,'Content-Type':'application/json','Accept':'application/json, text/event-stream'})
            try:
                with urllib.request.urlopen(req,timeout=25) as response:
                    status = response.status
            except urllib.error.HTTPError as error:
                status = error.code
            assert status == 403, (label,status)
            results.append({'case':label,'http_status':status})
        finally:
            url = base+urllib.parse.quote(name,safe='')+'?'+urllib.parse.urlencode({'ifGenerationMatch':meta['generation']})
            request = urllib.request.Request(url,method='DELETE',headers={'Authorization':'Bearer '+access})
            with urllib.request.urlopen(request,timeout=25) as response:
                assert response.status == 204
    args.output.write_text(json.dumps({'status':'passed','cases':results,'temporary_generations_removed':True},indent=2)+'\n')
    print(json.dumps({'status':'passed','invalid_scopes_denied':len(results),'temporary_generations_removed':True}))


if __name__ == '__main__':
    main()
