import sys
import unittest
from pathlib import Path
sys.path.insert(0,str(Path(__file__).resolve().parents[1]))
from market_feature_windows import book_values,materialize

def row(at,ask='.45',available=None):
    return {'t':at,'available':at if available is None else available,'ticker':'X','close':2000,
            'book':{'yes':[['.40','20'],['.35','10']], 'no':[[ask,'10']]}}

class FeatureWindowsTest(unittest.TestCase):
    def test_depth_microprice_and_missing_sides(self):
        values=book_values(row(1)['book'])
        self.assertAlmostEqual(values['microprice_cents'],50.)
        self.assertAlmostEqual(values['top10_imbalance'],.5)
        self.assertEqual(values['yes_top3_size'],30)
        self.assertEqual(book_values({'yes':[],'no':[['.2',4]]}),{})
        self.assertEqual(book_values({'yes':[['.4',3]],'no':[['.7',4]]}),{})

    def test_real_windows_need_complete_fresh_grid_and_two_receipts(self):
        rows=[row(i) for i in range(1000,1101,2)]
        result=materialize(rows,1100)
        self.assertEqual(result['defined_metrics'],17*5*5)
        self.assertEqual(result['windows']['yes_mid_cents.5s']['values']['mean'],47.5)
        self.assertEqual(result['windows']['yes_spread_change_cents.60s']['values']['maximum'],0)
        self.assertIsNone(result['windows']['yes_mid_cents.120s']['values']['mean'])
        self.assertEqual(sum(result['missing_metrics'].values())+result['defined_metrics'],1000)
        single=materialize([row(1096)],1100)
        self.assertEqual(single['windows']['yes_mid_cents.5s']['reason'],'fewer_than_two_receipts')

    def test_late_updates_and_future_values_cannot_change_the_cutoff_packet(self):
        rows=[row(i) for i in range(1000,1101,2)]
        expected=materialize(rows,1100)
        self.assertEqual(expected,materialize(rows+[row(1099,'.42',1101),row(1102)],1100))
        other=row(1100);other['ticker']='Y'
        with self.assertRaises(ValueError):materialize(rows+[other],1100)
        gap=materialize([row(1060),row(1099)],1100)
        self.assertEqual(gap['windows']['yes_mid_cents.5s']['reason'],'incomplete_fresh_window')

    def test_intermediate_change_waits_for_both_receipts_to_be_durable(self):
        rows=[row(1000,available=1007),row(1001),row(1004),row(1006)]
        result=materialize(rows,1007)
        # A late source may become readable by the final cutoff. That does not
        # retroactively make the earlier one-second cells available.
        window=result['windows']['yes_mid_change_cents.5s']
        self.assertEqual(window['observed_seconds'],4)
        self.assertIsNone(window['values']['mean'])
