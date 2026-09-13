"""Cold Rust CLI construction and immutable wiring-to-request import for PPO starts."""
from fractions import Fraction
import json
from pathlib import Path
import subprocess
import time

from full_factory_screen import sha, write


def read(path):
    return json.loads(Path(path).read_text())


def rejected(row, status, stage, entity, diagnostics=None):
    row.update(status=status, diagnostics=diagnostics or [dict(code=f'family.{status}', severity='error',
               stage=stage, message=f'{status}; inspect the retained process output', entity=str(entity))])
    return None, row


def process(binary, arguments, milliseconds, directory, name, *, retain_stdout=True):
    began = time.monotonic()
    timed_out = False
    try:
        result = subprocess.run([binary, *arguments], capture_output=True, text=True, timeout=milliseconds / 1000)
        stdout, stderr, code = result.stdout, result.stderr, result.returncode
    except subprocess.TimeoutExpired as error:
        timed_out = True
        stdout, stderr, code = error.stdout or b'', error.stderr or b'', None
        stdout = stdout.decode() if isinstance(stdout, bytes) else stdout
        stderr = stderr.decode() if isinstance(stderr, bytes) else stderr
    if retain_stdout:
        (directory / f'{name}.stdout').write_text(stdout)
    (directory / f'{name}.stderr').write_text(stderr)
    record = dict(code=code, timed_out=timed_out, elapsed_ms=(time.monotonic() - began) * 1000,
                  binary=binary, argv=arguments)
    write(directory / f'{name}.json', record)
    return record, stdout


def original_request(wiring):
    """Obligations come only from complete wiring, never candidate geometry."""
    if wiring.get('schema_version') != 1 or wiring.get('success') is not True:
        raise ValueError('random-factory.wiring: successful schema-1 wiring required')
    nodes = [n for n in wiring['nodes'] if n['kind'] == 'facility']
    members = {n['id'] for n in nodes}
    internal, external = [], []
    for edge in wiring['edges']:
        (internal if edge['source'] in members and edge['target'] in members else external).append(edge)
    graph = dict(placements=[dict(instance=n['id'], recipe=n['recipe'], facility=n['facility']) for n in nodes],
                 internal_requirements=internal, external_requirements=external)
    totals = {}
    for edge in internal + external:
        amount = Fraction(edge['rate']['numerator'], edge['rate']['denominator'])
        if amount <= 0:
            raise ValueError(f'random-factory.wiring-rate: {edge["id"]}')
        for node, direction in ((edge['source'], 'output'), (edge['target'], 'input')):
            key = (node, direction, edge['item'])
            totals[key] = totals.get(key, Fraction()) + amount
    terminals = [dict(endpoint=dict(kind='facility', instance=node) if node in members else dict(kind='external', node=node),
                      direction=direction, item=item, rate=dict(numerator=amount.numerator, denominator=amount.denominator))
                 for (node, direction, item), amount in totals.items()]
    request = dict(schema_version=1,
                   instances=[dict(instance=n['id'], recipe=n['recipe'], utilization=n['work_seconds_per_second']) for n in nodes],
                   terminals=terminals)
    return request, graph, nodes


def accept_case(runtime, request, graph, witness, case_id, directory, elapsed, work):
    directory.mkdir(parents=True)
    for name, value in [('request', request), ('original-request', graph), ('initial', witness)]:
        (directory / f'{name}.json').write_text(json.dumps(value, separators=(',', ':'), ensure_ascii=False, allow_nan=False) + '\n')
    case = dict(id=case_id, dialect='closure-factory', instances=len(request['instances']),
                request=str(directory / 'request.json'), initial=str(directory / 'initial.json'),
                original=str(directory / 'original-request.json'))
    episode = runtime.open_episode(case, 1, elapsed, work)
    # Opening validates immutable terminal margins and complete physical geometry.
    write(directory / 'reference.json', dict(schema_version=2, backend=runtime.metadata['backend'],
          factory_validated=True, request_sha256=sha(case['request']), metrics=episode.summary()['initial_metrics']))
    case['files_sha256'] = {name: sha(directory / name) for name in
                           ('request.json', 'initial.json', 'original-request.json', 'reference.json')}
    return case, episode


