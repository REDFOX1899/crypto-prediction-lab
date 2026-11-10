import io
import importlib.util
from pathlib import Path
import unittest
from unittest.mock import patch
from urllib.error import HTTPError

spec = importlib.util.spec_from_file_location('blockworks_audit', Path(__file__).resolve().parents[1] / 'blockworks-audit.py')
audit = importlib.util.module_from_spec(spec)
spec.loader.exec_module(audit)

class Response:
    headers = {'X-RateLimit-Limit': '25'}
    def __enter__(self): return self
    def __exit__(self, *args): pass
    def read(self, size):
        return b'{"error":null,"data":{"models":[{"slug":"asset-price","intervals":["5m"]},{"slug":"irrelevant"}]}}'

class AuditTests(unittest.TestCase):
    def test_only_two_catalog_requests_and_no_key_in_report(self):
        calls = []
        class Opener:
            def open(self, request, timeout):
                calls.append(request)
                return Response()
        with patch.object(audit, 'build_opener', return_value=Opener()):
            report = audit.audit('unit-test-key')
        self.assertEqual(len(calls), 2)
        self.assertTrue(all('/catalog?accessible=true&publicOnly=true' in r.full_url for r in calls))
        self.assertTrue(all('unit-test-key' not in r.full_url for r in calls))
        self.assertNotIn('unit-test-key', str(report))
        self.assertEqual(report['catalogs']['timeseries']['relevant_models'][0]['slug'], 'asset-price')
        self.assertEqual(len(report['catalogs']['timeseries']['relevant_models']), 1)

    def test_forbidden_stops_without_retry_or_body_disclosure(self):
        class Opener:
            count = 0
            def open(self, request, timeout):
                self.count += 1
                raise HTTPError(request.full_url, 403, 'private provider details', {}, None)
        opener = Opener()
        with patch.object(audit, 'build_opener', return_value=opener):
            with self.assertRaisesRegex(ValueError, 'HTTP 403; stopped without retry'):
                audit.audit('unit-test-key')
        self.assertEqual(opener.count, 1)

    def test_redirect_never_forwards_key(self):
        self.assertIsNone(audit.NoRedirect().redirect_request(None, None, 302, '', {}, 'https://other.invalid'))

class PingTests(unittest.TestCase):
    def test_reports_only_fixed_error_categories_without_provider_details(self):
        class Opener:
            count = 0
            def open(self, request, timeout):
                self.count += 1
                raise HTTPError(request.full_url, 403, 'private detail', {}, io.BytesIO(b'{"error":"invalid API key: unit-test-key"}'))
        opener = Opener()
        with patch.object(audit, 'build_opener', return_value=opener):
            result = audit.ping('unit-test-key')
        self.assertEqual(opener.count, 1)
        self.assertEqual(result['failure_category'], 'invalid_api_key')
        self.assertNotIn('unit-test-key', str(result))
        self.assertNotIn('private detail', str(result))

    def test_cloudflare_signature_denial_is_not_misreported_as_bad_key(self):
        class Opener:
            def open(self, request, timeout):
                raise HTTPError(request.full_url, 403, 'denied', {}, io.BytesIO(b'{"cloudflare_error":true,"title":"Error 1010: Access denied"}'))
        with patch.object(audit, 'build_opener', return_value=Opener()):
            result = audit.ping('unit-test-key')
        self.assertEqual(result['failure_category'], 'cloudflare_access_denied')

    def test_ping_uses_truthful_app_identity_and_checks_pong(self):
        class PingResponse(Response):
            status = 200
            def read(self, size): return b'{"error":null,"data":{"message":"pong"}}'
        class Opener:
            def open(self, request, timeout):
                self.request = request
                return PingResponse()
        opener = Opener()
        with patch.object(audit, 'build_opener', return_value=opener):
            self.assertTrue(audit.ping('unit-test-key')['authenticated'])
        self.assertEqual(opener.request.get_header('User-agent'), audit.USER_AGENT)

    def test_non_json_denial_does_not_claim_invalid_key(self):
        class Opener:
            def open(self, request, timeout):
                raise HTTPError(request.full_url, 403, 'denied', {}, io.BytesIO(b'<html>blocked</html>'))
        with patch.object(audit, 'build_opener', return_value=Opener()):
            result = audit.ping('unit-test-key')
        self.assertFalse(result['authenticated'])
        self.assertEqual(result['failure_category'], 'unclassified_access_failure')

if __name__ == '__main__': unittest.main()
