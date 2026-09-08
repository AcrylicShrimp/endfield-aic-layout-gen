"""Opened family observation and bounded physical-edit capability; no training."""
import argparse
import gzip
import hashlib
import json
from pathlib import Path
import time

import numpy as np
import torch

from full_factory_environment import FactoryRuntime, FactoryEpisode
from full_factory_policy import FullFactoryPolicy, observe
from full_factory_selection import SelectionMemory, physical_key, action_key, geometry_choice


def sha(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def write(path, value):
    path.write_text(json.dumps(value, indent=2)+'\n')


def run(args):
    inputs = json.loads(args.inputs.read_text())
    catalogs = json.loads(Path(inputs['catalogs']).read_text())
    sources = [args.inputs, args.native_library, Path(inputs['catalogs']), *map(Path, catalogs.values()),
               *Path(__file__).parent.glob('full_factory*.py')]
    # Include every JS dependency of physical action generation and routing.
    import re
    pending = [Path(__file__).with_name('full_factory_executor.mjs')]
    seen = set()
    while pending:
        file = pending.pop().resolve()
        if file in seen:
            continue
        seen.add(file)
        pending.extend((file.parent/p).resolve() for p in re.findall(r"from\s+['\"](\.[^'\"]+)['\"]", file.read_text()))
    sources.extend(seen)
    for case in inputs['rows']:
        for key, filename in [('request', 'request.json'), ('initial', 'initial.json'), ('original', 'original-request.json')]:
            if sha(case[key]) != case['files_sha256'][filename]:
                raise ValueError(f'factory.input-hash: {case[key]}')
            sources.append(Path(case[key]))
    provenance = {str(p.resolve()): sha(p) for p in sources}
    args.output.mkdir()
    write(args.output/'protocol.json', dict(schema_version=1, inputs=str(args.inputs.resolve()), horizon=8,
        episode_seconds=30, route_work=5000, device='cpu', depth=2, width=32, weight_seed=17,
        scope='opened-family capability; untrained forward pass; geometry and internal-pipe probes', provenance=provenance))
    torch.set_num_threads(1)
    torch.manual_seed(17)
    model = FullFactoryPolicy(2).eval()
    runtime = FactoryRuntime(args.native_library, inputs['catalogs'])
    observations, rows = [], []
    try:
        for case in inputs['rows']:
            episode = FactoryEpisode(runtime, case, 8, 30, 5000)
            began = time.perf_counter()
            description = episode.describe()
            obs = observe(episode.state['current'], description, episode.state, 1)
            observed = time.perf_counter()-began
            if any(not np.isfinite(v).all() for v in obs.values() if isinstance(v, np.ndarray)):
                raise ValueError(f'Nonfinite observation: {case["id"]}')
            began = time.perf_counter()
            with torch.no_grad():
                logits = model(obs)
            if len(logits) != len(description['actions']) or not torch.isfinite(logits).all():
                raise ValueError(f'Invalid policy logits: {case["id"]}')
            observations.append(dict(case=case['id'], facilities=case['instances'], nodes=len(obs['nodes']),
                actions=len(logits), raster_shape=list(obs['raster'].shape), observation_seconds=observed,
                inference_seconds=time.perf_counter()-began, pipe_networks=sum(n['transport']=='pipe' for n in episode.state['current']['transport_networks'])))
            # A fresh edit episode gets the same complete budget after the support measurement.
            episode = FactoryEpisode(runtime, case, 8, 30, 5000)
            memory = SelectionMemory(episode.state['current'])
            while not episode.done:
                description = episode.describe()
                w = episode.state['current']
                best_key = physical_key(episode.state['best'])
                keys = [action_key(a, description, w, best_key) for a in description['actions']]
                before = physical_key(w)
                allowed, exhausted = memory.mask(before, keys, description['actions'], True)
                if exhausted or episode.done:
                    break
                selected = geometry_choice(w, description['actions'], allowed)
                episode.step(description['actions'][selected])
                memory.record(before, physical_key(episode.state['current']), keys[selected])
            arms = [('geometry', episode)]
            pipe_episode = FactoryEpisode(runtime, case, 8, 30, 5000)
            d = pipe_episode.describe()
            internal_pipes = {i for i, n in enumerate(pipe_episode.state['current']['transport_networks'])
                              if n['transport']=='pipe' and len({t['endpoint']['instance'] for t in n['terminals'] if t['endpoint']['kind']=='facility'}) > 1}
            actions = [a for a in d['actions'] if a['kind']=='reroute' and a['network'] in internal_pipes][:8]
            # Chain indices change after edits. Each diagnostic action addresses a fresh original.
            for ordinal, action in enumerate(actions):
                probe = FactoryEpisode(runtime, case, 1, 30, 5000)
                probe.step(action)
                arms.append((f'internal-pipe-{ordinal}', probe))
            for arm, result in arms:
                directory = args.output/f'{case["id"]}--{arm}'
                directory.mkdir()
                summary = result.summary()
                with gzip.open(directory/'episode.json.gz', 'wt') as stream:
                    json.dump(summary, stream)
                with gzip.open(directory/'best.json.gz', 'wt') as stream:
                    json.dump(result.state['best'], stream)
                rows.append(dict(case=case['id'], arm=arm, directory=str(directory.resolve()),
                    initial_metrics=summary['initial_metrics'], best_metrics=summary['best_metrics'], selected=summary['selected']))
            print(json.dumps(dict(case=case['id'], observation=observations[-1], edits=episode.selected,
                                  initial=episode.state['initial_metrics']['score'], best=episode.state['best_metrics']['score'])), flush=True)
        for file, expected in provenance.items():
            if sha(file) != expected:
                raise ValueError(f'Changed probe source: {file}')
        write(args.output/'report.json', dict(schema_version=1, status='complete', protocol_sha256=sha(args.output/'protocol.json'),
            observations=observations, rows=rows))
    finally:
        runtime.close()


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--inputs', type=Path, required=True)
    parser.add_argument('--native-library', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    run(parser.parse_args())