def admit(runtime, spec, chosen, directory, guard):
    directory.mkdir()
    generation, catalogs = spec['generation'], spec['catalogs']
    source = read(chosen['source'])
    source['target'].update(chosen['rate'])
    plan = directory / 'source-plan.json'
    write(plan, source)
    flow, stdout = process(spec['cli'], ['recipes', 'contextual-instance-wiring', '--file', catalogs['recipes'],
        '--request', str(plan)], generation['flow_elapsed_ms'], directory, 'flow-process')
    row = dict(directory=str(directory), status='pending', factory_validated=False, diagnostics=[], flow_ms=flow['elapsed_ms'])
    if flow['timed_out']:
        return rejected(row, 'flow-budget', 'wiring', plan)
    try:
        wiring = json.loads(stdout)
    except json.JSONDecodeError:
        if flow['code'] == 0:
            raise ValueError(f'random-factory.wiring-json: {directory / "flow-process.stdout"}') from None
        wiring = {}
    if flow['code'] != 0 or wiring.get('success') is not True:
        return rejected(row, 'flow-rejected', 'wiring', plan, wiring.get('diagnostics'))
    write(directory / 'wiring.json', wiring)
    request, graph, nodes = original_request(wiring)
    row['facilities'] = len(nodes)
    if len(nodes) > generation['max_facilities']:
        return rejected(row, 'instance-guard', 'admission', plan)
    targets = {n['id'] for n in wiring['nodes'] if n['kind'] == 'target'}
    target = next(e['source'] for e in wiring['edges'] if e['target'] in targets)
    c = generation['construction']
    arguments = ['layouts', 'construct-production', '--recipes', catalogs['recipes'], '--source-plan', str(plan),
        '--facility-catalog', catalogs['facilities'], '--item-catalog', catalogs['items'],
        '--transport-catalog', catalogs['transports'], '--logistics-component-catalog', catalogs['components'],
        '--target-instance', target]
    for key in ('max_steps', 'max_placement_attempts', 'max_route_searches', 'analysis_workers'):
        arguments.extend(['--' + key.replace('_', '-'), str(c[key])])
    arguments.extend(['--downstream-closed-growth', '--final-json'])
    guard()
    result, stdout = process(spec['cli'], arguments, c['elapsed_ms'], directory, 'construction-process',
                             retain_stdout=False)
    row['construction_ms'] = result['elapsed_ms']
    if result['timed_out']:
        return rejected(row, 'construction-budget', 'construction', plan)
    try:
        report = json.loads(stdout)
    except json.JSONDecodeError:
        if result['code'] == 0:
            raise ValueError(f'random-factory.construction-json: {directory}') from None
        return rejected(row, 'construction-rejected', 'construction', plan)
    if report.get('schema_version') != 1 or report.get('kind') != 'constructive-production-final':
        raise ValueError(f'random-factory.construction-schema: {directory}')
    row['construction_status'] = report['status']
    if result['code'] != 0 or report.get('success') is not True or report.get('complete') is not True:
        if 'input_failure' in report:
            row['construction_input_failure'] = report['input_failure']
        return rejected(row, 'construction-stopped', 'construction', plan, report['diagnostics'])
    final = report['final_node']
    witness = dict(schema_version=1, **{k: final[k] for k in ('placements', 'transport_networks', 'logistics_components')},
                   utilizations={n['id']: n['work_seconds_per_second'] for n in nodes})
    validation_started = time.monotonic()
    case, _ = accept_case(runtime, request, graph, witness, f'random-factory-{chosen["product"]}-rate-0',
                          directory / 'input', generation['setup_seconds'], spec['selection']['route_work'])
    row.update(status='accepted', factory_validated=True, validation_ms=(time.monotonic() - validation_started) * 1000)
    return case, row
