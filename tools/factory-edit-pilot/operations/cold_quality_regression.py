"""Explicit post-hoc inspection of retained cold-study evaluations; never trains."""
import argparse
from collections import Counter, defaultdict
import hashlib
import json
from pathlib import Path
import statistics
import sys


PROFILES = ('baseline-8', 'priority-8', 'uniform-16')
UPDATES = (0, 32, 64, 96, 128)


def read(path):
    return json.loads(Path(path).read_text())


def reference(path):
    return dict(path=str(path), sha256=hashlib.sha256(path.read_bytes()).hexdigest())


def collect(root):
    reports = []
    for seed, phase in ((71, 'development'), (113, 'confirmation')):
        for profile in PROFILES:
            owner = root / f'{phase}-seed{seed}-{profile}' / f'seed-{seed}'
            for update in UPDATES:
                path = owner / f'evaluation/{update:04d}/report.json'
                reports.append(dict(seed=seed, phase=phase, profile=profile,
                    update=update, **reference(path), report=read(path)))
    return dict(source_root=str(root), reports=reports)


def behavior(rows):
    counts = Counter()
    for row in rows:
        counts.update(row['counts'])
    return dict(mean_counts={k: v / len(rows) for k, v in sorted(counts.items())},
        unfinished=sum(r['unfinished_draft'] for r in rows),
        guards=dict(Counter(r['guard'] for r in rows)),
        mean_milestone_gain={str(n): statistics.mean(
            100 * (r['start_score'][0] - next(m['best_score'][0]
                for m in r['action_milestones'] if m['policy_action'] == n)) / r['start_score'][0]
            for r in rows) for n in (8, 32, 64)})


def analyze(bundle, evidence):
    # Reuse the study's identity, physical-area, paired-replicate validation
    # and geometry bootstrap instead of defining a second scoring contract.
    from declarative_quality_analysis import aggregate, geometry_outcomes, paired
    result = []
    for seed, phase in ((71, 'development'), (113, 'confirmation')):
        panel = evidence['panels'][phase]
        cases = {c['geometry_identity']: c for c in panel['cases']}
        strata = {s: sorted({c['product'] for c in cases.values() if c['stratum'] == s})
            for s in ('small', 'large')}
        for profile in PROFILES:
            reports = {r['update']: r for r in bundle['reports']
                if r['seed'] == seed and r['profile'] == profile}
            assert set(reports) == set(UPDATES), (seed, profile, 'missing checkpoint')
            outcomes, totals = {}, {}
            for update, report in reports.items():
                rows = report['report']['rows']
                assert report['phase'] == phase and report['report']['update'] == update
                assert all(r['seed'] == seed and r['update'] == update and r['policy_actions'] == 64
                    and r['request_sha256'] == cases[r['geometry_identity']]['files']['request']['sha256']
                    and r['initial_sha256'] == cases[r['geometry_identity']]['files']['initial']['sha256']
                    for r in rows), (seed, profile, update, 'changed evaluation binding')
                outcomes[update] = geometry_outcomes(rows, panel, strata)
                totals[update] = aggregate([outcomes[update]], strata)
            comparisons = {}
            for start in (64, 96):
                geometries = []
                for identity, before in outcomes[start].items():
                    after = outcomes[128][identity]
                    delta = after['mean'] - before['mean']
                    samples = [b - a for a, b in zip(before['action_replicates'], after['action_replicates'])]
                    case = cases[identity]
                    geometries.append(dict(generation_seed=case['generation_seed'], identity=identity,
                        product=case['product'], stratum=case['stratum'], before=before['mean'],
                        after=after['mean'], delta_pp=delta, action_delta_pp=samples,
                        both_actions_decline=all(d < -1e-12 for d in samples)))
                geometries.sort(key=lambda g: (g['delta_pp'], g['generation_seed']))
                losses = [-g['delta_pp'] for g in geometries if g['delta_pp'] < -1e-12]
                gains = [g['delta_pp'] for g in geometries if g['delta_pp'] > 1e-12]
                comparison = paired(totals[128], totals[start], strata)
                comparison['scope'] = ('Post-hoc geometry bootstrap conditional on one training seed and two fixed '
                    'action replicates; does not estimate checkpoint-selection or action-sampling uncertainty.')
                comparison.update(geometries=geometries,
                    gross_loss_pp=sum(losses) / len(cases), gross_gain_pp=sum(gains) / len(cases),
                    top_five_gross_loss_share=sum(losses[:5]) / sum(losses) if losses else 0,
                    both_actions_decline=sum(g['both_actions_decline'] for g in geometries),
                    by_product_delta_pp={p: statistics.mean(g['delta_pp'] for g in geometries if g['product'] == p)
                        for p in sorted({c['product'] for c in cases.values()})})
                comparisons[f'{start}-128'] = comparison
            result.append(dict(seed=seed, phase=phase, profile=profile,
                curve={str(u): dict(mean=totals[u]['mean'], by_stratum=totals[u]['by_stratum'],
                    behavior=behavior(reports[u]['report']['rows'])) for u in UPDATES},
                comparisons=comparisons))
    confirmation = [r for r in result if r['seed'] == 113]
    loss_sets = [{g['generation_seed'] for g in r['comparisons']['96-128']['geometries']
        if g['delta_pp'] < -1e-12} for r in confirmation]
    return dict(schema='cold-quality-posthoc-regression-v1', arms=result,
        confirmation_common_losses=sorted(set.intersection(*loss_sets)),
        reports=[{k: v for k, v in r.items() if k != 'report'} for r in bundle['reports']])


