"""Admit count-varied serial chains from runtime recipes and existing case templates."""

import argparse
from copy import deepcopy
from fractions import Fraction
import hashlib
import json
from pathlib import Path
import tempfile

from geometry_augmentation import digest, transform, validate
from reference.acceptance import Catalog


def require(condition, code, message):
    if not condition:
        raise ValueError(f'chain_augmentation.{code}: {message}')


def serial_case(root, catalogs, template, key):
    recipes = {r['id']: r for r in json.loads((root/catalogs['recipes']).read_text())['recipes']}
    external = set(json.loads((root/catalogs['recipes']).read_text())['external_items'])
    facilities = {f['id']: f for f in json.loads((root/catalogs['facilities']).read_text())['facilities']}
    ids = template['recipes']
    require(len(ids) >= 2 and all(r in recipes for r in ids), 'recipes', f'{key}: at least two known recipe steps required')
    chain = [recipes[r] for r in ids]
    require(all(len(r['inputs']) == len(r['outputs']) == 1 for r in chain), 'serial', f'{key}: one input and output per step required')
    require(chain[0]['inputs'][0]['item'] in external, 'external', f'{key}: first input is not an admitted raw import')
    require(all(a['outputs'][0]['item'] == b['inputs'][0]['item'] for a, b in zip(chain, chain[1:])),
            'commodity', f'{key}: adjacent recipe commodities differ')
    definitions = [facilities[r['facility']] for r in chain]
    ports = []
    for recipe, definition in zip(chain, definitions):
        require(0 in definition['allowed_rotations'], 'ports', f'{key}: {recipe["id"]}: rotation zero unavailable')
        selected = {}
        for role, edge in (('input', 'south'), ('output', 'north')):
            admitted = [p for p in definition['ports'] if p['direction'] == role and p['transport'] == 'belt' and p['edge'] == edge]
            require(admitted, 'ports', f'{key}: {recipe["id"]}: {edge} {role} belt port unavailable')
            selected[role] = min(admitted, key=lambda p: (abs(2*p['position']['x']-(definition['footprint']['width']-1)), p['id']))
        ports.append(selected)
    rate = Fraction(*template['target_rate'])
    require(rate > 0, 'rate', f'{key}: target rate must be positive')
    rates, utilization = [None]*len(chain), [None]*len(chain)
    for i in reversed(range(len(chain))):
        recipe = chain[i]
        rates[i] = rate
        utilization[i] = rate / Fraction(recipe['outputs'][0]['quantity']*1000, recipe['duration_ms'])
        require(0 < utilization[i] <= 1, 'rate', f'{key}: {recipe["id"]}: required utilization exceeds capacity')
        rate *= Fraction(recipe['inputs'][0]['quantity'], recipe['outputs'][0]['quantity'])
    raw_rate = rate
    poses = [[0, 0, 0] for _ in chain]
    for i in reversed(range(len(chain)-1)):
        consumer = poses[i+1]
        target_x = consumer[0]+ports[i+1]['input']['position']['x']
        target_y = consumer[1]+definitions[i+1]['footprint']['height']
        poses[i] = [target_x-ports[i]['output']['position']['x'], target_y+3, 0]

    def fraction(value):
        return [value.numerator, value.denominator]

    def cell(index, role):
        port = ports[index][role]
        return [poses[index][0]+port['position']['x'], poses[index][1]+port['position']['y']+(1 if role == 'input' else -1)]

    case = {'schema_version': 1, 'id': key, 'catalogs': deepcopy(catalogs),
            'target': {'item': chain[-1]['outputs'][0]['item'], 'rate': fraction(rates[-1])},
            'facilities': [{'id': f'S{i}', 'recipe': recipe['id'], 'utilization': fraction(utilization[i]), 'pose': poses[i]}
                           for i, recipe in enumerate(chain)], 'networks': []}
    case['networks'].append({'id': 'N0', 'item': chain[0]['inputs'][0]['item'], 'rate': fraction(raw_rate),
        'source': {'external': 'raw-import'}, 'target': {'facility': 'S0'},
        'source_binding': {'cell': cell(0, 'input'), 'side': 'S'}, 'target_binding': {'port': ports[0]['input']['id']},
        'corners': [cell(0, 'input')]})
    for i, recipe in enumerate(chain):
        final = i == len(chain)-1
        source_cell = cell(i, 'output')
        case['networks'].append({'id': f'N{i+1}', 'item': recipe['outputs'][0]['item'], 'rate': fraction(rates[i]),
            'source': {'facility': f'S{i}'}, 'target': {'external': 'product-export'} if final else {'facility': f'S{i+1}'},
            'source_binding': {'port': ports[i]['output']['id']},
            'target_binding': {'cell': source_cell, 'side': 'N'} if final else {'port': ports[i+1]['input']['id']},
            'corners': [source_cell] if final else [source_cell, cell(i+1, 'input')]})
    inlet, outlet = cell(0, 'input'), cell(0, 'output')
    case['witness'] = [
        *({'op': 'UNBIND_TERMINALS', 'terminal': tid} for tid in ('N0:source', 'N0:target', 'N1:source')),
        {'op': 'ERASE_TRANSPORT', 'network': 'N0', 'cells': [inlet]},
        {'op': 'ERASE_TRANSPORT', 'network': 'N1', 'cells': [outlet, [outlet[0], outlet[1]-1]]},
        {'op': 'SET_PLACEMENTS', 'assignments': {'S0': [poses[0][0], poses[0][1]-2, 0]}},
        {'op': 'BIND_TERMINALS', 'terminal': 'N0:target', 'binding': {'port': ports[0]['input']['id']}},
        {'op': 'BIND_TERMINALS', 'terminal': 'N1:source', 'binding': {'port': ports[0]['output']['id']}},
        {'op': 'BIND_TERMINALS', 'terminal': 'N0:source', 'binding': {'cell': [inlet[0], inlet[1]-2], 'side': 'S'}}]
    return case


