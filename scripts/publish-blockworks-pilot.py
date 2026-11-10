#!/usr/bin/env python3
"""Publish one approved normalized pilot to this app's named Firestore database.

No provider calls, credentials in arguments, retries, schedules or IAM changes.
"""
import argparse
import json
import math
from pathlib import Path
import subprocess
import sys
import time
from urllib.request import Request, build_opener, HTTPRedirectHandler
from urllib.error import HTTPError

ROOT = 'projects/gitunderstand/databases/crypto-prediction-lab/documents'
API = 'https://firestore.googleapis.com/v1/'
META = ('schema_version', 'provider', 'model', 'granularity', 'captured_at', 'start', 'end', 'volume_unit')
CANDLE = ('time', 'open', 'high', 'low', 'close', 'volume')

class NoRedirect(HTTPRedirectHandler):
    def redirect_request(self, *args, **kwargs): return None

def documents(snapshot, now=None):
    now = int(time.time()) if now is None else now
    meta = {k: snapshot[k] for k in META}
    start, end, capture = (meta[k] for k in ('start', 'end', 'captured_at'))
    if (meta['schema_version'] != 1 or meta['provider'] != 'Blockworks'
        or meta['model'] != 'asset-price' or meta['granularity'] != '5m'
        or not all(type(v) is int for v in (start, end, capture))
        or start < 0 or not 0 < end-start <= 604800 or start % 300 or end % 300
        or end > capture-300 or capture > now+60
        or meta['volume_unit'] not in ('usd', 'unverified')):
        raise ValueError('Invalid normalized pilot metadata')
    rows = snapshot['series']
    if len(rows) != 3 or {s['asset'] for s in rows} != {'BTC', 'ETH', 'SOL'}:
        raise ValueError('Expected exactly three asset series')
    result = {}
    for series in rows:
        candles = []
        previous = start-300
        if len(series['candles']) > 2016: raise ValueError('Pilot exceeds point limit')
        for c in series['candles']:
            c = {k:c[k] for k in CANDLE}
            t = c['time']
            if type(t) is not int or t % 300 or not start <= t < end or t <= previous:
                raise ValueError('Invalid or duplicate pilot timestamp')
            o,h,l,cl = (c[k] for k in ('open','high','low','close'))
            if not all(type(n) in (float,int) and math.isfinite(n) and n>0 for n in (o,h,l,cl)) or l>min(o,cl) or h<max(o,cl):
                raise ValueError('Invalid candle prices')
            v = c['volume']
            if v is not None and (type(v) not in (float,int) or not math.isfinite(v) or v<0):
                raise ValueError('Invalid volume')
            previous = t
            candles.append(c)
        row = {k:series[k] for k in ('asset','asset_id','rejected_points')}
        row['candles'] = candles
        result[f'p{capture}_{row["asset"]}'] = {'captured_at':capture,'series':row}
    result['latest'] = {**meta, 'public_display_confirmed':True,
                        'permission_basis':'Owner explicitly confirmed public display permission on 2026-11-10'}
    return result

def write_body(docs, previous=None):
    writes = []
    for name, payload in docs.items():
        encoded = json.dumps(payload, separators=(',',':'), allow_nan=False)
        # Firestore JSON escapes the embedded string; bound the actual transfer too.
        update = {'name':f'{ROOT}/blockworks_pilots/{name}', 'fields':{'payload':{'stringValue':encoded}}}
        condition = {'updateTime':previous} if name == 'latest' and previous else {'exists':False}
        write = {'update':update,'currentDocument':condition}
        if len(json.dumps(write).encode()) > 850000: raise ValueError('Asset document exceeds size limit')
        writes.append(write)
    return {'writes':writes}

def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--snapshot',type=Path,required=True)
    parser.add_argument('--confirm-public-display',action='store_true')
    parser.add_argument('--dry-run',action='store_true')
    args=parser.parse_args()
    try:
        if args.snapshot.stat().st_size > 2000000: raise ValueError('Pilot file exceeds limit')
        docs=documents(json.loads(args.snapshot.read_text()))
        if args.dry_run:
            body=write_body(docs)
            print(f'Validated {len(docs)-1} asset documents and manifest; {len(json.dumps(body).encode())} request bytes. No writes.')
            return 0
        if not args.confirm_public_display: raise ValueError('Explicit public-display confirmation required')
        token=subprocess.run(['gcloud','auth','print-access-token'],capture_output=True,text=True,check=True).stdout.strip()
        if not token: raise ValueError('Cloud authentication unavailable')
        opener=build_opener(NoRedirect())
        headers={'Authorization':'Bearer '+token,'Content-Type':'application/json'}
        previous=None
        try:
            with opener.open(Request(API+ROOT+'/blockworks_pilots/latest',headers=headers),timeout=20) as r:
                raw=r.read(65537)
                if len(raw)>65536:raise ValueError('Manifest exceeds limit')
                old=json.loads(raw)
            latest=json.loads(old['fields']['payload']['stringValue'])
            if latest['captured_at'] >= docs['latest']['captured_at']:
                raise ValueError('A same-age or newer pilot is already published; stopped')
            previous=old['updateTime']
        except HTTPError as e:
            if e.code != 404: raise
        body=json.dumps(write_body(docs,previous),separators=(',',':')).encode()
        req=Request(API+ROOT+':commit',data=body,headers=headers,method='POST')
        with opener.open(req,timeout=30) as r:
            result=json.loads(r.read(65536))
        if len(result.get('writeResults',[])) != 4:raise ValueError('Unexpected publish response')
        print('Published three immutable asset documents and latest manifest to crypto-prediction-lab. No credentials or raw provider payloads uploaded.')
        return 0
    except (OSError,ValueError,KeyError,TypeError,subprocess.SubprocessError):
        print('Pilot publish stopped. Check the normalized snapshot, existing manifest and dedicated database access. No retries; credentials omitted.',file=sys.stderr)
        return 1

if __name__=='__main__':sys.exit(main())
