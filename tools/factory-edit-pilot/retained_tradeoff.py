"""Summarize retained case-level exposure and failure evidence without resampling."""

import argparse
from collections import Counter
import json
from pathlib import Path

from evidence import write
from mastery_policy_selection import digest, read


COMPARISON = '0d7b94a049fac6871b1a7e6463f0a4408619827776ba2464630c7db501a5ee50'
PRECISION = 'cc51b8ac5f8accb855c5546331320aacd271afb32b55d2fbb9a0c132ebc17ce1'


def require(condition, path, message):
    if not condition:
        raise ValueError(f'retained_tradeoff.input: {path}: {message}')


def verified(path, expected):
    require(digest(path) == expected, path, 'source hash differs')
    return read(path)


def run(source, precision, output):
    require(not output.exists(), output, 'choose a fresh output file')
    comparison = verified(source/'comparison.json', COMPARISON)
    frozen = verified(precision/'precision.json', PRECISION)
    require(comparison['status'] == frozen['status'] == 'complete'
            and comparison['decision']['candidate_for_larger_test'] is False
            and frozen['original_criterion_passed'] is False, source, 'expected original failed criterion')
    cases = [c['case'] for c in comparison['anchor_roots']]
    require(len(cases) == 16 and len(set(cases)) == 16 and len(comparison['branches']) == 6,
            source, 'incomplete declared matrix')
    result = {'schema_version': 1, 'purpose': 'retrospective_case_tradeoff',
              'comparison_sha256': COMPARISON, 'precision_sha256': PRECISION,
              'original_criterion_passed': False, 'new_commands': 0, 'new_updates': 0, 'branches': []}
    for branch in comparison['branches']:
        directory = source/f"seed{branch['seed']}-lr{branch['multiplier']:g}"
        path = directory/'collected.jsonl'
        require(digest(path) == branch['collected_sha256'], path, 'collection hash differs')
        episodes = [json.loads(line) for line in path.read_text().splitlines()]
        require(len(episodes) == 2048, path, 'expected 2048 retained episodes')
        totals = {case: Counter() for case in cases}
        first = episodes[0]['summary']['episode']
        for index, row in enumerate(episodes):
            s, trace = row['summary'], row['trace']
            rewards = row['samples']['rewards']
            require(s['case'] == cases[index % len(cases)] and s['episode'] == first+index
                    and s['horizon'] == len(trace) == s['counts']['commands'] == len(rewards) == 16
                    and s['original_root'] == (not row['prefix'])
                    and s['setup_commands'] == len(row['prefix'])
                    and s['start_reason'] in ('current', 'rehearsal')
                    and abs(sum(rewards)-s['return']) < 1e-10
                    and all(abs(t['reward']-v) < 1e-10 for t, v in zip(trace, rewards)),
                    path, f'episode {index}: accounting or start identity differs')
            a = totals[s['case']]
            a.update(episodes=1, commands=16, setup_commands=len(row['prefix']),
                     root_episodes=int(s['original_root']), current_episodes=int(s['start_reason'] == 'current'),
                     positive_returns=int(s['return'] > 0),
                     root_positive_returns=int(s['original_root'] and s['return'] > 0),
                     unfinished_drafts=int(s['unfinished']))
            completed = [i+1 for i, t in enumerate(trace) if t['new_completion']]
            a['episodes_with_new_completion'] += bool(completed)
            if completed:
                a['first_completion_command_sum'] += completed[0]
            a['positive_returns_with_unfinished_draft'] += s['return'] > 0 and s['unfinished']
        assessments = []
        for item in branch['assessments']:
            gate = verified(directory/item['file'], item['sha256'])
            require(gate['successes'] == item['successes'], directory/item['file'], 'assessment summary differs')
            assessments.append((item['update'], gate))
        require([i for i, _ in assessments] == [8, 16, 24, 32], directory, 'assessment schedule differs')
        rows = []
        for case in cases:
            a = totals[case]
            require(a['episodes'] == 128 and a['current_episodes'] == 96, path, f'{case}: exposure differs')
            rows.append({'case': case, 'training': dict(a),
                         'short_assessments': [{'update': i, 'level': g['level'][case],
                                                'successes': g['successes'][case], 'episodes': 64}
                                               for i, g in assessments]})
        result['branches'].append({'seed': branch['seed'], 'multiplier': branch['multiplier'], 'cases': rows})
    result['precision_cases'] = []
    for cell in frozen['cells']:
        failed = [e for e in cell['episodes'] if not
                  (e['score'][0] < cell['initial_score'][0] and e['translated_facilities'])]
        result['precision_cases'].append({'case': cell['case'], 'condition': cell['condition'],
            'successes': cell['translated_successes'], 'episodes': len(cell['episodes']), 'failures': len(failed),
            'failures_without_new_completion': sum(e['counts'].get('new_completion', 0) == 0 for e in failed),
            'failures_without_placement_command': sum(e['counts'].get('SET_PLACEMENTS', 0) == 0 for e in failed)})
    require(digest(source/'comparison.json') == COMPARISON and digest(precision/'precision.json') == PRECISION,
            source, 'source changed during analysis')
    result['status'] = 'complete'
    output.parent.mkdir(parents=True, exist_ok=True)
    write(output, result)
    return result


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--source', type=Path, required=True)
    parser.add_argument('--precision', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    run(args.source.resolve(), args.precision.resolve(), args.output.resolve())
