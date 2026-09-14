"""Bounded recovery metadata, immutable actual-input coverage and slot rejection."""
from copy import deepcopy
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

from blueprint_ppo_store import read,reference,write
from declarative_input_history import initialize,materialize,register


class InputHistoryTests(unittest.TestCase):
    def test_resume_keeps_history_once_and_rejects_changed_or_recycled_slots(self):
        with tempfile.TemporaryDirectory() as tmp:
            root=Path(tmp);old={'0':{'actual':'legacy'},'1':{'actual':'legacy-1'}}
            job=dict(inputs=deepcopy(old),records=[])
            initialize(job,root)
            prefix=job['input_history_prefix'];first={'2':{'actual':'a'},'3':{'actual':'b'}}
            register(job,first)
            path=root/'updates/0001.json';write(path,dict(inputs=first))
            job['records'].append(reference(path,root));write(root/'state.json',job)
            resumed=read(root/'state.json')
            with patch('declarative_input_history.read',side_effect=AssertionError('history scan during resume')):
                initialize(resumed,root)
                register(resumed,first)
            self.assertEqual(resumed['input_count'],4)
            self.assertEqual(resumed['input_history_prefix'],prefix)
            self.assertEqual(materialize(resumed,root),dict(old,**first))
            with self.assertRaisesRegex(ValueError,'changed or recycled'):
                register(resumed,{'2':{'actual':'replacement'}})
            register(resumed,{'4':{'actual':'next'},'5':{'actual':'next-1'}})
            self.assertEqual(len(resumed['inputs']),2)
            self.assertEqual(resumed['input_count'],6)
            with self.assertRaisesRegex(ValueError,'changed or recycled'):register(resumed,first)


if __name__=='__main__':unittest.main()
