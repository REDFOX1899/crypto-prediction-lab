#!/usr/bin/env python3
"""Operator-side replay, mutation and independent ledger checks; no network access."""
import argparse
import copy
from decimal import Decimal
import json
import os
from pathlib import Path
import subprocess
import tempfile
from run_prospective_evaluation import private_write


def require(condition, message):
    if not condition:
        raise ValueError(message)


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('bundle',type=Path);parser.add_argument('report',type=Path)
    args=parser.parse_args()
    binary=Path(__file__).resolve().parents[1]/'target/debug/dashboard'
    bundle=json.loads(args.bundle.read_bytes());original=json.loads(args.report.read_bytes())
    def replay(data):
        with tempfile.TemporaryDirectory() as tmp:
            inp=Path(tmp)/'input.json';out=Path(tmp)/'report.json'
            private_write(inp,data)
            subprocess.run([str(binary)],env={**os.environ,'DASHBOARD_ROLE':'paper-replay','EVALUATION_INPUT':str(inp),'EVALUATION_OUTPUT':str(out)},check=True,stdout=subprocess.DEVNULL)
            return out.read_bytes()
    require(replay(bundle)==args.report.read_bytes(),'Exact report replay failed')
    changed=copy.deepcopy(bundle)
    for label in changed['evaluation']['labels']:
        if label['result'] in ('yes','no'):
            label['result']='yes' if label['result']=='no' else 'no'
    flipped=json.loads(replay(changed))
    require(len(original['cycles'])==len(flipped['cycles']),'Cohort changed')
    for a,b in zip(original['cycles'],flipped['cycles']):
        for k in ['scorecard','committed_at','candidate_at','packet_hash']:
            require(a.get(k)==b.get(k),'Outcome changed opening or later book')
        for strategy,s in a['strategies'].items():
            t=b['strategies'][strategy]
            for k in ['side','cap','reason','intent_at','execution_at','filled_qty','fills','ioc_canceled_qty']:
                require(s.get(k)==t.get(k),'Outcome changed intent or fill')
    for key,wallet in original['strategies'].items():
        net=Decimal(0);paid=Decimal(0);fees=Decimal(0);spent=Decimal(0)
        for row in original['cycles']:
            sim=row['strategies'][key]
            for level in sim.get('fills',[]):
                spent+=Decimal(level['qty'])*Decimal(level['price'])
                fees+=Decimal(level['fee'])
            if sim.get('net_pnl') is not None:
                payout=Decimal(sim['filled_qty']) if sim['side'].lower()==row['official_result'] else Decimal(0)
                require(payout-Decimal(sim['cost_with_fees'])==Decimal(sim['net_pnl']),'Settlement mismatch')
                paid+=payout;net+=Decimal(sim['net_pnl'])
        require((spent+fees,paid,fees)==tuple(Decimal(wallet[k]) for k in ['capital_spent','settled_payout','modeled_fees']),'Cash reconciliation failed')
        if wallet['net_pnl'] is not None:
            require(net==Decimal(wallet['net_pnl']),'Net total mismatch')
    # Introduce a future candidate where the as-of replay has none. Every field,
    # including the missing-evidence reason and scorecard hash, must stay equal.
    absent=copy.deepcopy(bundle);future=copy.deepcopy(bundle)
    for name,e in absent['evidence'].items():
        e['candidate']=None
        future['evidence'][name]['candidate']={'createTime':'2100-01-01T00:00:00Z','outcome':'future'}
    require(replay(absent)==replay(future),'Future candidate influenced report')
    print(f"Verified exact replay, future-book isolation, winner mutations and {len(original['strategies'])} balances for {len(original['cycles'])} scheduled markets.")


if __name__=='__main__':main()
