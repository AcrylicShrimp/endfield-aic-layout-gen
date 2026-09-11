"""Compare audited width-40 shared runs with immutable width-32 factorial controls."""
import argparse
from copy import deepcopy
import json
from pathlib import Path
import statistics
import time

from compare_ppo_combinations import check, read, verify_file, sha
from ppo_actor_critic import FIELDS


def load_audit(report_path, inputs_path, manifest, schema, expected_report=None, expected_inputs=None):
    report = read(report_path, manifest, expected_report)
    check(report['schema'] == schema, f'comparison schema: {report_path}')
    identities = read(inputs_path, manifest, expected_inputs or report['input_manifest_sha256'])
    check(manifest[str(inputs_path.resolve())] == report['input_manifest_sha256'], 'audit manifest identity differs')
    for index, (path, digest) in enumerate(identities.items(), 1):
        verify_file(path, manifest, digest)
        if index % 20000 == 0:
            print(dict(audit=str(report_path), verified_files=index), flush=True)
    accepted = sum(len(j['evaluations'][-1]['pairs']) for c in report['cells'] for j in c['jobs'].values())
    check(report['native_final_layouts_verified'] == accepted, 'all admitted final layouts require native verification')
    return report, identities


def verified_read(path, identities, manifest):
    path = Path(path).resolve()
    check(str(path) in identities, f'input was not covered by the comparison audit: {path}')
    return read(path, manifest, identities[str(path)])


def inputs(report, identities, manifest):
    """The per-width audit already requires identical geometry across every cell."""
    root = Path(report['cells'][0]['root'])
    protocol = verified_read(root/'protocol.json', identities, manifest)
    state = verified_read(root/'state.json', identities, manifest)
    geometries = {}
    for job in state['jobs']:
        for ref in job['records']:
            record = read(root/ref['path'], manifest, ref['sha256'])
            for result in record['results']:
                identity = (str(job['seed']), record['phase']['kind'], record['phase']['batch'], result['identity']['index'])
                check(identity not in geometries, 'duplicate generation identity')
                geometries[identity] = tuple(result['generation'].get(k) for k in
                    ('status', 'generation_seed', 'request_sha256', 'initial_sha256', 'geometry_sha256'))
    digest = sha(json.dumps(sorted(geometries.items()), separators=(',', ':')).encode())
    check(len(geometries) == report['matched_input_identities'], 'geometry count differs from audited report')
    if 'input_geometry_sha256' in report:
        check(digest == report['input_geometry_sha256'], 'geometry signature differs from audited report')
    return protocol, geometries, digest


def portable(value, root):
    if isinstance(value, dict):
        return {k: portable(v, root) for k, v in value.items()}
    if isinstance(value, list):
        return [portable(v, root) for v in value]
    return '<runtime>/' + value[len(root)+1:] if isinstance(value, str) and value.startswith(root+'/') else value


