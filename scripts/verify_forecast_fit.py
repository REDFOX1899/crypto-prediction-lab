#!/usr/bin/env python3
"""Independent offline arithmetic oracle for the fixed v1 pilot. Aggregate stdout only."""
import argparse
import json
import math
from pathlib import Path
from export_forecast_data import ns, digest

ASSETS = ('BTC', 'ETH', 'SOL')
TRAIN = ns('2026-11-11T06:00:00Z')
CAL = ns('2026-11-11T11:00:00Z')
GAP = 1800 * 10**9


def clip(p):
    return max(.000001, min(.999999, p))


def sigmoid(z):
    return 1 / (1 + math.exp(-max(-40, min(40, z))))


def vector(p):
    yes = sum(float(q) for _, q in p['book']['yes_dollars'])
    no = sum(float(q) for _, q in p['book']['no_dollars'])
    mid = clip(float(p['yes_midpoint_usd']))
    cutoff_millis = ns(p['cutoff']) // 10**6
    return [1., math.log(mid / (1-mid)), float(p['spread_usd']),
            (p['open_ts'] + 900 - cutoff_millis/1000) / 900,
            (yes-no)/(yes+no), float(p['asset'] == 'BTC'), float(p['asset'] == 'ETH')]


def close(left, right):
    assert math.isclose(left, right, rel_tol=1e-10, abs_tol=1e-11), (left, right)


def verify(data, model, report):
    labels = {(l['asset'], l['open_ts']): l for l in data['labels']}
    rows = []
    for d in data['decisions']:
        l = labels.get((d['asset'], d['open_ts']))
        if d['status'] != 'forecast' or l is None:
            continue
        p = d['packet']
        assert d['packet_hash'] == digest(p)
        assert p['asset'] == d['asset'] == l['asset'] and p['ticker'] == l['ticker']
        assert p['open_ts'] == d['open_ts'] == l['open_ts']
        times = [p[k] for k in ('request_started_at', 'metadata_request_started_at', 'metadata_received_at', 'book_request_started_at', 'book_received_at', 'received_at', 'source_created_at', 'cutoff')]
        assert list(map(ns, times)) == sorted(map(ns, times))
        assert ns(p['cutoff']) <= ns(d['committed_at']) < (p['open_ts']+40)*10**9
        assert ns(l['available_at']) >= ns(l['source_version']) >= l['close_ts']*10**9
        rows.append({'open':d['open_ts']*10**9,'asset':d['asset'],'x':vector(p),'mid':float(p['yes_midpoint_usd']),'y':float(l['yes']),'available':ns(l['available_at'])})
    train = [r for r in rows if r['open'] < TRAIN-GAP and r['available'] <= TRAIN]
    cal = [r for r in rows if TRAIN <= r['open'] < CAL-GAP and r['available'] <= CAL]
    test = [r for r in rows if r['open'] >= CAL]
    assert set(r['open'] for r in train).isdisjoint(r['open'] for r in cal)
    assert set(r['open'] for r in cal).isdisjoint(r['open'] for r in test)
    mean = [0.] + [sum(r['x'][j] for r in train)/len(train) for j in range(1, 7)]
    scale = [1.] + [max(.01, math.sqrt(sum((r['x'][j]-mean[j])**2 for r in train)/len(train))) for j in range(1, 7)]
    for x,y in zip(mean, model['mean']): close(x,y)
    for x,y in zip(scale, model['scale']): close(x,y)
    standard = lambda x: [1.] + [max(-10,min(10,(x[j]-mean[j])/scale[j])) for j in range(1,7)]
    xs = [standard(r['x']) for r in train]
    beta = [0.] * 7
    for _ in range(800):
        grad = [0.] * 7
        for r,x in zip(train,xs):
            residual = sigmoid(sum(b*t for b,t in zip(beta,x)))-r['y']
            for j in range(7): grad[j] += residual*x[j]/len(train)
        beta = [b-.1*(g+(.1*b if j else 0)) for j,(b,g) in enumerate(zip(beta,grad))]
    for x,y in zip(beta,model['coefficients']): close(x,y)
    zs = [sum(b*x for b,x in zip(beta,standard(r['x']))) for r in cal]
    calibration = [0.,1.]
    for _ in range(800):
        grad = [0.,0.]
        for r,z in zip(cal,zs):
            residual = sigmoid(calibration[0]+calibration[1]*z)-r['y']
            grad[0] += residual/len(cal)
            grad[1] += residual*z/len(cal)
        calibration[0] -= .05*(grad[0]+.1*calibration[0])
        calibration[1] = max(.05,min(10,calibration[1]-.05*(grad[1]+.1*(calibration[1]-1))))
    for x,y in zip(calibration,model['calibration']): close(x,y)
    rate = [(1+sum(r['y'] for r in train if r['asset']==a))/(2+sum(r['asset']==a for r in train)) for a in ASSETS]
    for x,y in zip(rate,model['historical_yes_rate']): close(x,y)
    checked = 0
    for asset in ('pooled',) + ASSETS:
        cohort = [r for r in test if asset=='pooled' or r['asset']==asset]
        for strategy in ('constant_half','historical_yes_rate','market_midpoint','logistic_raw','logistic_calibrated'):
            pairs = []
            for r in cohort:
                z = sum(b*x for b,x in zip(beta,standard(r['x'])))
                p = {'constant_half':.5,'historical_yes_rate':rate[ASSETS.index(r['asset'])], 'market_midpoint':r['mid'], 'logistic_raw':clip(sigmoid(z)), 'logistic_calibrated':clip(sigmoid(calibration[0]+calibration[1]*z))}[strategy]
                pairs.append((p,r['y']))
            actual = report['evaluation'][asset]['strategies'][strategy]
            assert actual['rows'] == len(pairs)
            if pairs:
                close(sum((p-y)**2 for p,y in pairs)/len(pairs),actual['brier'])
                close(-sum(y*math.log(clip(p))+(1-y)*math.log(1-clip(p)) for p,y in pairs)/len(pairs),actual['log_loss'])
            checked += 1
    assert report['model_id'] == 'forecast_models/v1/' + digest(model) + '.json'
    return {'status':'passed','independent_strategy_score_pairs':checked,'rows':{'train':len(train),'calibration':len(cal),'test':len(test)},'normalization_coefficients_calibration_baselines_and_scores':'matched'}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('dataset', type=Path)
    parser.add_argument('fit_directory', type=Path)
    args = parser.parse_args()
    print(json.dumps(verify(json.loads(args.dataset.read_text()), json.loads((args.fit_directory/'model.json').read_text()), json.loads((args.fit_directory/'report.json').read_text()))))


if __name__ == '__main__':
    main()
