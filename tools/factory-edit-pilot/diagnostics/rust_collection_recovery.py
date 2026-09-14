"""Bounded recovery/rejection checks over retained actual collection fixtures."""
import argparse
import copy
import json
from pathlib import Path
import shutil
import subprocess
import tempfile


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ('binary', 'config', 'kernel-input', 'prefix', 'resumed', 'output'):
        parser.add_argument('--'+name, type=Path, required=True)
    args = parser.parse_args()
    assert not args.output.exists()
    args.output.mkdir(parents=True)
    from safetensors.torch import load_file
    import torch
    before = load_file(args.prefix/'checkpoint/learner.safetensors')
    after = load_file(args.resumed/'chunk-0001/before.safetensors')
    assert before.keys() == after.keys()
    assert all(torch.equal(before[k], after[k]) for k in before)
    report = json.loads((args.resumed/'report.json').read_text())
    assert report['resumed'] and report['completed_chunks'] == 2
    assert report['records'][0]['true_terminals'] == report['streams']
    config = json.loads(args.config.read_text())
    for e in config['episodes']:
        for key in ('request', 'original'):
            e[key]['path'] = str((args.config.parent/e[key]['path']).resolve())
    results = []
    with tempfile.TemporaryDirectory(prefix='collection-rejection-') as tmp:
        tmp = Path(tmp)
        def reject(name, value, code, resume=None):
            path = tmp/(name+'.json')
            path.write_text(json.dumps(value))
            output = args.output/name
            command = [str(args.binary.resolve()), 'learning', 'verify-collection', '--config', str(path),
                       '--kernel-input', str(args.kernel_input.resolve()), '--output', str(output), '--chunks', '1']
            if resume:
                # Resume must bind to the original unmodified manifest bytes.
                command[command.index('--config')+1] = str(args.config.resolve())
                command.extend(['--resume', str(resume)])
            child = subprocess.run(command, capture_output=True, text=True, timeout=60)
            assert child.returncode != 0 and code in child.stderr, (name, child.stdout, child.stderr)
            assert not (output/'checkpoint').exists(), name
            results.append(dict(case=name, code=code, passed=True))
            return output
        reserved = copy.deepcopy(config)
        reserved['reserved_geometry'].append(reserved['episodes'][0]['geometry_sha256'])
        reject('reserved', reserved, 'aic-learning.collection-reservation')
        corrupt = copy.deepcopy(config)
        changed = tmp/'original.json'
        changed.write_bytes(Path(config['episodes'][0]['original']['path']).read_bytes()+b' ')
        corrupt['episodes'][0]['original']['path'] = str(changed)
        reject('input-hash', corrupt, 'aic-learning.collection-input-hash')
        checkpoint = tmp/'checkpoint'
        shutil.copytree(args.prefix/'checkpoint', checkpoint)
        tensor = checkpoint/'learner.safetensors'
        with tensor.open('ab') as file:
            file.write(b'corrupted')
        reject('checkpoint-hash', config, 'aic-learning.collection-checkpoint-hash', checkpoint)
        limited = copy.deepcopy(config)
        limited['max_observation_bytes'] = 1
        failed = reject('memory', limited, 'aic-learning.collection-memory')
        failure = json.loads((failed/'failure.json').read_text())
        n = len(config['episodes'])
        assert failure['completed'] is False and failure['samples'] == n
        assert len(failure['snapshots']) == len(failure['evidence']) == n
        rows = [json.loads(line) for line in (failed/'partial.jsonl').open()]
        samples = [r for r in rows if r['kind'] == 'sample']
        current = [r for r in rows if r['kind'] == 'current']
        assert len(samples) == len(current) == n
        assert all(r['attempts'] == 0 and r['outcome']['outcome']['charged']
                   and r['choice'] is not None and r['input']['global_features'] for r in samples)
        assert all(r['attempts'] == 1 for r in current)
        assert all(len(r['native']) == 1 for r in failure['evidence'])
        state = load_file(failed/'partial-learner.safetensors')
        assert state and any(k.startswith('rng.') for k in state)
    summary = dict(schema='aic-native-collection-recovery-check-v1', status='passed',
                   restored_tensor_fields_exact=len(before), resumed_workers=report['workers'],
                   resumed_streams=report['streams'], resumed_terminals=report['streams'],
                   failure_sample_inputs=n, failure_current_inputs=n, rejections=results)
    (args.output/'summary.json').write_text(json.dumps(summary, indent=2)+'\n')
    print(json.dumps(summary, indent=2))


if __name__ == '__main__':
    main()
