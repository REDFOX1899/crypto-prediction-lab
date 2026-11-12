#!/usr/bin/env python3
"""Publish aggregate diagnostics from an independently verified private daily report."""
import argparse
import json
from pathlib import Path
from verify_prospective_evaluation import verify

def aggregate(directory):
    load = lambda name: json.loads((directory / name).read_text())
    envelope = load('envelope.json')
    data = load('input.json')
    rows = load('rows.json')
    verify(data, envelope, rows)
    report = envelope['report']
    evaluation = {}
    for group, cohort in report['evaluation'].items():
        evaluation[group] = {key: cohort[key] for key in
                             ('expected', 'timely_forecasts', 'scored', 'unscored', 'reason_counts')}
        evaluation[group]['strategies'] = {
            method: {key: stats[key] for key in ('rows', 'brier', 'reliability_bins')}
            for method, stats in cohort['strategies'].items()
            if method in ('logistic_calibrated', 'market_midpoint', 'constant_half')}
    return dict(protocol='opening_logistic_sigmoid_v1', day=report['day'],
                read_time=report['asof'], test={'cycles': len({r['open_ts'] for r in rows})},
                evaluation=evaluation)

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('directory', type=Path)
    args = parser.parse_args()
    path = Path(__file__).resolve().parents[1] / 'apps/dashboard/public/forecast-pilot-v1.json'
    study = json.loads(path.read_text())
    study['prospective'] = aggregate(args.directory)
    path.write_text(json.dumps(study, indent=2, allow_nan=False) + '\n')
    print('Published verified aggregate metrics only')

if __name__ == '__main__':
    main()
