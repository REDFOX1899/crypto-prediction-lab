import hashlib
import json
from pathlib import Path
import sys
import unittest
from unittest.mock import patch, MagicMock
sys.path.insert(0,str(Path(__file__).resolve().parents[1]))
import run_prospective_evaluation as client


class ProspectiveTools(unittest.TestCase):
    def test_generation_pinned_read_hashes_exact_rust_float_bytes(self):
        raw = b'{"items":[1e-8]}'
        name = 'prospective/inputs/v1/'+'a'*64+'/'+hashlib.sha256(raw).hexdigest()+'.json'
        response = MagicMock();response.__enter__.return_value.read.return_value = raw
        with patch.object(client.urllib.request,'urlopen',return_value=response) as fetch:
            self.assertEqual(client.read_object({'name':name,'generation':'42'},'test-only'),{'items':[1e-8]})
            self.assertIn('generation=42',fetch.call_args[0][0].full_url)
        with self.assertRaises(AssertionError):
            client.read_object({'name':'mcp/scopes/v1/'+'a'*64+'.json','generation':'42'},'test-only')

    def test_operator_rebuild_keeps_labels_separate_and_checks_manifest_identity(self):
        envelope = {'report':{'input_hash':'input','experiment_id':'experiment','input_manifest':{'name':'root'}}}
        header = {'decisions':[],'labels':[],'previous_labels':[]}
        manifest = {'input_hash':'input','experiment_id':'experiment','parts':[{'kind':'header','object':{'name':'header'}},{'kind':'labels','object':{'name':'labels'}}]}
        values = {'root':manifest,'header':{'items':[header]},'labels':{'items':[{'result':'yes'}]}}
        with patch.object(client,'read_object',side_effect=lambda r,a:values[r['name']]):
            data,rows,_ = client.download_input(envelope,'test-only')
            self.assertEqual(data['labels'],[{'result':'yes'}]);self.assertEqual(data['decisions'],[]);self.assertEqual(rows,[])
        manifest['experiment_id']='other'
        with patch.object(client,'read_object',return_value=manifest):
            with self.assertRaises(AssertionError):client.download_input(envelope,'test-only')
