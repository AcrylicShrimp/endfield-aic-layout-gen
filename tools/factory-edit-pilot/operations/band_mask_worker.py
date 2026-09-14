"""Frozen-policy mask evaluation on the existing persistent worker transport."""
from copy import copy
import os
from pathlib import Path
import sys

SOURCE = Path(os.environ.get('AIC_MASK_SOURCE_ROOT', Path(__file__).resolve().parents[3]))
sys.path.insert(0, str(SOURCE / 'tools/factory-edit-pilot'))

from blueprint_execution_worker import main
from blueprint_ppo_store import ROOT, checked, read
from blueprint_ppo_update import digest, require
from declarative_learning_evaluate import evaluate
from declarative_learning_worker import Worker as BaseWorker
from band_mask_policy import EvaluationRuntime, MaskedActor


class Worker(BaseWorker):
    def execute(self, command):
        operation = command['operation']
        require(operation in ('initialize', 'model', 'evaluate', 'model_digest'),
            'band-mask.evaluation-only: training operations are disabled')
        if operation == 'initialize':
            result = super().execute(command)
            self.components = read(ROOT / self.runtime.catalogs['components'])
            return result
        if operation == 'model_digest':
            return dict(model_sha256=digest(self.model.state_dict()))
        if operation != 'evaluate':
            return super().execute(command)
        require(self.before is None and self.model is not None and not command.get('uniform', False),
            'band-mask.evaluation: idle worker with trained frozen weights required')
        case = command['case']
        require(case['mask_variant'] in ('original', 'masked'), 'band-mask.variant: unknown policy condition')
        identity = case['directory'], case['files']['request']['sha256'], case['files']['initial']['sha256']
        if identity not in self.case_cache:
            root = Path(case['directory'])
            self.case_cache[identity] = read(checked(case['files']['request'], root)), read(checked(case['files']['initial'], root))
        base = copy(self.runtime)
        base.request, base.original = self.case_cache[identity]
        runtime = EvaluationRuntime(base, self.components, case['mask_variant'] == 'masked')
        row = evaluate(runtime, MaskedActor(self.model, runtime), case, command)
        row['band_mask'] = runtime.metrics()
        return dict(row=row)


if __name__ == '__main__':
    main(Worker)
