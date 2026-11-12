#!/usr/bin/env python3
"""One bounded, local capture of public Yes No Tape hourly snapshots or Kalshi books.

No credentials, cloud writes, background service or recurring schedule. Up to
630 seconds of observations per run. A manifest records completed book file writes.
"""
import argparse
from concurrent.futures import ThreadPoolExecutor
from datetime import datetime, timezone
import json
import os
from pathlib import Path
import re
import time
import urllib.error
import urllib.request

from gemini_research import SERIES, digest, timestamp, write_new

SITE = 'https://yesnotape.com'
KALSHI = 'https://external-api.kalshi.com/trade-api/v2'


def now():
    return datetime.now(timezone.utc).isoformat()


def get(url):
    with urllib.request.urlopen(url, timeout=5) as response:
        payload = response.read(1_000_001)
    if len(payload)>1_000_000:
        raise ValueError('Public response exceeds capture bound')
    return json.loads(payload)


def wait_until(epoch):
    while time.time() < epoch:
        time.sleep(min(15,max(0,epoch-time.time())))


def discover(asset,opened):
    deadline = time.monotonic()+24
    while True:
        market = (get(f'{SITE}/api/snapshot?asset={asset}').get('market') or {})
        if market.get('status')=='active' and re.fullmatch('KX'+asset+r'15M-[A-Z0-9-]+',market.get('ticker','')) and (
                timestamp(market['open_time']).timestamp()==opened and
                timestamp(market['close_time']).timestamp()==opened+900):
            return market
        if time.monotonic()>=deadline:
            raise ValueError('Exact active 15-minute contract unavailable')
        time.sleep(2)


def capture(path,mode,asset,step,market):
    if mode=='hourly':
        tick=time.monotonic()
        board=get(f'{SITE}/api/hourly?asset={asset}')
        envelope={'receipt':now(),'elapsed_ms':round((time.monotonic()-tick)*1000,1),'board':board}
        write_new(path/f'{asset}-{step:02}.json',envelope)
        return None
    close=timestamp(market['close_time'])
    if datetime.now(timezone.utc)>=close:
        raise ValueError('Contract closed')
    started=now()
    raw=get(f"{KALSHI}/markets/{market['ticker']}/orderbook?depth=10")
    received=now()
    if timestamp(received)>=close:
        raise ValueError('Book received after expiry')
    book=raw.get('orderbook_fp',{})
    if not all(isinstance(book.get(k),list) for k in ('yes_dollars','no_dollars')):
        raise ValueError('Public book unavailable')
    record={'asset':asset,'ticker':market['ticker'],'close':market['close_time'],'request_started_at':started,
            'received_at':received,'provider_published_at':None,
            'book':{'yes':book['yes_dollars'],'no':book['no_dollars']}}
    name=f'{asset}-{step:03}.json'
    write_new(path/name,record)
    return {'file':name,'observed_durable_at':now(),'sha256':digest(record)}


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--mode',choices=('hourly','books'),required=True)
    parser.add_argument('--output-dir',type=Path,required=True)
    parser.add_argument('--steps',type=int)
    parser.add_argument('--interval',type=int)
    parser.add_argument('--next-window',action='store_true',help='Start a book capture in the next 15-minute contract')
    args=parser.parse_args()
    steps=args.steps if args.steps is not None else (12 if args.mode=='hourly' else 36)
    interval=args.interval if args.interval is not None else (30 if args.mode=='hourly' else 3)
    if not 1<=steps<=160 or interval<3 or (steps-1)*interval>630:
        parser.error('Choose 1–160 observations at least 3s apart over at most 630s')
    if args.mode=='hourly' and (args.next_window or interval<20 or steps>64):
        parser.error('Hourly snapshots use at least 20s spacing and at most 64 observations')
    os.umask(0o077)
    path=args.output_dir
    path.mkdir(parents=True,exist_ok=False)
    manifest=[]
    misses=[]
    markets={}
    with ThreadPoolExecutor(max_workers=3) as pool:
        if args.mode=='books':
            opened=int(time.time())//900*900
            if args.next_window:
                opened+=900
                wait_until(opened+2)
            markets=dict(zip(SERIES,pool.map(lambda a:discover(a,opened),SERIES)))
            if time.time()+(steps-1)*interval+5>=opened+900:
                parser.error('Capture would cross the selected expiry; use --next-window')
        start=time.monotonic()
        for i in range(steps):
            delta=start+i*interval-time.monotonic()
            if delta>0:time.sleep(delta)
            if delta < -interval:
                misses.extend({'asset':a,'step':i,'reason':'schedule_late'} for a in SERIES)
                continue
            jobs={a:pool.submit(capture,path,args.mode,a,i,markets.get(a)) for a in SERIES}
            for a,job in jobs.items():
                try:
                    entry=job.result()
                except (ValueError,KeyError,TimeoutError,OSError,urllib.error.URLError) as exc:
                    misses.append({'asset':a,'step':i,'reason':type(exc).__name__})
                    continue
                if entry:manifest.append(entry)
        if args.mode=='books':write_new(path/'manifest.json',manifest)
    summary={'mode':args.mode,'steps':steps,'interval_seconds':interval,'scheduled':steps*3,
             'completed':steps*3-len(misses),'misses':misses}
    write_new(path/'capture-summary.json',summary)
    print(json.dumps({k:v for k,v in summary.items() if k!='misses'}))


if __name__=='__main__':
    main()
