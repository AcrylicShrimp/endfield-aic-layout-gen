"""Run bounded scripted contract checks and an untrained graph-model smoke test."""
import argparse
from copy import deepcopy
from fractions import Fraction
import hashlib
import json
from pathlib import Path
import time

import torch

from blueprint_edit_audit import audit, require
from blueprint_edit_input import BlueprintActorCritic, prepare
from native import load_native

ROOT = Path(__file__).resolve().parents[2]


def sha(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def write(path, value):
    path.write_text(json.dumps(value, indent=2) + '\n')


def request_from_initial(witness):
    """Freeze this external fixture's obligations before any editing occurs."""
    terminals = {}
    for network in witness['transport_networks']:
        for terminal in network['terminals']:
            endpoint = terminal['endpoint']
            kind = endpoint['kind']
            name = endpoint['instance'] if kind == 'facility' else endpoint['node']
            key = kind, name, terminal['direction'], network['item']
            value = terminal['rate']
            terminals[key] = terminals.get(key, Fraction()) + Fraction(value['numerator'], value['denominator'])
    return dict(schema_version=1,
                instances=[dict(instance=p['instance'], recipe=p['recipe'], utilization=witness['utilizations'][p['instance']])
                           for p in witness['placements']],
                terminals=[dict(endpoint=dict(kind=kind, **({'instance': name} if kind == 'facility' else {'node': name})),
                                direction=direction, item=item, rate=dict(numerator=value.numerator, denominator=value.denominator))
                           for (kind, name, direction, item), value in sorted(terminals.items())])


def model_smoke(observation):
    torch.manual_seed(17)
    model = BlueprintActorCritic()
    batch = prepare(observation)
    logits, value = model(batch)
    require(torch.isfinite(logits).all().item() and torch.isfinite(value).item(), 'nonfinite graph forward')
    require(len(batch['actions']) == len(logits), 'action alignment differs')
    # Test dataflow, not an environment transition: corrupt only the model's copy
    # of incidence and prove it affects predictions without touching the session.
    altered = deepcopy(observation)
    edge = altered['blueprint']['edges'][0]
    edge['target'] = (edge['target'] + 1) % len(altered['blueprint']['nodes'])
    other_logits, other_value = model(prepare(altered))
    actor_difference = (other_logits - logits).abs().max().item()
    critic_difference = (other_value - value).abs().item()
    require(min(actor_difference, critic_difference) > 1e-8, 'actor/critic ignored original blueprint incidence')
    loss = -torch.log_softmax(logits, dim=0)[0] + value.square()
    loss.backward()
    require(all(p.grad is not None and torch.isfinite(p.grad).all().item() for p in model.parameters()), 'graph backward is incomplete')
    return dict(schema=batch['schema'], trained=False, seed=17,
                facility_entities=len(observation['facilities']), graph_nodes=len(batch['node_static']),
                graph_edges=batch['edge_index'].shape[1], routed_cell_entities=len(batch['path_cells']),
                actions=len(logits), original_blueprint_changes_predictions=True,
                actor_prediction_delta=actor_difference, critic_prediction_delta=critic_difference,
                all_parameter_gradients_finite=True)


def run(spec_path, library, output):
    require(not output.exists(), f'output must be fresh: {output}')
    spec = json.loads(spec_path.read_text())
    require(spec['schema'] == 'blueprint-edit-probe-v1', f'unsupported probe spec: {spec_path}')
    module = load_native(library)
    build = json.loads(module.BlueprintEditSession.build_info_json())
    for relative, expected in build['files_sha256'].items():
        require(sha(ROOT / relative) == expected, f'stale native build: {relative}')
    original = json.loads((ROOT / spec['initial']).read_text())
    request = request_from_initial(original)
    output.mkdir(parents=True)
    write(output / 'request.json', request)
    began = time.monotonic()
    report = dict(schema='blueprint-edit-probe-report-v1', status='running', trained_policy=False, scenarios=[],
                  inputs={str(p.resolve()): sha(p) for p in [spec_path, library, ROOT / spec['initial'],
                          *(ROOT / p for p in spec['catalogs'].values()), Path(__file__),
                          Path(__file__).with_name('blueprint_edit_input.py'), Path(__file__).with_name('blueprint_edit_audit.py')]},
                  native_build=build)
    for scenario in spec['scenarios']:
        directory = output / scenario['name']
        directory.mkdir()
        session = module.BlueprintEditSession.open(str(ROOT), json.dumps(spec['catalogs']), json.dumps(request),
                                                  json.dumps(original), json.dumps(spec['config'] | scenario.get('config', {})))
        initial = json.loads(session.observation_json())
        baseline = audit(original, initial['blueprint'], original)
        write(directory / 'initial-observation.json', initial)
        trace, topology_checks, smoke_checks = [], [], []
        for instruction in scenario['steps']:
            for _ in range(instruction.get('repeat', 1)):
                before = json.loads(session.observation_json())
                if instruction.get('probe') == 'wrong_endpoint':
                    edge = instruction['edge']
                    cells = deepcopy(before['edges'][edge]['path'])
                    cells[-1]['x'] += 1
                    action = dict(kind='route', edge=edge, cells=cells)
                else:
                    action = instruction.get('action', dict(kind='commit'))
                envelope = dict(blueprint_sha256=before['blueprint_sha256'], revision=before['revision'], action=action)
                if instruction.get('probe') == 'stale_revision':
                    envelope['revision'] += 1
                outcome = json.loads(session.step_json(json.dumps(envelope)))
                after = json.loads(session.observation_json())
                require(outcome['status'] == instruction['expect'], f'{scenario["name"]}: {outcome}')
                require(before['blueprint'] == after['blueprint'] == initial['blueprint'], 'immutable blueprint changed')
                require(before['blueprint_sha256'] == after['blueprint_sha256'], 'blueprint hash changed')
                if 'code' in instruction:
                    require(outcome['diagnostics'][0]['code'] == instruction['code'], f'unexpected diagnostic: {outcome}')
                if outcome['status'] == 'rejected':
                    for key in ('revision', 'facilities', 'edges', 'best_metrics', 'draft_factory_validated'):
                        require(before[key] == after[key], f'rejected action mutated {key}')
                if outcome['status'] != 'improved':
                    require(outcome['reward'] == 0., 'non-improvement earned area reward')
                if instruction.get('overlap'):
                    require(bool(after['conflicts']), 'expected retained overlap')
                    try:
                        session.validated_draft_json()
                    except ValueError:
                        pass
                    else:
                        raise ValueError('blueprint.invalid-export: overlapping draft exported as factory')
                if instruction.get('all_routed'):
                    require(all(e['status'] == 'routed' for e in after['edges']), 'original edge remains pending')
                    require(not after['conflicts'], 'conflicts remain')
                if instruction.get('model_smoke'):
                    smoke_checks.append(model_smoke(after))
                    write(directory / 'overlap-observation.json', after)
                if action['kind'] == 'commit' and outcome['status'] != 'rejected':
                    witness = json.loads(session.validated_draft_json())
                    topology_checks.append(audit(original, initial['blueprint'], witness))
                    write(directory / f'validated-{outcome["revision"]}.json', witness)
                trace.append(dict(envelope=envelope, outcome=outcome, pending_edges=sum(e['status'] == 'pending' for e in after['edges']),
                                  conflicts=after['conflicts']))
        final_observation = json.loads(session.observation_json())
        result = json.loads(session.finish_json())
        if 'unfinished_draft' in scenario:
            require(result['unfinished_draft'] == scenario['unfinished_draft'], 'unexpected draft completion status')
        if scenario.get('expect_area_improvement'):
            require(result['best_metrics']['score'][0] < initial['best_metrics']['score'][0], 'no validated area improvement')
        audit(original, initial['blueprint'], result['best'])
        write(directory / 'best.json', result['best'])
        # Independent public complete-factory admission, separate from the draft API.
        verifier = module.FactoryEditSession.open(str(ROOT), str((output / 'request.json').resolve()),
            str((directory / 'best.json').resolve()), json.dumps(spec['catalogs']), json.dumps(dict(proposals=1, elapsed_ms=60000)))
        require(json.loads(verifier.snapshot_json())['factory_validated'], 'final best failed independent native admission')
        write(directory / 'trace.json', trace)
        write(directory / 'final-observation.json', final_observation)
        write(directory / 'result.json', result)
        row = dict(name=scenario['name'], attempts=result['attempts'], original_graph=baseline,
                   commit_topology_audits=topology_checks, model_smoke=smoke_checks,
                   initial_score=initial['best_metrics']['score'], best_score=result['best_metrics']['score'],
                   unfinished_draft=result['unfinished_draft'], best_independently_validated=True,
                   blueprint_sha256=result['blueprint_sha256'])
        report['scenarios'].append(row)
        print(json.dumps(row), flush=True)
    report.update(status='passed', seconds=time.monotonic() - began)
    report['artifacts'] = {str(p.relative_to(output)): sha(p) for p in sorted(output.rglob('*.json'))}
    write(output / 'report.json', report)
    return report


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--spec', type=Path, required=True)
    parser.add_argument('--native-library', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    torch.set_num_threads(1)
    torch.use_deterministic_algorithms(True)
    run(args.spec.resolve(), args.native_library.resolve(), args.output.resolve())


if __name__ == '__main__':
    main()
