"""Audit and explain post-pose recovery versus actual policy continuation."""
import argparse
from collections import Counter, defaultdict
from pathlib import Path
import tempfile

import matplotlib
matplotlib.use('Agg')
import matplotlib.pyplot as plt

from audit_blueprint_ppo import WitnessAudit
from blueprint_ppo import check_sources
from blueprint_ppo_environment import Runtime, semantic
from blueprint_ppo_store import ROOT, checked, load, read, reference, sha, write
from blueprint_ppo_update import require
from plot_blueprint_edit import draw
from post_pose_recovery import geometry, restore_verified
from probe_post_pose_recovery import continuation


def bounds(*values):
    points = []
    for value in values:
        facilities = value.get('facilities', value.get('placements', []))
        points.extend((p['x'] + dx, p['y'] + dy) for p in facilities for dx, dy in ((0, 0), (p['width'], p['height'])))
        if 'nodes' in value:
            points.extend((n['position']['x'], n['position']['y']) for n in value['nodes'])
        else:
            points.extend((p['x'], p['y']) for n in value['transport_networks'] for p in n['cells'])
    return min(x for x, _ in points) - 1, max(x for x, _ in points) + 1, min(y for _, y in points) - 1, max(y for _, y in points) + 1


def summarize(output):
    protocol, report = read(output / 'protocol.json'), read(output / 'report.json')
    require(report['status'] == 'complete' and report['protocol_sha256'] == sha(output / 'protocol.json'), 'completed bound probe report')
    for path, expected in protocol['diagnostic_sources'].items():
        require(sha(ROOT / path) == expected, f'probe source identity: {path}')
    source = Path(protocol['source_run'])
    source_protocol = read(checked(protocol['source_protocol'], source))
    checked(protocol['source_state'], source)
    checked(protocol['source_audit'], source)
    check_sources(source_protocol)
    runtime = Runtime(source_protocol['config'], source_protocol['native_library'])
    episodes = load(checked(report['source_episodes'], output))['episodes']
    rows = report['cases']
    failures, by_seed, by_start, overlaps = Counter(), defaultdict(Counter), defaultdict(Counter), Counter()
    unresolved = Counter()
    actual_recovery, missed, selected = [], [], {}
    source_refs = {}
    with tempfile.TemporaryDirectory(prefix='post-pose-recheck-') as temporary:
        checker = WitnessAudit(runtime, temporary)
        for row in rows:
            case = load(checked(row['case'], output))
            require(case['identity'] == {k: row[k] for k in case['identity']}, 'case/report source identity')
            source_refs[case['source_chunk']['path']] = case['source_chunk']
            require(case['source_semantic'] == semantic(case['after']), 'retained post-pose semantic identity')
            restore_verified(runtime, case['checkpoint'], case['after'])
            require(geometry(case['after']) == case['recovery']['geometry'], 'geometry classification')
            require(row['continuation'] == continuation(row, episodes[row['seed'], row['stream'], row['episode']]), 'actual continuation classification')
            following = row['continuation']
            event = following['boundary_action']
            if following['next_boundary'] == 'valid_commit':
                require(row['classification'] not in ('overlap_requires_pose', 'endpoint_requires_pose'), 'fixed-pose commit cannot clear a hard geometry blocker')
                require(event['commit_score'] is not None, 'actual complete commit score recorded')
                actual_recovery.append(dict(name=row['name'], classification=row['classification'], score=event['commit_score'],
                    improves_score=event['commit_score'] < row['start_score'], improves_area=event['commit_score'][0] < row['start_score'][0],
                    source_chunk=event['source_chunk'], stream=row['stream'], policy_action=event['policy_action']))
            for branch in case['recovery']['branches']:
                require(branch['expansions'] == sum(t['outcome']['details'].get('route_expansions', 0) for t in branch['actions']), 'charged branch routing work')
                require(branch['expansions'] <= branch['work_allowance'], 'total branch expansion bound')
                if branch['status'] == 'validated':
                    checker.check(case['after']['blueprint'], branch['witness'])
                    require(branch['witness']['placements'] == case['after']['facilities'], 'successful fixed-pose witness')
                    require(branch['score'] == branch['actions'][-1]['outcome']['details']['native_acceptance']['metrics']['score'], 'recovery score identity')
                elif branch['name'] == 'original':
                    for diagnostic in branch['actions'][-1]['outcome']['diagnostics'] if branch['actions'] else []:
                        failures[diagnostic['code'] + ': ' + diagnostic['message']] += 1
                    if row['classification'] == 'bounded_routing_unresolved':
                        routes = branch['actions'][0]['outcome']['details'].get('routes', [])
                        stopped = routes[-1]['result']['status'] if routes else 'no_route_result'
                        if stopped == 'same-endpoint':
                            edge = case['after']['blueprint']['edges'][routes[-1]['edge']]
                            a, b = [case['after']['nodes'][edge[end]]['position'] for end in ('source', 'target')]
                            require(a == b and not edge['zero_length'], 'original distinct endpoints collapsed at this pose')
                            unresolved['original_distinct_endpoints_collapsed'] += 1
                        elif branch['actions'][-1]['action']['kind'] == 'commit':
                            unresolved['complete_validation_failed'] += 1
                        else:
                            unresolved[stopped] += 1
            for counter in (by_seed[str(row['seed'])], by_start[row['start']]):
                counter['poses'] += 1
                counter[row['classification']] += 1
                counter['recovered_score_improvements'] += row['improves_score']
                counter['recovered_area_improvements'] += row['improves_area']
                counter['boundary/' + following['next_boundary']] += 1
                if row['classification'] == 'fixed_pose_recoverable':
                    counter['recoverable_boundary/' + following['next_boundary']] += 1
            if row['after_conflicts']:
                overlaps['cases'] += 1
                overlaps['introduced_from_conflict_free'] += row['before_conflicts'] == 0
                overlaps['cleared_before_restore'] += following['conflicts_cleared_before_restore'] is not None
                overlaps['next_boundary/' + following['next_boundary']] += 1
            if row['improves_area'] and following['next_boundary'] != 'valid_commit':
                missed.append(dict(name=row['name'], seed=row['seed'], stream=row['stream'], episode=row['episode'],
                    start=row['start'], next_boundary=following['next_boundary'], recovered_score=row['recovered_score'],
                    actual_repair_actions=following['counts'].get('repair_actions', 0)))
                if following['next_boundary'] == 'restore_best' and row['start'] != 'repair_ready' and 'area_then_restore' not in selected:
                    selected['area_then_restore'] = row
            if row['recovered_by'] not in (None, 'original') and 'order_recovery' not in selected:
                selected['order_recovery'] = row
            if row['after_conflicts'] and 'overlap' not in selected:
                selected['overlap'] = row
        for ref in source_refs.values():
            checked(ref, source)
        # Recover original recorded commit witnesses for an independent audit of
        # actual same-pose completions, including cases our cascade missed.
        by_chunk = defaultdict(list)
        for row in actual_recovery:
            by_chunk[row['source_chunk']['path']].append(row)
        for requested in by_chunk.values():
            chunk = load(checked(requested[0]['source_chunk'], source))
            for row in requested:
                sample = next(s for s in chunk['samples'] if s['stream'] == row['stream'] and s['policy_action'] == row['policy_action'])
                require(sample['validated_witness'] is not None, 'actual successful commit witness exists')
                checker.check(sample['observation']['blueprint'], sample['validated_witness'])
        summary = dict(schema='blueprint-post-pose-summary-v1', status='passed', protocol_sha256=sha(output / 'protocol.json'),
            report=reference(output / 'report.json', output), source_sha256=sha(Path(__file__)),
            classifications=report['classifications'], recoveries=report['recoveries'], by_seed=dict(by_seed), by_start=dict(by_start),
            initial_routing_failures=dict(failures), overlap_continuations=dict(overlaps),
            unresolved_details=dict(unresolved),
            actual_same_pose_commits=actual_recovery,
            actual_recoveries_missed_by_probe=sum(r['classification'] == 'bounded_routing_unresolved' for r in actual_recovery),
            area_opportunities_without_direct_commit=missed,
            distinct_opportunity_episodes=len({(r['seed'], r['stream'], r['episode']) for r in rows if r['improves_area']}),
            checked_native_restorations=len(rows), unique_validated_witnesses=len(checker.unique), witness_references=checker.references,
            examples={k: dict(name=v['name'], case=v['case']) for k, v in selected.items()})
    write(output / 'summary.json', summary)
    plt.rcParams.update({'font.size': 10, 'axes.spines.top': False, 'axes.spines.right': False})
    seeds = list(by_seed)
    fig, axes = plt.subplots(1, 2, figsize=(13, 5), constrained_layout=True)
    classes = [('overlap_requires_pose', 'Overlap: further movement needed'),
               ('endpoint_requires_pose', 'Endpoint inside a facility'),
               ('fixed_pose_recoverable', 'Validated fixed-pose recovery'),
               ('bounded_routing_unresolved', 'Unresolved by bounded routing')]
    bottoms = [0] * len(seeds)
    for name, label in classes:
        values = [by_seed[seed][name] for seed in seeds]
        axes[0].bar(seeds, values, bottom=bottoms, label=label)
        bottoms = [a + b for a, b in zip(bottoms, values)]
    axes[0].set(title='Every accepted training pose', xlabel='Seed', ylabel='Post-pose states')
    axes[0].legend(fontsize=8)
    groups = [('all recoverable', [r for r in rows if r['classification'] == 'fixed_pose_recoverable']),
              ('score-improving', [r for r in rows if r['improves_score']]),
              ('area-improving', [r for r in rows if r['improves_area']])]
    bottoms = [0] * len(groups)
    for boundary in ('next_pose', 'restore_best', 'valid_commit', 'terminal'):
        values = [sum(r['continuation']['next_boundary'] == boundary for r in group) for _, group in groups]
        axes[1].bar([name for name, _ in groups], values, bottom=bottoms, label=boundary.replace('_', ' '))
        bottoms = [a + b for a, b in zip(bottoms, values)]
    axes[1].set(title='Actual next boundary after recoverable poses', ylabel='Post-pose states')
    axes[1].legend(fontsize=8)
    fig.suptitle('Policy-created drafts | correlated pose states, not independent episodes')
    fig.savefig(output / 'recovery.png', dpi=170)
    plt.close(fig)
    for label, row in selected.items():
        case = load(output / row['case']['path'])
        winner = next((b for b in case['recovery']['branches'] if b['status'] == 'validated'), None)
        columns = 3 if winner else 2
        fig, axes = plt.subplots(1, columns, figsize=(5 * columns, 5), constrained_layout=True)
        extent = bounds(case['before'], case['after'], *([winner['witness']] if winner else []))
        draw(axes[0], case['before'], extent, draft=True)
        draw(axes[1], case['after'], extent, draft=True)
        axes[0].set_title('Before policy pose | private draft')
        axes[1].set_title(f'After pose | {len(case["recovery"]["geometry"]["pending"])} pending connections')
        if winner:
            draw(axes[2], winner['witness'], extent)
            axes[2].set_title(f'Validated recovery | area {winner["score"][0]}\nBest before pose: {row["start_score"][0]}')
        for ax in axes:
            labels = defaultdict(list)
            for text in ax.texts:
                labels[text.get_position()].append(text)
            for (x, y), texts in labels.items():
                for index, text in enumerate(texts):
                    text.set_position((x, y + (index - (len(texts) - 1) / 2) * .9))
        fig.suptitle(f'{row["name"]} | next policy boundary: {row["continuation"]["next_boundary"]}')
        fig.savefig(output / f'{label}.png', dpi=170)
        plt.close(fig)
    print(__import__('json').dumps({k: v for k, v in summary.items() if k not in ('actual_same_pose_commits', 'area_opportunities_without_direct_commit')}))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--run', type=Path, required=True)
    args = parser.parse_args()
    summarize(args.run.resolve())


if __name__ == '__main__':
    main()
