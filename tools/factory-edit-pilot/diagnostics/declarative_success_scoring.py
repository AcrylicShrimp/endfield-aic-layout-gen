"""Offline success-retention ranking; never changes rewards, buffers or a learner.

Capture decodes each referenced chunk once using the established loader. Analyze
needs only the compact JSON export and the Python standard library.
"""
import argparse
from collections import Counter, defaultdict
import gzip
import hashlib
import json
import math
from pathlib import Path
import statistics
import sys
import time

SCHEMA = 'declarative-success-scoring-input-v1'
sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from declarative_success_features import FIELDS, bounds, extent, witness_extent, validate_row, features, score


def require(ok, code, message):
    if not ok:
        raise ValueError(f'success-scoring.{code}: {message}')


def read(path):
    with (gzip.open(path, 'rt') if str(path).endswith('.gz') else open(path)) as f:
        return json.load(f)


def write(path, value):
    path = Path(path)
    path.parent.mkdir(parents=True, exist_ok=True)
    with (gzip.open(path, 'wt', compresslevel=3) if str(path).endswith('.gz') else open(path, 'w')) as f:
        json.dump(value, f, allow_nan=False)
        f.write('\n')


def capture(args):
    sys.path.insert(0, str(Path(args.runtime).resolve()))
    from blueprint_ppo_store import load
    began = time.perf_counter()
    root = Path(args.run)
    job = read(root / 'state.json')
    config = read(root.parent / 'launch.json')['config']
    checkpoint = load(root / job['latest_checkpoint']['path'])
    require(checkpoint['update'] == job['update'], 'checkpoint', 'Published update mismatch')
    entries = list(checkpoint['buffer']['entries'].values())
    needed = defaultdict(set)
    for entry in entries:
        for label in entry['labels']:
            needed[label['reference'][0]].add(label['reference'][1])
    samples, sources, source_bytes = {}, [], 0
    for ordinal, indices in sorted(needed.items()):
        ref = job['replay_sources'][str(ordinal)]
        path = root / ref['path']
        source_bytes += path.stat().st_size
        chunk = load(path)
        sources.append(dict(update=ordinal, **ref))
        for index in indices:
            sample = chunk['samples'][index]
            witness = sample['validated_witness']
            samples[(ordinal, index)] = dict(
                action=sample['action'], reward=sample['reward'],
                step=sample['policy_action'], best_before=sample['observation']['best_metrics'],
                before=[[p[k] for k in FIELDS] for p in sample['observation']['facilities']],
                final=None if witness is None else [[p[k] for k in FIELDS] for p in witness['placements']],
                final_extent=None if witness is None else witness_extent(witness))
        del chunk
    rows = []
    for entry in entries:
        selected = [samples[tuple(l['reference'])] for l in entry['labels']]
        last = selected[-1]
        require(last['reward'] > 0 and last['final'] is not None, 'not-success', entry['id'])
        before_area = selected[0]['best_before']['score'][0]
        after_area = math.prod(last['final_extent'])
        require(0 < after_area < before_area, 'area', f"{entry['id']}: {before_area} -> {after_area}")
        aliases = {p[0]: i for i, p in enumerate(selected[0]['before'])}
        def geometry(layout):
            require(set(p[0] for p in layout) == set(aliases), 'identity', entry['id'])
            return sorted([[aliases[p[0]], *p[1:]] for p in layout])
        actions, places = [], []
        for index, sample in enumerate(selected):
            action = dict(sample['action'])
            if action['kind'] == 'place':
                action['instance'] = aliases[action['instance']]
                # The next changed-action label's before state is this label's
                # actual after geometry. Rejected/unchanged attempts are omitted.
                after = selected[index + 1]['before'] if index + 1 < len(selected) else last['final']
                places.append(dict(label=index, instance=action['instance'],
                    before=geometry(sample['before']), after=geometry(after)))
            actions.append(dict(step=sample['step'], action=action))
        product = entry['input']['draw']['product']
        matches = [s for s, products in config['strata'].items() if product in products]
        require(len(matches) == 1, 'stratum', product)
        rows.append(dict(id=entry['id'], stratum=matches[0], product=product,
            episode=[entry['stream'], entry['episode']], geometry=entry['input']['geometry_sha256'],
            depth=entry['input']['start_curriculum']['depth'],
            admitted_update=entry['admitted_update'], uses=entry['uses'], utility=entry.get('utility'),
            label_references=[l['reference'] for l in entry['labels']], actions=actions, places=places,
            before_area=before_area, after_area=after_area,
            before_extent=[selected[0]['best_before']['width'], selected[0]['best_before']['height']],
            after_extent=last['final_extent'], before=geometry(selected[0]['before']), final=geometry(last['final']),
            native_validated=True))
    result = dict(schema=SCHEMA, update=checkpoint['update'], entries=rows,
        provenance=dict(run=str(root), checkpoint=job['latest_checkpoint'], sources=sources,
            retention=config['success']['retention'], strata=config['strata'],
            source_bytes=source_bytes, decoded_chunks=len(sources), unique_samples=len(samples),
            capture_seconds=time.perf_counter() - began))
    write(args.output, result)
    print(json.dumps(dict(update=result['update'], entries=len(rows), **{
        k:result['provenance'][k] for k in ('capture_seconds', 'source_bytes', 'decoded_chunks')})))


