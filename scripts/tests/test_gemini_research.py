import json
import sys
import tempfile
import unittest
from pathlib import Path
sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from gemini_research import catalog, digest, prepare, write_new


def board():
    return {'fetched_at':'2026-11-12T01:12:01.234567890Z','markets':[
        {'ticker':f'KXBTCD-TEST-{i}', 'status':'active', 'strike_type':'greater',
         'floor_strike':12345+i,'yes_ask_dollars':'.52','no_ask_dollars':'.49',
         'close_time':'2026-11-12T02:00:00Z'} for i in range(21)]}

class GeminiResearchTest(unittest.TestCase):
    def test_catalog(self):
        rows=catalog()
        self.assertEqual(len({r['id'] for r in rows}),1000)
        self.assertEqual(len(rows),1000)

    def test_primary_and_stress_share_only_blinded_features(self):
        data=board()
        full=prepare(data,'BTC','stress')
        self.assertEqual(len(full['request']['questions']),1000)
        primary=prepare(data,'BTC','primary')
        self.assertEqual(list(primary['request']['questions']),['official_yes'])
        self.assertNotIn('KXBTCD',json.dumps(full['request']))
        self.assertNotIn('2026-11',json.dumps(full['request']))
        self.assertEqual(full['evaluation_only']['request_hash'],digest(full['request']))
        for m in data['markets']:
            m['result']='yes'
            m['prompt']='arbitrary source text'
        self.assertEqual(full['request'],prepare(data,'BTC','stress')['request'])

    def test_expiry_and_data_integrity(self):
        data=board();data['fetched_at']='2026-11-12T01:59:30Z'
        with self.assertRaises(ValueError):prepare(data,'BTC','stress')
        data['fetched_at']='2026-11-12T01:59:01Z'
        self.assertEqual(len(prepare(data,'BTC','stress')['request']['questions']),100)
        data['markets'].append(data['markets'][0])
        with self.assertRaises(ValueError):prepare(data,'BTC','primary')
        with self.assertRaises(ValueError):prepare(board(),'SOL','primary')
        data=board()
        for row in data['markets']:row['no_ask_dollars']='.01'
        with self.assertRaises(ValueError):prepare(data,'BTC','primary')

    def test_durable_output_is_private_and_create_only(self):
        with tempfile.TemporaryDirectory() as tmp:
            path=Path(tmp)/'bundle.json'
            write_new(path,{'a':1})
            with self.assertRaises(FileExistsError):write_new(path,{'a':2})
            self.assertEqual(json.loads(path.read_text()),{'a':1})
            self.assertEqual(path.stat().st_mode & 0o777,0o600)

class RehearsalLabelsTest(unittest.TestCase):
    def test_later_snapshot_scores_only_measurable_targets(self):
        from gemini_research import replay
        data=board()
        initial={'board':data,'receipt':'2026-11-12T01:12:02Z'}
        future=board();future['fetched_at']='2026-11-12T01:12:31Z'
        for r in future['markets']:
            r['yes_ask_dollars']='.55';r['no_ask_dollars']='.50'
        next_record={'board':future,'receipt':'2026-11-12T01:12:32Z'}
        result=replay(initial,[next_record],'BTC')
        self.assertEqual(result['eligible_horizons'],1)
        self.assertEqual(result['observed_labels'],80)
        self.assertEqual(result['by_target']['ask_up']['yes'],20)
        self.assertEqual(result['by_target']['wide_spread']['yes'],20)
        self.assertEqual(result['by_target']['offer_size']['missing'],200)
        next_record['receipt']='2026-11-12T01:13:05Z'
        self.assertEqual(replay(initial,[next_record],'BTC')['observed_labels'],0)

class PortfolioTest(unittest.TestCase):
    def test_cross_asset_cap_and_timeliness(self):
        from gemini_research import portfolio,SERIES
        snapshots={}
        for a,prefix in SERIES.items():
            b=board()
            for r in b['markets']:r['ticker']=r['ticker'].replace('KXBTCD',prefix)
            snapshots[a]={'receipt':'2026-11-12T01:12:02Z','board':b}
        packet=portfolio(snapshots)
        self.assertEqual(len(packet['request']['questions']),1000)
        self.assertEqual(len(packet['evaluation_only']['strikes']),20)
        self.assertTrue(all(any(k.startswith(a) for k in packet['request']['questions']) for a in SERIES))
        snapshots['ETH']['receipt']='2026-11-12T01:13:00Z'
        with self.assertRaises(ValueError):portfolio(snapshots)

    def test_shared_grading_uses_joint_cutoff_and_exact_baseline_contracts(self):
        from gemini_research import replay_portfolio,SERIES
        snapshots, followups = {}, {}
        for a,prefix in SERIES.items():
            b=board()
            for r in b['markets']:r['ticker']=r['ticker'].replace('KXBTCD',prefix)
            snapshots[a]={'board':b,'receipt':'2026-11-12T01:12:05Z'}
            later=json.loads(json.dumps(b));later['fetched_at']='2026-11-12T01:12:35Z'
            followups[a]=[{'board':later,'receipt':'2026-11-12T01:12:37Z'}]
        scores=replay_portfolio(snapshots,followups)
        self.assertEqual(scores['question_count'],1000)
        self.assertEqual(scores['observed_labels'],80)
        self.assertTrue(all(s['provider_calls']==0 for s in scores['by_asset'].values()))
