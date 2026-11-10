import importlib.util
from pathlib import Path
import unittest

spec=importlib.util.spec_from_file_location('publisher',Path(__file__).resolve().parents[1]/'publish-blockworks-pilot.py')
publisher=importlib.util.module_from_spec(spec)
spec.loader.exec_module(publisher)

def snapshot():
    return dict(schema_version=1,provider='Blockworks',model='asset-price',granularity='5m',captured_at=900,start=0,end=600,volume_unit='unverified',series=[dict(asset=a,asset_id='id',rejected_points=0,candles=[dict(time=0,open=10.,high=12.,low=9.,close=11.,volume=0)]) for a in ('BTC','ETH','SOL')])

class PublisherTests(unittest.TestCase):
    def test_only_four_app_specific_writes_and_no_extra_fields(self):
        s=snapshot();s['private_key']='must-not-escape';s['series'][0]['candles'][0]['extra']='private'
        docs=publisher.documents(s,900);self.assertEqual(len(docs),4)
        body=publisher.write_body(docs,'existing-revision')
        self.assertNotIn('must-not-escape',str(body));self.assertNotIn('extra',str(body))
        self.assertTrue(all(w['update']['name'].startswith(publisher.ROOT+'/blockworks_pilots/') for w in body['writes']))
        self.assertEqual(body['writes'][-1]['currentDocument'],{'updateTime':'existing-revision'})
        self.assertTrue(all(w['currentDocument']=={'exists':False} for w in body['writes'][:-1]))
    def test_rejects_oversized_horizon_bad_ohlc_duplicate_times_and_future_capture(self):
        for mutate in (
            lambda s:s.update(end=605100,captured_at=700000),
            lambda s:s['series'][0]['candles'][0].update(high=1),
            lambda s:s['series'][0]['candles'].append(s['series'][0]['candles'][0]),
            lambda s:s.update(captured_at=1000),
        ):
            s=snapshot();mutate(s)
            with self.assertRaises(ValueError):publisher.documents(s,900)
    def test_missing_volume_and_zero_are_preserved(self):
        s=snapshot();s['series'][0]['candles'][0]['volume']=None
        docs=publisher.documents(s,900)
        self.assertIsNone(docs['p900_BTC']['series']['candles'][0]['volume'])
        self.assertEqual(docs['p900_ETH']['series']['candles'][0]['volume'],0)

if __name__=='__main__':unittest.main()
