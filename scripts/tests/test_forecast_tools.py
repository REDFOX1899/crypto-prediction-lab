import math
from pathlib import Path
import sys
import unittest
sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from export_forecast_data import ns
from verify_forecast_fit import vector, sigmoid


class ForecastTools(unittest.TestCase):
    def test_nanosecond_cutoff_and_timezone_offsets_are_exact(self):
        t = ns('2026-11-11T06:00:00Z')
        self.assertEqual(ns('2026-11-11T06:00:00.000000001Z'), t + 1)
        self.assertEqual(ns('2026-11-11T01:00:00-05:00'), t)
        for text in ['2026-11-11', '2026-11-11T06:00:00.1234567890Z', 'tomorrow']:
            with self.assertRaises(ValueError):
                ns(text)

    def test_independent_features_have_correct_units_and_complements(self):
        opened = ns('2026-11-11T06:00:00Z') // 10**9
        p = {'asset':'ETH', 'open_ts':opened, 'cutoff':'2026-11-11T06:00:09.999999999Z',
             'yes_midpoint_usd':'.5', 'spread_usd':'.02',
             'book':{'yes_dollars':[['.49','30']],'no_dollars':[['.49','10']]}}
        x = vector(p)
        self.assertEqual(x[:3],[1.,0.,.02])
        self.assertAlmostEqual(x[3],(900-9.999)/900, places=9)
        self.assertEqual(x[4:],[.5,0.,1.])
        self.assertTrue(math.isfinite(sigmoid(1000)))
        self.assertTrue(0 < sigmoid(-1000) < .000001)
