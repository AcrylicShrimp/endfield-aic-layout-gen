# Horizon-12 on-policy seed stress

Status: all 24 planned records complete. The predeclared three-seed gate
passes. This confirms the combined search-and-trained-model advantage on the
training-partition diagnostic; a same-search model comparison and untouched
held-out evaluation remain required.

## Question and method

Does the final horizon-12 setup retain its completion advantage when the
root Gumbel seed changes? Evaluate the same three final horizon-6 `current`
models and three final horizon-12 models on the same disjoint 64-case
diagnostic under seeds 20260922, 20260923, and 20260924. Evaluate the
starting weights under both search horizons for context. Apart from the
root seed, keep each arm's trained weights, lower model, search horizon,
128 simulations, 16 candidates, eight anchors, 24-move limit, catalog,
case IDs, and panel fixed. Seeded shadow checkpoints change only the outer
config's embedded search contract; their weight hashes match the originals.

The [protocol](2026-09-26-hierarchical-h12-seed-stress-protocol.json) binds
the complete on-policy progress, release binary, models, panel, runner and
numeric gate. The [progress](2026-09-26-hierarchical-h12-seed-stress-progress.json)
retains every paired 64-case success mask, report hash and new-run time.
Eight seed-20260922 evaluations reuse the already verified on-policy records;
the other 16 are fresh CLI evaluations. The source runner was committed as
`19fc0fad` before the stress run.

## Results

| Root seed | Stream | H6 completed | H12 completed | H12 gains | H12 losses |
| --- | --- | ---: | ---: | ---: | ---: |
| 20260922 | s1 | 20 | 36 | 18 | 2 |
| 20260922 | s2 | 24 | 36 | 21 | 9 |
| 20260922 | s3 | 20 | 34 | 22 | 8 |
| 20260923 | s1 | 23 | 44 | 26 | 5 |
| 20260923 | s2 | 22 | 44 | 26 | 4 |
| 20260923 | s3 | 27 | 46 | 26 | 7 |
| 20260924 | s1 | 23 | 38 | 22 | 7 |
| 20260924 | s2 | 24 | 42 | 24 | 6 |
| 20260924 | s3 | 28 | 41 | 22 | 9 |

H12 gains and losses compare success masks within each stream and seed.
Every net difference is positive, but the two setups still solve different
subsets of problems. The seed-averaged final advantages are 14.0, 20.7 and
15.3 cases per 64; the nine-pair mean is 16.7. All three seed averages are
positive. The predeclared [summary](2026-09-26-hierarchical-h12-seed-stress-summary.json)
passes its mean-gain-at-least-four and positive-at-least-two-seeds gate.

The same starting weights complete 26/33, 21/37 and 25/39 cases with H6/H12
search under the three seeds. This shows a substantial immediate search
effect before this on-policy training. The current diagonal comparison cannot
show how much extra value came from the learned outer weights. The planned
two-by-two same-search cross-evaluation addresses that question next.

Across the 16 new 64-case evaluations, mean integrated wall time was 197.3
seconds for H6 and 204.4 seconds for H12, including search, report publication
and validation. This comparison uses eight runs per horizon on the same
machine and panel; it does not estimate training throughput or all workloads.
The first attempt at the new seed exited before search because the shell
omitted LibTorch's shared-library path. It published no evaluation record;
the path was set and the same job succeeded without changing inputs.

## Reproduction and closeout

With the retained source inputs and release binary under the recorded
`~/Srv/aic-target-causal-20260925` workspace and LibTorch environment:

```sh
export LIBTORCH="$HOME/Srv/aic-rust-learning-20260915/.venv/lib/python3.12/site-packages/torch"
export LIBTORCH_CXX11_ABI=1
export LD_LIBRARY_PATH="$LIBTORCH/lib"
python3 tools/factory-edit-pilot/hierarchical_horizon_stress.py prepare \
  --output artifacts/recovery-horizon-onpolicy-v2
python3 tools/factory-edit-pilot/hierarchical_horizon_stress.py run \
  --output artifacts/recovery-horizon-onpolicy-v2
python3 tools/factory-edit-pilot/hierarchical_horizon_stress.py summarize \
  --output artifacts/recovery-horizon-onpolicy-v2
```

The compact records and source configurations remain readable. The
[cleanup manifest](2026-09-26-hierarchical-h12-seed-stress-cleanup.json)
records removal of replayable raw evaluations and logs after this package
was verified; the small seed configs and shadow checkpoints stay only while
the cross-evaluation continuation needs them.
