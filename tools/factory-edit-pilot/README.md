# CPU fixed-factory edit pilot

An isolated first learning experiment, authorized by the user after the collaborative design
phase. See the [execution contract](../../docs/designs/2026-09-07.02-fixed-factory-pilot-execution.md)
and [pilot hypotheses](../../docs/designs/2026-09-07.01-factory-edit-learning-pilot.md).
The [first result](../../docs/reports/2026-09-07.00-first-cpu-factory-edit-pilot.md) records the
completed 65,536-command seed, outcome/reward curves, learned edit, and interpretation limits.

The model is a 100,898-parameter CPU CNN plus per-object MLP, with shared policy/value features
and a typed local-command scorer. PPO starts from fresh weights. There is no GNN, demonstration
pretraining, route-search helper, repair macro, or production fallback. All episodes start from
the same complete factory; only learner state persists. The first run has one training seed,
65,536 commands, and 128 commands per episode. It does not test generalization.

The deliberately narrow geometry domain is non-crossing belt paths with one source/demand per
commodity network. Four facilities produce a catalog battery from two catalog-external raw
materials. Rates, rotations, footprints, ports, and capacity come from runtime catalog files.
The executor supports the seven parent commands using the local domain in the execution contract;
the policy scores all mechanically proposed commands rather than an outcome-ranked shortlist.
Automatic final checking retains a complete best result, including the unchanged root if needed.

`case.json` is a compact human-authored input fixture retained because the identical starting
geometry and complete external obligations define this overfit task. It includes one improving
witness for environment checks only. `run.json` is the prospective run configuration. Neither
file is a generated solver/training result. No historical run artifact is a required input.

The current CPU default is four PyTorch intra-operation threads, selected by a local timing
comparison; the first published result used one. Inter-operation threads remain one. This
keeps PyTorch's standard convolutions, autograd, model size, PPO settings, and command budgets.
`updates.jsonl` now separates batch preparation, forward/loss, backward, and optimizer time.
See the [CPU timing report](../../docs/reports/2026-09-07.01-factory-edit-cpu-speed.md).

From the repository root:

```sh
uv sync --project tools/factory-edit-pilot --locked
tools/factory-edit-pilot/.venv/bin/python -m unittest discover -s tools/factory-edit-pilot -p 'test_*.py' -v
tools/factory-edit-pilot/.venv/bin/python tools/factory-edit-pilot/run.py check --case tools/factory-edit-pilot/case.json --config tools/factory-edit-pilot/run.json --output artifacts/factory-edit-pilot/preflight
tools/factory-edit-pilot/.venv/bin/python -u tools/factory-edit-pilot/run.py train --case tools/factory-edit-pilot/case.json --config tools/factory-edit-pilot/run.json --output artifacts/factory-edit-pilot/cpu-100898-seed17
```

An existing experiment directory is not overwritten. Use a new output directory for a new run.
The training command records catalog/source/configuration hashes and actual package versions,
raw episode/evaluation/update rows, complete checkpoint state, selected layouts/traces, and PNG/SVG
plots. Disposable preflight output and retained experiment evidence are ignored under `artifacts/`.
Checkpoints may include non-replayable interrupted rollout state; do not delete them on the
assumption that a seed can restore an interrupted stream.

To select CPU threads on another host, supply a trusted local checkpoint as an explicit input:

```sh
tools/factory-edit-pilot/.venv/bin/python -u tools/factory-edit-pilot/benchmark.py --case tools/factory-edit-pilot/case.json --config tools/factory-edit-pilot/run.json --checkpoint artifacts/factory-edit-pilot/cpu-100898-seed17/checkpoint-65536.pt --threads 1 2 4 8 --repeats 3 --output artifacts/factory-edit-pilot/thread-benchmark
```

This records one fixed sampled rollout, resets model/Adam/minibatch RNG before each update
measurement, excludes a warm-up round, alternates setting order, and reports median times.
It retains the exact input tensors under the ignored output directory. Optional `--channels-last`
and `--convolution unfold` / `view-unfold` are benchmark-only experiments; training uses standard
PyTorch Conv2d and autograd. Neither alternate convolution is selected by `run.json`.

Independent acceptance of an exported factory, without running the environment or policy:

```sh
tools/factory-edit-pilot/.venv/bin/python tools/factory-edit-pilot/acceptance.py --case tools/factory-edit-pilot/case.json --layout artifacts/factory-edit-pilot/cpu-100898-seed17/training-best.json
tools/factory-edit-pilot/.venv/bin/python tools/factory-edit-pilot/plots.py --case tools/factory-edit-pilot/case.json --output artifacts/factory-edit-pilot/cpu-100898-seed17
tools/factory-edit-pilot/.venv/bin/python tools/factory-edit-pilot/audit.py --case tools/factory-edit-pilot/case.json --output artifacts/factory-edit-pilot/cpu-100898-seed17 --source-commit 42ade74
```

`learning-curves.png` separates frozen-policy outcomes, training/evaluation episode rewards,
and the cumulative training champion. Under this reward, one episode's total equals its final
relative area reduction. `factory-comparison.png` shows initial, representative evaluation,
training-best, and scripted witness layouts with independent complete scores. A witness is
explicitly labeled as unlearned. A flat curve remains a reportable first result.
