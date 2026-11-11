import sys
from datetime import date, datetime, timedelta, timezone
from pathlib import Path
import unittest
sys.path.insert(0,str(Path(__file__).resolve().parents[1]))
from paper_experiment import protocol, summarize, SCHEMA, J1
from export_paper_day import source_urls


class PaperExperiment(unittest.TestCase):
    def day(self, day):
        ts=int(datetime.combine(day,datetime.min.time(),tzinfo=timezone.utc).timestamp())
        assets=('BTC','ETH','SOL')
        return {'schema':SCHEMA,'forecast_experiment':J1,'engine_sha256':'f'*64,'day':day.isoformat(),
                'cycles':[{'open_ts':ts+i*900,'asset':a,'candidate_at':'known','scorecard':{'YES':{'modeled_fee':'0'}}} for i in range(96) for a in assets],
                'forecast_evaluation':{a:{'timely_forecasts':96,'scored':96} for a in assets},
                'strategies':{k:{'net_bounds':['1','2']} for k in ['point_scenario','market_favorite','point_stress']}}

    def test_predeclared_consecutive_cohort_and_adverse_bounds(self):
        start=date(2026,12,1)
        reports=[self.day(start+timedelta(days=i)) for i in range(30)]
        m=protocol(reports[0],start)
        self.assertIsNone(summarize(m,reports[1:])['interval'])
        result=summarize(m,reports)
        self.assertAlmostEqual(result['mean_primary_usd_per_scheduled'],-1/288)
        self.assertAlmostEqual(result['interval'][0],-1/288)
        reports[0]['cycles'][0]=reports[0]['cycles'][1]
        self.assertIsNone(summarize(m,reports)['interval'])
        reports[0]['engine_sha256']='0'*64
        with self.assertRaises(ValueError):summarize(m,reports)

    def test_source_paths_bind_original_generation_and_database(self):
        ref={'name':'decisions/v1/s900_KXETH15M_market_midpoint_v1.json','generation':'102'}
        row={'record':{'asset':'ETH','open_ts':900},'object':ref}
        decision,candidate=source_urls(row)
        self.assertIn('generation=102',decision)
        self.assertTrue(candidate.endswith('/crypto-prediction-lab/documents/cycle_snapshots_v1/s900_ETH_execution_candidate'))
        row['record']['asset']='SOL'
        with self.assertRaises(ValueError):source_urls(row)
