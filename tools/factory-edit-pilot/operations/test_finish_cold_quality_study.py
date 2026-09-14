"""Administrative handoff rejection and exact cold-study accounting."""
from copy import deepcopy
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

import finish_cold_quality_study as finish
from declarative_quality_branch import PROFILES,write
from declarative_quality_cold import bound


class ClockTests(unittest.TestCase):
    def fixture(self,root):
        config=root/'protocol.json';write(config,{'original':True})
        common=root/'bound.json';write(common,{'immutable':True})
        arms=[]
        for profile in PROFILES:
            path=root/profile/'quality-summary.json';path.parent.mkdir()
            write(path,dict(profile=profile,initialization_seed=71,first_update=0,endpoint=128,updates=128,
                checkpoint=bound(common),branch=bound(common),evaluation=bound(common)))
            arms.append(dict(stage='development',seed=71,profile=profile,summary=bound(path)))
        panels={'development':bound(common),'confirmation':bound(common)}
        selection=root/'selection.json';write(selection,dict(selected=finish.SELECTED,protocol=bound(config),
            panels=panels,source_sha256='source',development=[r['summary'] for r in arms]))
        active=dict(directory=str(root/'confirmation-seed113-baseline-8'),seed=113,first_update=0,endpoint=128,resumed_update=0)
        study=dict(schema='declarative-cold-quality-study-v3',status='running',executed_updates=1024,
            carried_updates=86,discarded_updates=128,arms=arms,active_arm=active,config=bound(config),
            wall_deadline_unix_seconds=finish.OLD_DEADLINE,selection=bound(selection),panels=panels,
            source={'code_sha256':'source'},pure_confirmation={p:bound(common) for p in ['baseline-8',*finish.SELECTED]})
        control=dict(schema='declarative-cold-clock-handoff-v1',branch=active['directory'],
            old_deadline=finish.OLD_DEADLINE,new_deadline=finish.DEADLINE)
        return study,control

    def test_deadline_is_exactly_one_hour_and_expires(self):
        self.assertEqual(finish.remaining(now=finish.OLD_DEADLINE),3600)
        for kwargs in ({'now':finish.DEADLINE},{'now':finish.DEADLINE+1},
                       {'now':finish.OLD_DEADLINE,'deadline':finish.DEADLINE+1}):
            with self.assertRaisesRegex(ValueError,'deadline'):finish.remaining(**kwargs)

    def test_complete_handoff_and_rejection(self):
        with tempfile.TemporaryDirectory() as tmp:
            root=Path(tmp).resolve();study,control=self.fixture(root)
            self.assertEqual(finish.validate_handoff(study,control,root)['selected'],finish.SELECTED)
            for field,value in (('executed_updates',1152),('status','incomplete'),('arms',study['arms'][:-1]),
                                ('pure_confirmation',{})):
                bad=deepcopy(study);bad[field]=value
                with self.assertRaisesRegex(ValueError,'handoff'):finish.validate_handoff(bad,control,root)
            bad=deepcopy(control);bad['new_deadline']+=1
            with self.assertRaisesRegex(ValueError,'handoff'):finish.validate_handoff(study,bad,root)
            selected=finish.read(root/'selection.json');selected['selected']=['score-8','uniform-16']
            write(root/'selection.json',selected);study['selection']=bound(root/'selection.json')
            with self.assertRaisesRegex(ValueError,'selection'):finish.validate_handoff(study,control,root)

    def test_changed_carried_endpoint_is_rejected(self):
        with tempfile.TemporaryDirectory() as tmp:
            root=Path(tmp).resolve();study,control=self.fixture(root)
            write(root/'bound.json',{'changed':True})
            with self.assertRaisesRegex(ValueError,'binding'):finish.validate_handoff(study,control,root)

    def test_append_once_and_complete_exact_budget(self):
        with tempfile.TemporaryDirectory() as tmp:
            root=Path(tmp).resolve();study,_=self.fixture(root)
            for profile in ['baseline-8',*finish.SELECTED]:
                branch=root/f'confirmation-seed113-{profile}';branch.mkdir()
                summary=dict(profile=profile,initialization_seed=113,first_update=0,endpoint=128,updates=128)
                write(branch/'quality-summary.json',summary)
                finish.append_arm(study,branch,summary)
                with self.assertRaisesRegex(ValueError,'accounting'):finish.append_arm(study,branch,summary)
            self.assertEqual((len(study['arms']),study['executed_updates'],study['new_updates'],study['total_cold_updates']),
                             (11,1408,1322,1536))

    def test_failed_partial_counts_saved_prefix_once(self):
        with tempfile.TemporaryDirectory() as tmp:
            root=Path(tmp).resolve();study,_=self.fixture(root)
            owner=Path(study['active_arm']['directory'])/'seed-113';owner.mkdir(parents=True)
            write(owner/'state.json',dict(update=70,latest_checkpoint={'path':'saved'}))
            finish.failed(study,RuntimeError('interrupted'))
            finish.failed(study,RuntimeError('same interruption'))
            self.assertEqual((study['executed_updates'],study['new_updates'],study['total_cold_updates']),
                             (1094,1008,1222))
            self.assertEqual(study['partial_arm']['completed_update'],70)
            self.assertIsNone(study['ranking'])

    def test_process_identity_distinguishes_zombie_from_live_and_pid_reuse(self):
        fields=['Z']+['0']*18+['12345']
        with patch.object(Path,'read_text',return_value='9 (python worker) '+' '.join(fields)):
            self.assertEqual(finish.process(9,'12345'),'Z')
            with self.assertRaisesRegex(ValueError,'PID identity'):finish.process(9,'other')
        with patch.object(Path,'read_text',side_effect=FileNotFoundError):
            self.assertIsNone(finish.process(9,'12345'))


if __name__=='__main__':unittest.main()
