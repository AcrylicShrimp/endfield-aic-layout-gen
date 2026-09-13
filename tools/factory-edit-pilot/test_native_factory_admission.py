"""Construction stdout stays in memory; input and failure contracts remain explicit."""
import json
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch

import native_factory_admission as admission


class AdmissionOutputTests(unittest.TestCase):
    def test_process_consumes_stdout_without_creating_a_copy(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            result, stdout = admission.process(sys.executable,
                ['-c', 'print("{\\"complete\\":true}")'], 5000, root, 'construction-process',
                retain_stdout=False)
            self.assertEqual(result['code'], 0)
            self.assertTrue(json.loads(stdout)['complete'])
            self.assertFalse((root / 'construction-process.stdout').exists())
            self.assertEqual(json.loads((root / 'construction-process.json').read_text())['code'], 0)

    def test_admission_requires_final_schema_and_preserves_structured_failure(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            source = root / 'source.json'
            source.write_text('{"target":{}}')
            chosen = dict(source=str(source), rate=dict(quantity=1, duration_ms=1000), product='test')
            spec = dict(cli='aic-cli', catalogs={k:k for k in ('recipes','facilities','items','transports','components')},
                generation=dict(flow_elapsed_ms=1000,max_facilities=20,construction=dict(elapsed_ms=1000,
                    max_steps=20,max_placement_attempts=20,max_route_searches=20,analysis_workers=1)))
            wiring = dict(schema_version=1,success=True,nodes=[{'id':'target','kind':'target'}],
                          edges=[{'source':'facility','target':'target'}])
            diagnostic = dict(code='constructive.exhausted',severity='error',stage='growth',entity='facility',message='Bound exhausted')
            result = dict(schema_version=1,kind='constructive-production-final',status='stopped',
                          success=False,complete=False,final_node=None,diagnostics=[diagnostic],
                          input_failure={'diagnostics':[{'code':'catalog.invalid-schema'}]})
            with patch.object(admission,'original_request',return_value=({}, {}, [{}])), \
                    patch.object(admission,'process',side_effect=[
                        ({'code':0,'timed_out':False,'elapsed_ms':1},json.dumps(wiring)),
                        ({'code':1,'timed_out':False,'elapsed_ms':1},json.dumps(result))]) as call:
                case, row = admission.admit(None,spec,chosen,root/'admission',lambda:None)
            self.assertIsNone(case)
            self.assertEqual(row['diagnostics'],[diagnostic])
            self.assertEqual(row['construction_input_failure'],result['input_failure'])
            command = call.call_args_list[-1]
            self.assertIn('--final-json',command.args[1])
            self.assertNotIn('--report-output',command.args[1])
            self.assertFalse(command.kwargs['retain_stdout'])
            self.assertFalse(list((root/'admission').glob('construction.*')))


if __name__ == '__main__':
    unittest.main()