def workload_controls(old, new):
    old_config, new_config = deepcopy(old['config']), deepcopy(new['config'])
    for config, schema, width in ((old_config, 'ppo-actor-spatial-reach-v1', 32),
                                  (new_config, 'ppo-shared-capacity-v1', 40)):
        model = config['model']
        check(model.pop('schema') == schema and model.pop('width') == width
              and model.pop('critic_width') == width, 'declared capacity model differs')
        model.pop('actor_critic')
    check(old_config == new_config, 'non-capacity workload or PPO configuration differs')
    check(old['environment'] == new['environment'], 'runtime package or device environment differs')
    old_root = str(Path(old['spec']['cli']).parents[2])
    new_root = str(Path(new['spec']['cli']).parents[2])
    old_spec, new_spec = deepcopy(old['spec']), deepcopy(new['spec'])
    old_spec.pop('model')
    new_spec.pop('model')
    check(portable(old_spec, old_root) == portable(new_spec, new_root), 'native generation/selection specification differs')
    for key in ('native_library', 'cli', 'validator'):
        check(old['sources'][old['spec'][key]] == new['sources'][new['spec'][key]], f'native binary differs: {key}')
    def fixed_sources(protocol, root):
        # Generated cell configs/catalog path maps contain runtime-specific paths.
        # Their semantic specs are checked above; compare original data and code here.
        return {portable(path, root): digest for path, digest in protocol['sources'].items()
                if not path.endswith('.py') and not path.startswith(root+'/artifacts/')}
    fixed_old, fixed_new = fixed_sources(old, old_root), fixed_sources(new, new_root)
    check(fixed_old == fixed_new, 'catalog, request, native source or dependency identities differ')
    def runtime_sources(protocol, root):
        return {path[len(root)+1:]: digest for path, digest in protocol['sources'].items()
                if path.startswith(root+'/') and path.endswith('.py')}
    left, right = runtime_sources(old, old_root), runtime_sources(new, new_root)
    check(set(left) == set(right), 'runtime Python source coverage differs')
    changed = {path: dict(before=left[path], after=right[path]) for path in left if left[path] != right[path]}
    allowed = {'tools/factory-edit-pilot/ppo_spatial_model.py', 'tools/factory-edit-pilot/ppo_harness_tracking.py'}
    check(set(changed) <= allowed, f'unregistered runtime changes: {set(changed) - allowed}')
    return dict(runtime_files_compared=len(left), changed_runtime_sources=changed,
                fixed_source_files_compared=len(fixed_old), catalogs_and_native_sources_identical=True,
                environment=old['environment'], native_binaries_identical=True,
                non_capacity_configuration_identical=True)


def paired(wide, control):
    check(set(wide['jobs']) == set(control['jobs']), 'paired run seeds differ')
    values = {}
    for seed in wide['jobs']:
        a, b = wide['jobs'][seed], control['jobs'][seed]
        check([e['batch'] for e in a['evaluations']] == [e['batch'] for e in b['evaluations']], 'evaluation batches differ')
        for x, y in zip(a['evaluations'], b['evaluations']):
            check([p['generation_seed'] for p in x['pairs']] == [p['generation_seed'] for p in y['pairs']],
                  'paired evaluation admission differs')
        check(set(a['summary']) == set(b['summary']), 'paired metric coverage differs')
        delta = {k: a['summary'][k] - b['summary'][k] for k in a['summary']}
        initial_a, initial_b = a['evaluations'][0]['area_reduction_pp'], b['evaluations'][0]['area_reduction_pp']
        delta['initial_area_reduction_pp'] = initial_a - initial_b
        delta['area_learning_gain_pp'] = delta['area_reduction_pp'] - delta['initial_area_reduction_pp']
        check(delta['training_actions'] == 0, 'matched training action budgets differ')
        values[seed] = delta
    return dict(by_seed=values, mean={k: statistics.mean(v[k] for v in values.values()) for k in next(iter(values.values()))})


def compact(cell):
    return dict(name=cell['name'], factors=cell['factors'], jobs={s: dict(summary=j['summary'], evaluations=j['evaluations'])
                                                              for s, j in cell['jobs'].items()})


def parameter_counts():
    from ppo_actor_critic import treatment
    from ppo_area_model import models
    from ppo_spatial_model import specification, shared_capacity_specification
    counts = {}
    for name, spec in (('shared_40', shared_capacity_specification()),
                       ('shared_32', dict(specification('cnn2'), actor_critic=treatment(True, True, False))),
                       ('separate_32', dict(specification('cnn2'), actor_critic=treatment(False, True, False)))):
        actor, critic = models(dict(model=spec), 17)
        parameters = {id(p): p for module in (actor, critic) for p in module.parameters()}
        counts[name] = sum(p.numel() for p in parameters.values())
    return counts