def traces(root):
    # Inflate each retained snapshot once; no native replay, inference, or mutation.
    sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
    from blueprint_ppo_store import load
    records = []
    selected = {'baseline-8': {719200808}, 'priority-8': {719200816}, 'uniform-16': {719200304}}
    for profile in PROFILES:
        # Normal retention keeps raw episodes only for the final evaluation.
        for update in (128,):
            directory = root / f'confirmation-seed113-{profile}/seed-113/evaluation/{update:04d}'
            for row in read(directory / 'report.json')['rows']:
                path = directory / f'factory-{row["generation_seed"]}-{row["sample"]}.pt.gz'
                saved = load(path)
                trace = [t for t in saved['trace'] if t['outcome']['charged']]
                assert len(trace) == row['policy_actions'] == 64, path
                assert saved['result']['best_metrics']['score'] == row['best_score'], path
                assert saved['initial']['initial_metrics']['score'] == row['start_score'], path
                assert saved['result']['factory_validated'], path
                diagnostics, joint = Counter(), Counter()
                steps = []
                for t in trace:
                    codes = [d['code'] for d in t['outcome']['diagnostics']]
                    diagnostics.update(codes)
                    joint[f'{t["action"]["kind"]}/{t["outcome"]["status"]}'] += 1
                    steps.append(dict(n=t['policy_action'], action=t['action'], status=t['outcome']['status'],
                        codes=codes, best_score=t['best_score'], pending=t['pending'], conflicts=t['conflicts']))
                initial_area = row['start_score'][0]
                improvements = [s for i, s in enumerate(steps)
                    if s['best_score'][0] < (steps[i-1]['best_score'][0] if i else initial_area)]
                records.append(dict(profile=profile, update=update, generation_seed=row['generation_seed'],
                    sample=row['sample'], **reference(path), diagnostics=dict(diagnostics), joint=dict(joint),
                    area_gain_steps=[dict(n=s['n'], area=s['best_score'][0]) for s in improvements],
                    steps=steps if row['generation_seed'] in selected[profile] else None))
    return dict(schema='cold-quality-posthoc-traces-v1', records=records)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest='command', required=True)
    for name in ('collect', 'traces'):
        sub.add_parser(name).add_argument('--study-root', type=Path, required=True)
    summary = sub.add_parser('analyze')
    summary.add_argument('--bundle', type=Path, required=True)
    summary.add_argument('--evidence', type=Path, required=True)
    args = parser.parse_args()
    if args.command == 'analyze':
        result = analyze(read(args.bundle), read(args.evidence))
    else:
        result = globals()[args.command](args.study_root)
    print(json.dumps(result, indent=2))


if __name__ == '__main__':
    main()
