"""Freeze the main input/panel/runtime contract only after measured readiness."""
import os
from pathlib import Path
import platform
import sys
import time

import torch

from blueprint_execution_resources import cpu_allocation
from blueprint_evaluation_cohort import load_cohort
from blueprint_ppo_store import ROOT, read, reference, sha, write
from blueprint_ppo_update import digest
from blueprint_replay_contract import Sources, check
from blueprint_replay_panel import prepare_panel
from blueprint_replay_runtime import admitted_sources, child_config
from blueprint_training_input import source_paths


def runtime_sources():
    paths = list((ROOT / 'tools/factory-edit-pilot').glob('*.py'))
    paths += [ROOT / 'tools/factory-edit-pilot/blueprint-replay-balance.json']
    return {str(path.relative_to(ROOT)): sha(path) for path in sorted(paths)}


def prepare(sources, measurement, verification, output, spec, panel_path=None):
    began = time.monotonic()
    output = Path(output).resolve()
    check(not output.exists(), 'source-invalid', 'launch', 'fresh main output required', output)
    output.mkdir(parents=True)
    source = Sources()
    measured = read(source.path(measurement))
    verified = read(source.path(verification))
    check(measured['throughput_passed'] and measured['spec_sha256'] == digest(spec)
        and verified['status'] == 'complete' and verified['device'] == measured['device']
        and verified['measurement_sha256'] == sha(measurement) and verified['spec_sha256'] == digest(spec),
        'execution-gate', 'launch', 'readiness or restored fork not complete for this specification')
    check(bool(measured.get('measured_sources')), 'execution-gate', 'launch', 'missing measured runtime source hashes')
    for name, expected in measured['measured_sources'].items():
        source.path(Path(__file__).with_name(name), expected)
    allocation = cpu_allocation(8)
    check(all(allocation[k] == measured['allocation'][k] for k in ('workers', 'coordinator', 'effective_quota')),
        'execution-gate', 'launch', 'CPU settings changed after readiness')
    parent, imports, binding = admitted_sources(sources, 17)
    check(binding == verified['binding'], 'parent-incompatible', 'launch', 'verified fork uses a different parent binding')
    config = child_config(parent, spec, output / 'training-inputs')
    probe = read(ROOT / config['probe_spec'])
    historical, historical_sources = load_cohort(config['evaluation']['cohort'], 32,
        {k: ROOT / p for k, p in probe['catalogs'].items()})
    if panel_path is None:
        panel = prepare_panel(parent, imports, historical, output / 'transfer-panel', spec['transfer_panel'], allocation['workers'])
    else:
        panel = read(source.path(panel_path))
        check(panel['schema'] == 'blueprint-replay-transfer-panel-v1' and panel['options'] == spec['transfer_panel']
            and len(panel['cases']) == 16, 'evaluation-contaminated', 'launch', 'prebuilt transfer panel contract differs')
        for case in panel['cases']:
            for ref in case['files'].values():
                source.reference(Path(case['directory']), ref)
        write(output / 'transfer-panel/panel.json', panel)
    bindings = {}
    for seed in spec['seeds']:
        selected, _, bound = admitted_sources(sources, seed)
        check(selected['parent_protocol']['config']['training_input']['seed_base'] != spec['training_inputs']['seed_base'],
            'input-namespace', 'launch', 'child generation seed base reuses parent')
        bindings[str(seed)] = bound
    frozen = runtime_sources()
    native = parent['parent_protocol']['native_library']
    external = dict(source.bindings, **historical_sources)
    required = [ROOT / config['probe_spec'], *(ROOT / path for path in probe['catalogs'].values()),
        *source_paths(config), ROOT / 'tools/factory-edit-pilot/pyproject.toml', ROOT / 'tools/factory-edit-pilot/uv.lock']
    for local in required:
        check(local.is_file(), 'source-invalid', 'launch', 'runtime input missing', local)
        frozen[str(local.resolve().relative_to(ROOT))] = sha(local)
    launch = dict(schema=spec['schema'], spec=spec, source_directory=str(Path(sources).resolve()),
        bindings=bindings, device=measured['device'], allocation=allocation, readiness=measured,
        verification_reference=dict(path=str(Path(verification).resolve()), sha256=sha(verification)),
        historical=historical, transfer_panel=reference(output / 'transfer-panel/panel.json', output),
        native_library=native, native_sha256=sha(native), source_root=str(ROOT), sources=frozen,
        external_sources=external, training_input_directory=str(output / 'training-inputs'),
        runtime=dict(python=sys.version, torch=torch.__version__, platform=platform.platform(),
            cuda=torch.version.cuda, device_name=torch.cuda.get_device_name(0)),
        prepared_at=time.time(), order=[dict(seed=seed, arm=arm) for seed in spec['seeds'] for arm in spec['order'][str(seed)]])
    write(output / 'launch.json', launch)
    write(output / 'state.json', dict(schema='blueprint-replay-matrix-state-v1', status='prepared',
        launch_sha256=sha(output / 'launch.json'), jobs={}, seconds=time.monotonic() - began))
    return launch


def check_launch(launch, sources):
    check(launch['source_root'] == str(ROOT), 'execution-gate', 'launch', 'runtime root differs from frozen launch')
    for path, expected in launch['sources'].items():
        sources.path(ROOT / path, expected)
    for path, expected in launch['external_sources'].items():
        sources.path(path, expected)
    sources.path(launch['native_library'], launch['native_sha256'])
    current = cpu_allocation(8)
    check(all(current[k] == launch['allocation'][k] for k in ('workers', 'coordinator', 'effective_quota')),
        'execution-gate', 'launch', 'worker/coordinator allocation or quota changed')
    check(launch['device'] == launch['readiness']['device'] and launch['readiness']['throughput_passed'],
        'execution-gate', 'launch', 'device/readiness mismatch')
    if launch['device'].startswith('cuda'):
        check(torch.cuda.is_available() and torch.cuda.get_device_name(0) == launch['runtime']['device_name'],
            'execution-gate', 'launch', 'selected CUDA device unavailable or changed')
