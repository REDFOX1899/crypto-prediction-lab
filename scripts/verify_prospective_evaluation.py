#!/usr/bin/env python3
"""Independent arithmetic and temporal audit of a downloaded, immutable daily report."""
import argparse
from collections import Counter
import json
import math
from pathlib import Path
from export_forecast_data import ns, digest
from verify_forecast_fit import clip, close, sigmoid, vector

ASSETS = ('BTC','ETH','SOL')
STRATEGIES = ('constant_half','historical_yes_rate','market_midpoint','logistic_raw','logistic_calibrated')


def verify(data, envelope, rows):
    report = envelope['report']
    asof = ns(data['asof'])
    manifest = data['experiment']['manifest']
    assert ns(data['experiment']['object']['committed_at']) <= asof
    assert ns(data['experiment']['object']['committed_at']) < ns(manifest['confirmatory_start'])
    assert report['asof'] == data['asof'] and report['day'] == data['day']
    expected = {(a,t) for t in range(report['start_ts'],report['end_ts_exclusive'],900) for a in ASSETS}
    assert {(r['asset'],r['open_ts']) for r in rows} == expected and len(rows) == len(expected)
    decisions = {(d['record']['asset'],d['record']['open_ts']):d for d in data['decisions']}
    assert len(decisions) == len(data['decisions'])
    labels = {}
    for l in data['labels']:
        assert ns(l['source_version']) <= ns(l['available_at']) <= asof
        assert ns(l['available_at']) == max(ns(l['source_version']),ns(l['captured_at']))
        assert l['close_ts'] == l['open_ts']+900 and ns(l['available_at']) >= l['close_ts']*10**9
        key = (l['asset'],l['open_ts'])
        if key not in labels or (ns(l['available_at']),ns(l['source_version'])) > (ns(labels[key]['available_at']),ns(labels[key]['source_version'])):
            labels[key] = l
    checks = 0
    for row in rows:
        key = (row['asset'],row['open_ts']); saved = decisions.get(key)
        if saved is None:
            assert row['status'] == 'missing_decision';continue
        r = saved['record']; p = r.get('packet'); proof = r.get('forecast_proof')
        assert ns(saved['object']['committed_at']) <= asof
        if ns(saved['object']['committed_at']) >= (row['open_ts']+40)*10**9:
            assert row['status'] == 'late_or_invalid_decision';continue
        if r['status'] != 'forecast':
            assert row['status'] == 'decision_abstained';continue
        if proof is None:
            assert row['status'] == 'no_committed_forecast';continue
        assert r['packet_hash'] == proof['packet_hash'] == digest(p)
        assert ns(p['source_created_at']) <= ns(p['cutoff']) <= ns(r['produced_at']) <= ns(saved['object']['committed_at'])
        assert proof['object_name'] == manifest['model_object'] and proof['generation'] == manifest['model_generation']
        assert ns(proof['model_committed_at']) == ns(manifest['model_committed_at']) <= ns(p['cutoff'])
        times = [p[k] for k in ('request_started_at','metadata_request_started_at','metadata_received_at','book_request_started_at','book_received_at','received_at','source_created_at','cutoff')]
        assert list(map(ns,times)) == sorted(map(ns,times))
        m = proof['model']
        assert ns(m['training_max_label_available_at']) <= ns(m['train_before'])
        assert ns(m['calibration_max_label_available_at']) <= ns(m['calibrate_before']) <= ns(proof['model_committed_at'])
        x = vector(p); z = sum(b*(1 if j==0 else max(-10,min(10,(x[j]-m['mean'][j])/m['scale'][j]))) for j,b in enumerate(m['coefficients']))
        close(proof['predictions'][0],clip(sigmoid(z)));close(proof['predictions'][1],clip(sigmoid(m['calibration'][0]+m['calibration'][1]*z)))
        probabilities = [.5,m['historical_yes_rate'][ASSETS.index(row['asset'])],float(p['yes_midpoint_usd']),*proof['predictions']]
        for a,b in zip(row['probabilities'],probabilities):close(a,b)
        label = labels.get(key)
        if label is None:
            assert row['status']=='official_result_unavailable'
        elif label['result'] not in ('yes','no'):
            assert row['status']=='non_binary_official_result'
        elif label['ticker'] != p['ticker']:
            assert row['status']=='label_ticker_mismatch'
        else:
            assert row['status']=='scored' and row['official_result']==label['result']
        if label:
            assert row['label_id']==digest(label)
        checks += 1
    for asset in ('pooled',)+ASSETS:
        cohort = [r for r in rows if asset=='pooled' or r['asset']==asset]
        scored = [r for r in cohort if r['status']=='scored']
        actual = report['evaluation'][asset]
        assert actual['expected']==len(cohort) and actual['scored']==len(scored)
        assert actual['reason_counts']==dict(Counter(r['status'] for r in cohort))
        assert actual['timely_forecasts']==sum(r['probabilities'] is not None for r in cohort)
        deltas = []
        for j,strategy in enumerate(STRATEGIES):
            pairs = [(r['probabilities'][j],float(r['official_result']=='yes')) for r in scored]
            metric = actual['strategies'][strategy];assert metric['rows']==len(pairs)
            if pairs:
                close(metric['brier'],sum((p-y)**2 for p,y in pairs)/len(pairs))
                close(metric['log_loss'],-sum(y*math.log(clip(p))+(1-y)*math.log(1-clip(p)) for p,y in pairs)/len(pairs))
                for i,band in enumerate(metric['reliability_bins']):
                    ps = [(p,y) for p,y in pairs if min(4,int(p*5))==i]
                    assert band['rows']==len(ps)
                    if ps:
                        close(band['mean_probability'],sum(p for p,_ in ps)/len(ps));close(band['observed_yes_rate'],sum(y for _,y in ps)/len(ps))
                    else:
                        assert band['mean_probability'] is None and band['observed_yes_rate'] is None
            else:
                assert metric['brier'] is None and metric['log_loss'] is None
        for r in scored:
            y = float(r['official_result']=='yes');p = r['probabilities'];deltas.append((p[4]-y)**2-(p[2]-y)**2)
        close(actual['paired_brier_delta_sum'],sum(deltas))
        missing = len(cohort)-len(scored)
        if cohort:
            close(actual['full_scheduled_delta_bounds']['lower'],(sum(deltas)-missing)/len(cohort))
            close(actual['full_scheduled_delta_bounds']['upper'],(sum(deltas)+missing)/len(cohort))
    assert report['paper_order_eligible'] is False and report['probability_interval'] is None
    return {'status':'passed','forecast_arithmetic_and_cutoff_checks':checks,'score_pairs':20,'reliability_bins':100,'rows':len(rows),'asof':data['asof']}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('directory', type=Path)
    args = parser.parse_args()
    load = lambda name:json.loads((args.directory/name).read_text())
    print(json.dumps(verify(load('input.json'),load('envelope.json'),load('rows.json'))))


if __name__ == '__main__':
    main()
