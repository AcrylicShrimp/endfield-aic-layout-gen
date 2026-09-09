"""Standalone behavior equivalence check on retained causal native workloads."""
import argparse
from pathlib import Path

import torch

from benchmark_ppo_optimization import fresh
from full_factory_input_contracts import CONTRACTS
from full_factory_screen import sha
from ppo_training_batch import prepare, distribution, critic_values, require
from random_factory_generation import read, write
from random_factory_pilot import configure_torch


def run(args):
    configure_torch()
    require(not args.output.exists(), f'fresh workload-check output: {args.output}')
    report = dict(schema='ppo-input-workload-check-v1', status='running', candidates=[], source_sha256=sha(__file__))
    for candidate in CONTRACTS:
        directory = args.artifacts / ('ppo-rich-native-check' if candidate == 'current-best-layout-history'
                                      else f'ppo-native-inputs/{candidate}')
        native = read(directory / 'report.json')
        require(native['status'] == 'complete' and native['native_checkpoint_continuation_exact'], str(directory))
        workload = torch.load(directory / 'workload.pt', map_location='cpu', weights_only=False)
        require(workload['identity']['input_contract'] == candidate, f'workload contract: {directory}')
        actor, critic, _, _ = fresh(workload)
        batch = prepare(workload['steps'], workload['config'], actor.width, 'cpu')
        with torch.no_grad():
            logp = distribution(actor, batch, workload['config'])[0]
            value = critic_values(critic, batch)
        torch.testing.assert_close(logp, batch['old_logp'], atol=5e-6, rtol=5e-5, msg=candidate)
        torch.testing.assert_close(value, batch['old_values'], atol=5e-6, rtol=5e-5, msg=candidate)
        record = dict(candidate=candidate, selections=len(workload['steps']), native_report_sha256=sha(directory / 'report.json'),
                      workload_sha256=sha(directory / 'workload.pt'),
                      logp_max_absolute=float((logp - batch['old_logp']).abs().max()),
                      value_max_absolute=float((value - batch['old_values']).abs().max()))
        report['candidates'].append(record)
        print(record, flush=True)
    report['status'] = 'complete'
    write(args.output, report)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--artifacts', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    run(parser.parse_args())