def compare(baseline_provenance, wide_report, wide_inputs, output):
    check(not output.exists(), f'fresh comparison output required: {output}')
    began = time.monotonic()
    manifest = {}
    provenance = read(baseline_provenance, manifest)
    check(provenance['schema'] == 'ppo-combinations-final-provenance-v1', 'baseline provenance schema')
    old, old_ids = load_audit(Path(provenance['comparison']['path']), Path(provenance['comparison_inputs']['path']),
                             manifest, 'ppo-actor-critic-comparison-v1', provenance['comparison']['sha256'],
                             provenance['comparison_inputs']['sha256'])
    new, new_ids = load_audit(wide_report, wide_inputs, manifest, 'ppo-shared-capacity-comparison-v1')
    check(len(old['cells']) == 8 and len(new['cells']) == 4, 'expected eight controls and four wider cells')
    old_protocol, old_geometry, old_digest = inputs(old, old_ids, manifest)
    new_protocol, new_geometry, new_digest = inputs(new, new_ids, manifest)
    check(old_geometry == new_geometry and old_digest == new_digest, 'factory generation inputs differ across widths')
    controls = workload_controls(old_protocol, new_protocol)
    lookup = {tuple(c['factors'][k] for k in FIELDS): c for c in old['cells']}
    conditions = []
    for cell in new['cells']:
        f = cell['factors']
        check(f['shared_backbone'], 'wider model must use shared encoders')
        models = dict(shared_40=compact(cell))
        deltas = {}
        for shared, name in ((True, 'shared_32'), (False, 'separate_32')):
            control = lookup[(shared, f['critic_history'], f['critic_after_kl'])]
            models[name] = compact(control)
            deltas[name] = paired(cell, control)
        conditions.append(dict(name=cell['name'], critic_history=f['critic_history'],
                               critic_after_kl=f['critic_after_kl'], models=models, wider_minus_control=deltas))
    # Factor contexts are averaged within a seed; they are not extra replications.
    averages = {}
    for control in ('shared_32', 'separate_32'):
        seeds = conditions[0]['wider_minus_control'][control]['by_seed']
        metrics = next(iter(seeds.values()))
        by_seed = {seed: {metric: statistics.mean(c['wider_minus_control'][control]['by_seed'][seed][metric]
                                                 for c in conditions) for metric in metrics} for seed in seeds}
        averages[control] = dict(by_seed=by_seed,
                                mean={k: statistics.mean(v[k] for v in by_seed.values()) for k in metrics})
    report = dict(schema='ppo-shared-capacity-controls-v1', status='complete', conditions=conditions,
                  wider_minus_control_averages=averages, workload_controls=controls,
                  matched_input_identities=len(new_geometry), input_geometry_sha256=new_digest,
                  unique_parameters=parameter_counts(),
                  baseline_report_sha256=provenance['comparison']['sha256'],
                  wide_report_sha256=manifest[str(wide_report.resolve())],
                  source_sha256=sha(Path(__file__).read_bytes()), verified_file_identities=len(manifest),
                  seconds=time.monotonic()-began,
                  limitations=['Two run seeds, each with its own initialization and factory cohort; four contexts are not four independent replications.',
                    'Width changes initial policies and actor capacity; final levels and gains from initialization are both reported.',
                    'Gains from different untrained policies do not isolate a causal change in learning speed.',
                    'Previously completed controls were not run concurrently; elapsed times are descriptive.',
                    'Reused held-out factory cohort; no untouched final confirmation set.',
                    'Critic errors follow each policy trajectory, not common realized returns across architectures.',
                    'Total parameter counts are approximately matched; compute and actor-only capacity are not matched.'])
    output.mkdir(parents=True)
    payload = (json.dumps(manifest, indent=2)+'\n').encode()
    report['input_manifest_sha256'] = sha(payload)
    (output/'inputs.json').write_bytes(payload)
    (output/'report.json').write_text(json.dumps(report, indent=2, allow_nan=False)+'\n')
    print(dict(status='complete', conditions=len(conditions), verified_files=len(manifest), seconds=report['seconds']), flush=True)
    return report


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--baseline-provenance', type=Path, required=True)
    parser.add_argument('--wide-report', type=Path, required=True)
    parser.add_argument('--wide-inputs', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    compare(args.baseline_provenance.resolve(), args.wide_report.resolve(), args.wide_inputs.resolve(), args.output.resolve())
