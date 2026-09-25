# Hierarchical search-policy target quality audit

Status: all six predeclared searches completed; both diagnostic signals passed.
No model was trained or selected. The
[protocol](../designs/2026-09-25-hierarchical-policy-target-quality-audit.md)
was fixed before search results; deterministic duplicate filtering was added
after input preparation and before the first search. The
[compact audit](2026-09-25-hierarchical-target-quality-summary.json) contains
all 48 paired first-root measurements.

## Question and method

Does the current 128-simulation Gumbel search give the outer learner a target
grounded in enough tested actions? Three fresh, disjoint 16-case train-source
panels used the same generation-87 lower and outer model weights. Each panel
was searched with 128 and 512 simulations per committed macro action. All
other search controls, including the 16 root candidates, eight guaranteed
anchor candidates, 50/50 root prior mix, Gumbel seed, horizon and 24-move
limit, were held fixed. The 512 outer config changed only its embedded search
budget; checkpoint bytes were identical. First-root state hashes and complete
action-witness sets matched exactly for all 48 pairs.

The current 128-simulation sequential halving starts with 16 candidates and
four elimination rounds. With all 16 present, its first round runs two
simulations per candidate; later rounds add four, eight and 16 simulations to
surviving candidates. At 512 simulations, the corresponding first round has
eight per candidate. The candidate count remains 16 at both budgets. The
completed-Q target nevertheless assigns probabilities to *all* root actions,
including actions never simulated. This is the implementation's intended
Gumbel target construction, not a report parsing artifact.

## Results

| Panel | Mean root actions | Visited actions | 128 target mass on unvisited actions | Effective target actions, 128 / 512 | Target top-1 changed | Completed, 128 / 512 |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| S1 | 2,700 | 16 | 99.47% | 2,684 / 2,688 | 11/16 | 6 / 7 |
| S2 | 2,738 | 16 | 99.48% | 2,721 / 2,727 | 11/16 | 7 / 7 |
| S3 | 2,764 | 16 | 99.46% | 2,746 / 2,748 | 8/16 | 6 / 6 |

All 48 first roots were covered. The aggregate unvisited target mass at 128
simulations was 99.4717%, close to the 99.4123% expected from uniformly
distributing mass over roughly 2,734 actions when only 16 are visited. The
target's effective action count, `exp(entropy)`, stayed within roughly one
percent of the full action count at both budgets. The mean target top-action
probability at 128 simulations was only about 0.183%, although that top action
was visited in every case. The 128 target differed little from its own 50/50
policy/anchor prior: mean Jensen-Shannon divergence was 0.00237 bits.

The target top action changed in 30/48 first roots (62.5%) when the budget
increased fourfold. The mean Jensen-Shannon divergence between the two **full
target distributions** was only 0.00060 bits. These facts must be read
together: a nearly flat distribution is easy to reorder at the top without a
large overall probability shift. Search completion changed from 19/48 to
20/48, with four gains and three losses. On S2 there were no completed-case
changes; S1 gained three and lost two, and S3 gained one and lost one. The
mean unvisited mass and top-rank disagreement were similar in cases the
128-search did and did not complete. The audit's predeclared labels
`weakly grounded by sampled outcomes` and `budget-sensitive` both apply.

This **does not prove** that the target is wrong for an unvisited action or
that it caused the earlier training collapse. It shows that the present first
root target contains little search-derived concentration, and that spending
four times as many simulations on the same 16 candidates barely changes
overall completion or target spread. The 16-candidate cap is therefore at
least as relevant as the total simulation count. A forced-first-action
counterfactual or independent stronger solver would be needed to label
individual target preferences correct or incorrect. A controlled learner
comparison would then be needed to link target changes to training stability.

The benchmark comparison is context, not a direct budget rule: the original
[AlphaZero study](https://jeromehubert.ovh/Go/Text/AlphaGo/alphazero_preprint-7Dec2018.pdf)
used 800 simulations per move, while the
[Gumbel planning paper](https://davidstarsilver.wordpress.com/wp-content/uploads/2025/04/gumbel-alphazero.pdf)
studied 2, 4, 16 and 200 simulations in 9×9 Go and 400 in its large Go/chess
comparison. Action spaces, model quality and simulation cost differ from
this layout search.

## Evidence, runtime, and decision

The [protocol](2026-09-25-hierarchical-target-quality-protocol.json) records
panel, configuration, binary and checkpoint hashes. The
[progress manifest](2026-09-25-hierarchical-target-quality-progress.json)
records all six raw report hashes and wall times. The exact Rust search source
is commit `31555f5`; the release binary SHA-256 was
`e594b5425074a03731524d8c91991126232d840ef44a86e8b0630e1f16800fdf`.
The pre-search audited code is commit `9b0d397`; the final descriptive
summarizer is commit `cca2971`. The model SHA-256 values and source configs are
in the protocol. The six integrated search runs took 620.7 seconds in total,
including report publication and validation; their raw reports occupied
351,802,052 bytes. This is a bounded diagnostic, not training, so no W&B run
was needed.

After the compact package was verified readable, the generated input and raw
report tree and this checkout's build cache were removed. The
[cleanup manifest](2026-09-25-hierarchical-target-quality-cleanup.json)
records the exact paths and byte counts; server free space rose by
1,235,431,424 bytes.

To reconstruct in a fresh `~/Srv/aic-policy-stability-20260923` artifact root,
with the documented LibTorch environment set:

```bash
cargo build --release -p aic-cli --features learning
bash tools/factory-edit-pilot/prepare_hierarchical_target_quality.sh target/release/aic-cli
~/Srv/aic-rust-learning-20260915/.venv/bin/python \
  tools/factory-edit-pilot/hierarchical_target_quality_audit.py prepare \
  --output artifacts/target-quality-audit --experiment-commit 9b0d397
~/Srv/aic-rust-learning-20260915/.venv/bin/python \
  tools/factory-edit-pilot/hierarchical_target_quality_audit.py run \
  --output artifacts/target-quality-audit
~/Srv/aic-rust-learning-20260915/.venv/bin/python \
  tools/factory-edit-pilot/hierarchical_target_quality_audit.py summarize \
  --output artifacts/target-quality-audit
```

The first build attempt omitted the `learning` feature and produced no search
result. Preparation then detected duplicated problem identities, so a
deterministic first-distinct selector was committed before the first search.
One checksum literal in the protocol guard was corrected before any search.
After search, descriptive entropy and prior-distance summaries were added
without changing inputs, the six runs or the predeclared signals.

**Decision:** Do not increase training search to 512 simulations as a presumed
fix. The next causal test should compare candidate coverage and target
construction on the same frozen roots under a matched search-cost budget,
while measuring whether the target assigns more mass to independently
successful first actions. Keep training and historical held-out panels separate,
and require a subsequent multi-generation retention check before adopting any
learner change.