def summarize(rows):
    return dict(sequences=len(rows), families=dict(Counter(r['family'] for r in rows)),
        place_sequences=sum(r['has_place'] for r in rows),
        center_positive=sum(r['center_inward'] > 1e-12 for r in rows),
        exterior_positive=sum(r['exterior_release'] > 1e-12 for r in rows),
        center_without_exterior=sum(r['center_inward'] > 1e-12 and r['exterior_release'] <= 1e-12 for r in rows),
        mean_gain=statistics.mean(r['gain'] for r in rows) if rows else None,
        geometries=len({r['geometry'] for r in rows}),
        unique_episodes=len({tuple(r['episode']) for r in rows}),
        mean_labels=statistics.mean(r['labels'] for r in rows) if rows else None)


def analyze(args):
    began = time.perf_counter()
    data, config = read(args.input), read(args.config)
    require(data['schema'] == SCHEMA, 'schema', args.input)
    require(config['schema'] == 'declarative-success-scoring-experiment-v1', 'config', args.config)
    rows = [features(row) for row in data['entries']]
    require(len({r['id'] for r in rows}) == len(rows), 'duplicates', args.input)
    groups = {s:[r for r in rows if r['stratum'] == s] for s in sorted({r['stratum'] for r in rows})}
    for group in groups.values():
        fraction = sum(r['has_place'] for r in group) / len(group)
        for row in group:
            row['rarity'] = (1 - fraction) if row['has_place'] else 0.
    profiles = config['profiles']
    require(all(set(w) in ({'rare', 'center', 'exterior'}, {'rare', 'center', 'exterior', 'cap_geometry'}) and
        isinstance(w.get('cap_geometry', False), bool) and
        all(isinstance(w[k], (int, float)) and math.isfinite(w[k]) and w[k] >= 0 for k in ('rare','center','exterior'))
        for w in profiles.values()), 'weights', args.config)
    selections = []
    for budget in config['budgets_per_stratum']:
        require(isinstance(budget, int) and 0 < budget <= min(map(len, groups.values())), 'budget', str(budget))
        for name, weights in profiles.items():
            for stratum, group in groups.items():
                ranked = sorted(group, key=lambda r:(-score(r, weights), r['id']))
                chosen = ranked[:budget]
                selections.append(dict(profile=name, stratum=stratum, budget=budget,
                    **summarize(chosen), selected_ids=[r['id'] for r in chosen]))
    result = dict(schema='declarative-success-scoring-report-v1', update=data['update'], config=config,
        provenance=dict(input=str(Path(args.input).resolve()), input_sha256=hashlib.sha256(Path(args.input).read_bytes()).hexdigest(),
            config_sha256=hashlib.sha256(Path(args.config).read_bytes()).hexdigest(), source=data['provenance']),
        population={s:summarize(g) for s,g in groups.items()}, features=rows, selections=selections,
        analysis_seconds=time.perf_counter() - began,
        scope='Retained-pool ranking only; no counterfactual validity, online retention or policy-performance claim.')
    write(args.output, result)
    print(json.dumps({k:result[k] for k in ('update', 'population', 'analysis_seconds')}))
    for s in selections:
        if s['budget'] == config['budgets_per_stratum'][0]:
            print(json.dumps({k:v for k,v in s.items() if k not in ('selected_ids', 'families')}))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest='command', required=True)
    take = commands.add_parser('capture', help='Read one published buffer and its referenced evidence')
    take.add_argument('--run', required=True)
    take.add_argument('--runtime', required=True, help='Existing source directory containing blueprint_ppo_store.py')
    take.add_argument('--output', required=True)
    analyze_parser = commands.add_parser('analyze', help='Rank a captured pool locally without a learner')
    analyze_parser.add_argument('--input', required=True)
    analyze_parser.add_argument('--config', required=True)
    analyze_parser.add_argument('--output', required=True)
    args = parser.parse_args()
    try:
        (capture if args.command == 'capture' else analyze)(args)
    except (ValueError, KeyError, OSError, TypeError) as error:
        print(json.dumps(dict(code='success-scoring.input', severity='error', stage=args.command,
            message=str(error), entity=getattr(args, 'input', getattr(args, 'run', None)))), file=sys.stderr)
        return 2
    return 0


if __name__ == '__main__':
    raise SystemExit(main())
