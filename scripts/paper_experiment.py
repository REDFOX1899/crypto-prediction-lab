#!/usr/bin/env python3
"""Private, on-demand economic experiment registration, archival and held-out comparison."""
import argparse
from datetime import date, datetime, timedelta, timezone
from decimal import Decimal
import hashlib
import json
from pathlib import Path
import random
import re
import subprocess
import urllib.error
import urllib.parse
import urllib.request

from run_prospective_evaluation import BUCKET, private_write

ROOT = 'paper_studies/v1'
SCHEMA = 'opening_ioc_study_v1'
J1 = '97616a6481c3a6cc1435afb6198770a93874e37f4cd6779169d8247d49fc7a64'
DAYS = 30
LIMIT = 32 * 1024 * 1024


def canonical(value):
    return json.dumps(value, separators=(',', ':'), sort_keys=True, allow_nan=False).encode()


def sha(raw):
    return hashlib.sha256(raw).hexdigest()


def private_output(path):
    if path.exists() or Path(__file__).resolve().parents[1] in path.resolve().parents:
        raise ValueError('Choose a new private output outside the repository')


def bounded_file(path):
    if path.stat().st_size > LIMIT:
        raise ValueError('Exceeds one-day file limit')
    return path.read_bytes()


def request(url, token, data=None):
    req = urllib.request.Request(url, data=data, headers={'Authorization':'Bearer '+token, 'Content-Type':'application/json'})
    with urllib.request.urlopen(req, timeout=45) as response:
        body = response.read(LIMIT+1)
    if len(body)>LIMIT:
        raise ValueError('Oversized response')
    return body


def store(kind, raw, token):
    name = f'{ROOT}/{kind}/{sha(raw)}.json'
    url = 'https://storage.googleapis.com/storage/v1/b/'+BUCKET+'/o/'+urllib.parse.quote(name, safe='')
    query = urllib.parse.urlencode({'uploadType':'media','ifGenerationMatch':'0','name':name})
    try:
        meta = json.loads(request('https://storage.googleapis.com/upload/storage/v1/b/'+BUCKET+'/o?'+query, token, raw))
    except urllib.error.HTTPError as error:
        if error.code != 412:
            raise
        meta = json.loads(request(url, token))
    if request(url+'?alt=media&generation='+meta['generation'], token)!=raw:
        raise ValueError('Stored bytes differ')
    return {'name':name,'generation':meta['generation'],'created':meta['timeCreated']}


def read_ref(ref, kind, token):
    name=ref['name']
    if not re.fullmatch(ROOT+'/'+kind+'/[a-f0-9]{64}'+r'\.json',name) or not re.fullmatch('[0-9]+',ref['generation']):
        raise ValueError('Invalid paper object reference')
    url='https://storage.googleapis.com/storage/v1/b/'+BUCKET+'/o/'+urllib.parse.quote(name,safe='')
    raw=request(url+'?alt=media&generation='+ref['generation'],token)
    if sha(raw)!=name.rsplit('/',1)[1][:-5]:
        raise ValueError('Changed paper object')
    return json.loads(raw)


def protocol(report, start):
    if report['schema']!=SCHEMA or report['forecast_experiment']!=J1 or not re.fullmatch('[a-f0-9]{64}',report['engine_sha256']):
        raise ValueError('Original frozen forecast and paper engine required')
    return {'schema':SCHEMA,'engine':report['engine_sha256'],'forecast_experiment':J1,'start':start.isoformat(),'days':DAYS,
            'portfolio':'one non-recycled virtual dollar per asset/window; 288 USD per strategy per UTC day',
            'primary':'point_scenario lower net minus market_favorite upper net per scheduled window',
            'secondary':'point_scenario versus zero; stressed model versus zero',
            'coverage':'Each asset each day >=95% committed forecasts, official scoreable results, verified pricing and eligible later books',
            'interval':'2000 whole UTC day bootstrap draws; seed 17; descriptive percentile interval',
            'decision':'Research only; no individual probability bounds or trade promotion'}


