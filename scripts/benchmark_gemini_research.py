#!/usr/bin/env python3
"""Measure local feature calculation, request construction and JSON encoding only."""
import argparse
import json
import math
from pathlib import Path
from time import perf_counter_ns
from gemini_research import SERIES, portfolio, write_new
from market_feature_windows import load_manifest, materialize


def benchmark(action, runs):
    for _ in range(3):
        action()
    samples = []
    for _ in range(runs):
        start = perf_counter_ns()
        action()
        samples.append((perf_counter_ns() - start) / 1e6)
    samples.sort()
    return {f'p{p}_ms': round(samples[math.ceil(runs*p/100)-1], 3) for p in (50,95,99)}


def encode(value):
    return json.dumps(value, ensure_ascii=False, allow_nan=False, separators=(',', ':')).encode()


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--book-manifest',required=True,type=Path)
    parser.add_argument('--hourly-dir',required=True,type=Path)
    parser.add_argument('--runs',type=int,default=100)
    parser.add_argument('--output',required=True,type=Path)
    args=parser.parse_args()
    if not 100<=args.runs<=500:
        parser.error('Use 100–500 measured runs for percentiles')
    report={'runs_per_measurement':args.runs,'warmups':3,'provider_calls':0,'feature_windows':{},'encoding':{}}
    for asset in SERIES:
        rows=load_manifest(args.book_manifest,asset)
        if not rows:parser.error('Empty book capture for '+asset)
        cutoff=int(max(r['available'] for r in rows))+1
        report['feature_windows'][asset]=benchmark(lambda:materialize(rows,cutoff),args.runs)
    snapshots={a:json.loads((args.hourly_dir/(a+'-00.json')).read_text()) for a in SERIES}
    report['joint_request']=benchmark(lambda:portfolio(snapshots),args.runs)
    request=portfolio(snapshots)['request']
    for count in (1,10,100,1000):
        if count>len(request['questions']):continue
        subset={**request,'questions':dict(list(request['questions'].items())[:count])}
        report['encoding'][str(count)]={'bytes':len(encode(subset)),**benchmark(lambda:encode(subset),args.runs)}
    write_new(args.output,report)
    print(json.dumps(report,sort_keys=True))

if __name__=='__main__':
    main()