def generate(root, spec_path, output, library):
    spec = json.loads(spec_path.read_text())
    require(spec.get('schema_version') == 1 and set(spec['splits']) == {'train', 'development', 'heldout'}, 'schema', str(spec_path))
    require(not output.exists(), 'output_exists', str(output))
    require(spec['templates'], 'templates', f'{spec_path}: empty templates')
    config = json.loads((root/spec['config']).read_text())
    config.update(device='cpu', horizon=max(config['horizon'], 16))
    inputs = {spec_path, root/spec['config'], library, *(root/p for p in spec['catalogs'].values())}
    rows, payloads, seen = [], {}, set()
    with tempfile.TemporaryDirectory(prefix='aic-chain-augmentation-') as directory:
        directory = Path(directory)
        templates = {}
        for key, definition in spec['templates'].items():
            if 'case' in definition:
                source = root/definition['case']
                inputs.add(source)
                case = json.loads(source.read_text())
                require(case['catalogs'] == spec['catalogs'], 'catalogs', f'{key}: template catalogs differ')
            else:
                case = serial_case(root, spec['catalogs'], definition, key)
            path = directory/f'template-{key}.json'
            path.write_text(json.dumps(case, indent=2)+'\n')
            validate(root, path, config, library)
            templates[key] = Catalog(root, path)
        for split, groups in spec['splits'].items():
            require(groups, 'splits', f'{split}: empty split')
            for group in groups:
                key = group['template']
                require(key in templates and group['gap_extensions'] and group['quarter_turns'], 'splits', f'{split}/{key}: unknown or empty template')
                for gap in group['gap_extensions']:
                    for turns in group['quarter_turns']:
                        case = transform(templates[key], gap, turns)
                        identity = digest({k: v for k, v in case.items() if k not in {'id', 'witness'}})
                        require(identity not in seen, 'duplicate', f'{split}/{key}: gap={gap}, turns={turns}')
                        seen.add(identity)
                        name = f'{split}-{key}-gap{gap}-turn{turns}'
                        case['id'] = name
                        path = directory/f'{name}.json'
                        path.write_text(json.dumps(case, indent=2)+'\n')
                        evidence = validate(root, path, config, library)
                        payloads[path.name] = path.read_bytes()
                        rows.append({'id': name, 'path': path.name, 'split': split, 'template': key,
                            'gap_extension': gap, 'quarter_turns': turns, 'facilities': len(case['facilities']),
                            'networks': len(case['networks']), 'terminals': 2*len(case['networks']),
                            'geometry_sha256': identity, 'case_sha256': hashlib.sha256(payloads[path.name]).hexdigest(), **evidence})
    manifest = {'schema_version': 1, 'stage': 'chain_augmentation', 'status': 'passed',
        'inputs_sha256': {str(p): hashlib.sha256(p.read_bytes()).hexdigest() for p in sorted(inputs)}, 'cases': rows}
    output.mkdir(parents=True)
    for name, payload in payloads.items():
        (output/name).write_bytes(payload)
    (output/'manifest.json').write_text(json.dumps(manifest, indent=2)+'\n')
    return manifest


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--spec', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--native-library', type=Path, required=True)
    args = parser.parse_args()
    try:
        result = generate(Path(__file__).resolve().parents[2], args.spec.resolve(), args.output.resolve(), args.native_library.resolve())
    except (ValueError, KeyError, OSError) as error:
        message = str(error)
        print(json.dumps({'code': message.split(':')[0] if message.startswith('chain_augmentation.') else 'chain_augmentation.input',
                          'severity': 'error', 'stage': 'chain_augmentation', 'message': message, 'entity': str(args.spec)}))
        raise SystemExit(1) from error
    print(json.dumps({'status': result['status'], 'cases': len(result['cases']), 'output': str(args.output)}))


if __name__ == '__main__':
    main()
