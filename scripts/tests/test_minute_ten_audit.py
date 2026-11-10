import copy
from datetime import date
import json
from pathlib import Path
import sys
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import audit_minute_ten as audit


class MinuteTenAuditTests(unittest.TestCase):
    def setUp(self):
        self.row = json.loads((Path(__file__).parent / 'fixtures/minute_ten.json').read_text())
        self.opened = int(audit.instant(self.row['open_time']).timestamp())
        self.document = {'name': f'{audit.ROOT}/minute_ten_v1/s{self.opened}_BTC',
                    'createTime': '2026-11-10T08:10:21.100000Z',
                    'fields': {'payload': {'stringValue': json.dumps(self.row)}}}

    def doc(self):
        self.document['fields']['payload']['stringValue'] = json.dumps(self.row)
        return self.document

    def packet(self, cutoff):
        return audit.model_packet(self.doc(), 'BTC', self.opened, '2026-11-10T' + cutoff + 'Z')

    def test_receipt_21_seconds_late_hidden_until_durable_availability(self):
        for t in ['08:10:00', '08:10:21', '08:10:21.099999']:
            p = self.packet(t)
            self.assertEqual(p['reason'], 'not_yet_available')
            self.assertIsNone(p['features'])
        p = self.packet('08:10:21.100000')
        self.assertEqual(p['status'], 'available')
        self.assertEqual(p['features']['yes_ask_usd'], '0.45')
        self.assertEqual(p['features']['yes_midpoint_usd'], '0.425')
        self.assertEqual(self.packet('08:11:22')['reason'], 'stale_observation')

    def test_nanosecond_precision_rounds_up(self):
        self.assertEqual(audit.instant(self.row['received_at']).microsecond, 1)
        self.document['createTime'] = '2026-11-10T08:10:20Z'
        self.assertEqual(self.packet('08:10:21')['status'], 'unavailable')
        self.assertEqual(self.packet('08:10:21.000001')['status'], 'available')

    def test_labels_hindsight_and_unknown_fields_never_enter_packet_or_hash(self):
        before = self.packet('08:10:22')
        self.row['result'] = 'yes'
        self.row['observation'].update(result='no', expiration_value='200', best_entry='0.01',
                                       future_candles=[{'close': '1'}])
        self.document['updateTime'] = '2026-11-11T00:00:00Z'
        after = self.packet('08:10:22')
        self.assertEqual(before, after)
        self.assertNotIn('result', json.dumps(after))

    def test_late_combined_metadata_hides_book(self):
        self.row['received_at'] = '2026-11-10T08:10:29Z'
        self.document['createTime'] = '2026-11-10T08:10:30Z'
        self.assertIsNone(self.packet('08:10:25')['features'])
        self.assertEqual(self.packet('08:10:30')['status'], 'available')
        self.assertIn('unknown', self.packet('08:10:30')['features']['component_timing'])

    def test_missing_crossed_invalid_and_rollover_books(self):
        original = copy.deepcopy(self.row)
        for field, value, expected in [('yes_bids', [], 'missing_or_invalid_depth'),
                                       ('no_bids', [], 'missing_or_invalid_depth'),
                                       ('yes_bids', [['0.9', '1']], 'crossed_book'),
                                       ('no_bids', [['NaN', '1']], 'invalid_level'),
                                       ('ticker', 'KXETH15M-OTHER', 'ticker_mismatch')]:
            self.row = copy.deepcopy(original)
            self.row['observation'][field] = value
            self.assertEqual(self.packet('08:10:22')['reason'], expected)
        self.row = original
        self.row['open_time'] = '2026-11-10T08:15:00Z'
        self.assertEqual(self.packet('08:10:22')['reason'], 'identity_mismatch')
        self.assertEqual(audit.model_packet(None, 'BTC', self.opened,
                                           '2026-11-10T08:10:22Z')['reason'], 'missing_snapshot')
        with self.assertRaises(ValueError):
            self.packet('08:15:00')

    def test_reproducible_counts_join_and_missing_denominators(self):
        ticker = self.row['observation']['ticker']
        market = {'ticker': ticker, 'open_time': self.row['open_time'],
                  'close_time': '2026-11-10T08:15:00Z', 'result': 'yes'}
        summary = {'ticker': ticker, 'asset': 'BTC', 'result': 'yes'}
        doc = {'name': f'{audit.ROOT}/contracts_btc/{ticker}', 'fields': {
            'payload': {'stringValue': json.dumps({'market': market})},
            'summary': {'stringValue': json.dumps(summary)}}}
        data = {'start_day': '2026-11-10', 'end_day_exclusive': '2026-11-11',
                'read_time': '2026-11-11T00:00:00Z', 'snapshots': [self.doc()], 'outcomes': [doc]}
        report = audit.report(data)
        self.assertEqual(report, audit.report(copy.deepcopy(data)))
        btc = report['assets']['BTC']
        self.assertEqual(btc['completed_slots'], 96)
        self.assertEqual(btc['usable_books_with_official_result'], 1)
        self.assertEqual(btc['book_exclusions']['missing_snapshot'], 95)
        self.assertEqual(report['assets']['ETH']['usable_books'], 0)
        self.assertEqual(btc['usable_at_exact_cutoff'], 0)
        market['result'] = summary['result'] = 'no'
        doc['fields']['payload']['stringValue'] = json.dumps({'market': market})
        doc['fields']['summary']['stringValue'] = json.dumps(summary)
        self.assertEqual(audit.report(data)['assets'], report['assets'])
        with self.assertRaises(ValueError):
            audit.report(dict(data, snapshots=[self.doc(), self.doc()]))

    def test_all_registry_assets_have_blinded_versioned_packets(self):
        for asset in audit.ASSETS:
            self.row['asset'] = asset
            self.row['observation']['ticker'] = f'KX{asset}15M-SYNTHETIC'
            self.document['name'] = f'{audit.ROOT}/minute_ten_v1/s{self.opened}_{asset}'
            packet = audit.model_packet(self.doc(), asset, self.opened, '2026-11-10T08:10:22Z')
            self.assertEqual(packet['schema_version'], 1)
            self.assertEqual(packet['features']['ticker'], f'KX{asset}15M-SYNTHETIC')
            hidden = audit.model_packet(self.doc(), asset, self.opened, '2026-11-10T08:10:00Z')
            self.assertIsNone(hidden['features'])

    def test_cloud_reads_are_bounded_and_share_one_snapshot(self):
        class DB:
            def request(self, path, body):
                self.assert_path(path, body)
                return [{'readTime': '2026-11-10T16:00:00Z'}] if path == ':runQuery' else []

            def assert_path(inner, path, body):
                self.assertIn(path, (':runQuery', ':batchGet'))
                if path == ':batchGet':
                    self.assertEqual(len(body['documents']), 96)
                    self.assertEqual(body['readTime'], '2026-11-10T16:00:00Z')
                    self.assertTrue(all(name.startswith(audit.ROOT + '/minute_ten_v1/')
                                        for name in body['documents']))

            def archived_day(inner, asset, day, read_time):
                self.assertIn(asset, audit.ASSETS)
                self.assertEqual(read_time, '2026-11-10T16:00:00Z')
                return []
        with patch.object(audit, 'Firestore', return_value=DB()):
            data = audit.fetch(date(2026, 11, 10), date(2026, 11, 11))
        self.assertEqual(data['snapshots'], [])


if __name__ == '__main__':
    unittest.main()