def summarize(manifest, reports):
    start=date.fromisoformat(manifest['start'])
    by_day={}
    for report in reports:
        day=date.fromisoformat(report['day'])
        if day in by_day or not start<=day<start+timedelta(days=DAYS) or protocol(report,start)!=manifest:
            raise ValueError('Duplicate day, wrong cohort, or changed experiment')
        by_day[day]=report
    daily=[]
    for offset in range(DAYS):
        day=start+timedelta(days=offset)
        report=by_day.get(day)
        if report is None:
            daily.append({'day':day.isoformat(),'status':'missing_day'})
            continue
        start_ts=int(datetime.combine(day, datetime.min.time(),tzinfo=timezone.utc).timestamp())
        slots={(c['asset'],c['open_ts']) for c in report['cycles']}
        expected={(a,start_ts+900*i) for a in ('BTC','ETH','SOL') for i in range(96)}
        covered=slots==expected and len(report['cycles'])==288
        for asset in ('BTC','ETH','SOL'):
            score=report['forecast_evaluation'][asset]
            eligible=sum(1 for c in report['cycles'] if c['asset']==asset and c.get('candidate_at') and c.get('scorecard',{}).get('YES',{}).get('modeled_fee') is not None)
            covered=covered and min(score['timely_forecasts'],score['scored'],eligible)>=92
        point=report['strategies']['point_scenario']; favorite=report['strategies']['market_favorite']; stress=report['strategies']['point_stress']
        contrast=(Decimal(point['net_bounds'][0])-Decimal(favorite['net_bounds'][1]))/288
        daily.append({'day':day.isoformat(),'status':'covered' if covered else 'insufficient_coverage',
                      'primary_usd_per_scheduled':str(contrast),'model_lower_vs_abstain_usd':point['net_bounds'][0],
                      'stress_lower_vs_abstain_usd':stress['net_bounds'][0],'report_hash':sha(canonical(report))})
    result={'days':daily,'status':'insufficient_evidence','interval':None,'eligible_for_orders':False}
    if all(d['status']=='covered' for d in daily):
        values=[float(d['primary_usd_per_scheduled']) for d in daily]
        rng=random.Random(17)
        draws=sorted(sum(rng.choices(values,k=DAYS))/DAYS for _ in range(2000))
        result.update(status='diagnostic_review',mean_primary_usd_per_scheduled=sum(values)/DAYS,interval=[draws[49],draws[1949]])
    return result


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    commands=parser.add_subparsers(dest='action',required=True)
    register=commands.add_parser('register');register.add_argument('example_report',type=Path);register.add_argument('start',type=date.fromisoformat)
    archive=commands.add_parser('archive');archive.add_argument('bundle',type=Path);archive.add_argument('report',type=Path)
    compare=commands.add_parser('compare');compare.add_argument('registration',type=Path);compare.add_argument('archives',nargs='*',type=Path,help='Private daily archive receipts')
    for p in (register,archive,compare):p.add_argument('--output',required=True,type=Path)
    a=parser.parse_args();private_output(a.output)
    token=subprocess.check_output(['gcloud','auth','print-access-token'],text=True).strip()
    if a.action=='register':
        if a.start<=datetime.now(timezone.utc).date():parser.error('Start on a future UTC day')
        manifest=protocol(json.loads(bounded_file(a.example_report)),a.start)
        ref=store('experiments',canonical(manifest),token)
        if ref['created'][:10]>=a.start.isoformat():raise ValueError('Registration missed the start')
        result={'manifest':manifest,'object':ref}
    elif a.action=='archive':
        # Independently replay before publishing the bundle and report as a pair.
        import tempfile, os
        with tempfile.TemporaryDirectory() as tmp:
            reproduced=Path(tmp)/'report.json'
            binary=Path(__file__).resolve().parents[1]/'target/debug/dashboard'
            subprocess.run([str(binary)],check=True,env={**os.environ,'DASHBOARD_ROLE':'paper-replay','EVALUATION_INPUT':str(a.bundle),'EVALUATION_OUTPUT':str(reproduced)})
            raw=bounded_file(reproduced)
            if raw!=bounded_file(a.report):raise ValueError('Report did not replay exactly')
        pair={'input':store('inputs',bounded_file(a.bundle),token),'report':store('reports',raw,token)}
        result={'objects':pair,'manifest':store('manifests',canonical(pair),token)}
    else:
        receipt=json.loads(bounded_file(a.registration));manifest=receipt['manifest'];ref=receipt['object']
        name=f"{ROOT}/experiments/{sha(canonical(manifest))}.json"
        if ref['name']!=name or not re.fullmatch(r'[0-9]+',ref['generation']):raise ValueError('Invalid registration')
        url='https://storage.googleapis.com/storage/v1/b/'+BUCKET+'/o/'+urllib.parse.quote(name,safe='')+'?generation='+ref['generation']
        meta=json.loads(request(url,token))
        if meta['timeCreated'][:10]>=manifest['start'] or request(url+'&alt=media',token)!=canonical(manifest):raise ValueError('Unverified registration')
        if len(a.archives)>DAYS:raise ValueError('Expected one revision per day')
        reports=[]
        for path in a.archives:
            pair=read_ref(json.loads(bounded_file(path))['manifest'],'manifests',token)
            reports.append(read_ref(pair['report'],'reports',token))
        result=summarize(manifest,reports)
        result['registration']=ref
    private_write(a.output,result)


if __name__=='__main__':main()
