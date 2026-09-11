"""Inspect recorded credit and fixed-state pose probabilities without retraining."""
from pathlib import Path
from collections import defaultdict, Counter
import argparse, json
import torch
from blueprint_ppo_store import read, load, write, checked, sha, restore_learners
from blueprint_ppo_batch import pack, distribution
from blueprint_ppo import check_sources
from blueprint_ppo_update import credit, gae, digest
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--run', type=Path, required=True)
root = parser.parse_args().run.resolve()
p = read(root / 'protocol.json')
cfg = p['config']
allrows = []
batches = []
state = read(root / 'state.json')
assert state['status'] == 'complete' and state['protocol_sha256'] == sha(root / 'protocol.json')
assert cfg['training']['updates'] == 64, 'This diagnostic reports the registered 64-update screen.'
assert read(root / 'audit.json')['status'] == 'passed'
check_sources(p)
torch.set_num_threads(1)
for seed in cfg['seeds']:
    for ordinal in range(1, 65):
        record = read(checked(state['jobs'][str(seed)]['records'][ordinal - 1], root))
        chunk = load(checked(record['chunk'], root))
        raw, normalized, targets = credit(chunk['samples'], chunk['frontiers'], cfg['ppo'])
        assert digest(normalized) == chunk['ppo']['advantages_sha256']
        assert digest(targets) == chunk['ppo']['targets_sha256']
        reward_only = torch.zeros_like(raw)
        for f in chunk['frontiers']:
            ii = [i for i, s in enumerate(chunk['samples']) if s['stream'] == f['stream']]
            rows = [chunk['samples'][i] for i in ii]
            rr, _ = gae([s['reward'] for s in rows], [0.0] * len(rows), [s['terminal'] for s in rows], 0.0, cfg['ppo']['gamma'], cfg['ppo']['gae_lambda'])
            reward_only[ii] = rr
        rows = []
        for i, s in enumerate(chunk['samples']):
            rows.append(dict(seed=seed, update=ordinal, stream=s['stream'], episode=s['episode'], start=s['start'], action=s['policy_action'], kind=s['input']['actions'][s['index']]['kind'], raw=float(raw[i]), norm=float(normalized[i]), direct_reward_credit=float(reward_only[i]), value_component=float(raw[i] - reward_only[i]), value=s['old_value'], reward=s['reward'], terminal=s['terminal'], pending_before=sum((e['status'] == 'pending' for e in s['observation']['edges'])), pending_after=s['after_pending'], status=s['outcome']['status']))
        allrows += rows
        batches.append(dict(seed=seed, update=ordinal, reward=sum((s['reward'] for s in rows)), raw_std=raw.std(unbiased=False).item(), kinds={kind: dict(n=sum((s['kind'] == kind for s in rows)), raw_mean=sum((s['raw'] for s in rows if s['kind'] == kind)) / max(1, sum((s['kind'] == kind for s in rows))), norm_mean=sum((s['norm'] for s in rows if s['kind'] == kind)) / max(1, sum((s['kind'] == kind for s in rows)))) for kind in ['pose', 'repair', 'repair_all', 'commit', 'restore_best']}))
    print('loaded seed', seed, flush=True)
episodes = defaultdict(list)
for s in allrows:
    episodes[s['seed'], s['stream'], s['episode']].append(s)
for rows in episodes.values():
    remaining = 0.0
    next_reward_action = None
    for s in reversed(rows):
        remaining += s['reward']
        if s['reward'] > 0:
            next_reward_action = s['action']
        s['realized_future_reward'] = remaining
        s['delay_to_reward'] = None if next_reward_action is None else next_reward_action - s['action']

def stats(rows):
    n = len(rows)
    return dict(n=n, raw_mean=sum((s['raw'] for s in rows)) / max(n, 1), norm_mean=sum((s['norm'] for s in rows)) / max(n, 1), negative_fraction=sum((s['norm'] < 0 for s in rows)) / max(n, 1), mean_value_component=sum((s['value_component'] for s in rows)) / max(n, 1), direct_reward_credit_mean=sum((s['direct_reward_credit'] for s in rows)) / max(n, 1), positive_actual_future=sum((s['realized_future_reward'] > 0 for s in rows)), negative_adv_despite_positive_future=sum((s['realized_future_reward'] > 0 and s['norm'] < 0 for s in rows)))
summary = []
for seed in cfg['seeds']:
    for lo, hi in [(1, 1), (2, 2), (1, 8), (1, 16), (17, 64), (1, 64)]:
        rows = [s for s in allrows if s['seed'] == seed and lo <= s['update'] <= hi]
        summary.append(dict(seed=seed, updates=[lo, hi], kinds={k: stats([s for s in rows if s['kind'] == k]) for k in ['pose', 'repair', 'repair_all', 'commit', 'restore_best']}))
result = dict(schema='blueprint-credit-diagnostic-v1', protocol_sha256=sha(root / 'protocol.json'), source_sha256=sha(Path(__file__)), summary=summary, batches=batches, zero_reward_batches=sum((b['reward'] == 0 for b in batches)), positive_reward_batches=sum((b['reward'] > 0 for b in batches)), positive_future_poses=[s for s in allrows if s['kind'] == 'pose' and s['realized_future_reward'] > 0], gae_reward_weights={str(n): cfg['ppo']['gae_lambda'] ** n for n in [8, 16, 32, 64]})
probes = []
for seed in cfg['seeds']:
    chunk = load(root / f'seed-{seed}/chunks/0001.pt.gz')
    frozen = chunk['samples'][0]['input']
    for ordinal in [0, 16, 64]:
        cp = load(root / f'seed-{seed}/checkpoints/{ordinal:04d}.pt.gz')
        model, *_ = restore_learners(cp, cfg['ppo'], state['protocol_sha256'])
        probabilities = []
        for elapsed in [0.0, 0.5, 1.0]:
            x = dict(frozen, global_features=frozen['global_features'].clone())
            x['global_features'][5] = elapsed
            batch = pack([x])
            with torch.no_grad():
                logits, _ = model.forward_packed(batch)
                logp, _ = distribution(logits, batch)
            probabilities.append(sum((float(logp[i].exp()) for i, a in enumerate(x['actions']) if a['kind'] == 'pose')))
        probes.append(dict(seed=seed, update=ordinal, elapsed_fractions=[0.0, 0.5, 1.0], pose_probabilities=probabilities))
result['fixed_original_state_probes'] = probes
write(root / 'credit-diagnostic.json', result)
for group in summary:
    if group['updates'] in [[1, 1], [2, 2], [1, 8], [1, 64]]:
        print(json.dumps(group), flush=True)
print('reward batches', result['zero_reward_batches'], result['positive_reward_batches'])
print('poses preceding real rewards', len(result['positive_future_poses']))
