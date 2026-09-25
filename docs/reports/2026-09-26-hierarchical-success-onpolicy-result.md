# Successful-action imitation across fresh on-policy generations

Status: closed. The frozen-collection screen's promising target does not pass
the predeclared on-policy promotion gate. It is not a recovered training setup.

## Question and method

Does giving the selected action 80% of the policy target on successful roots
preserve and improve layout completion when each learner collects its own next
generation? Compare `current` and `success-on` from the same generation-87
lower and outer checkpoints on three disjoint streams, each with three new
32-case training panels. Generation-zero search collection is shared by the
arms; later collections are produced by each arm's preceding model. The lower
model stays frozen. Each arm keeps the same 100-epoch, 1600-update outer
training configuration, search seed 20260922, 128 simulations per move,
six-move horizon, and 64-case diagnostic panel. That panel is disjoint from
the 48 frozen-screen training cases and all nine training panels. The
diagnostic has 26/64 completions at the common starting checkpoint.

The transformed target leaves failed roots and value supervision unchanged.
Its per-generation [manifests](2026-09-26-hierarchical-success-onpolicy-transforms/)
record the exact source and transformed report hashes, successful and failed
root counts, and target total-variation changes. All 18 planned post-training
diagnostic evaluations finished and W&B acknowledged all 18 records.

| Stream, generation | Current collection /32 | Success-on collection /32 | Current diagnostic /64 (lost start) | Success-on diagnostic /64 (lost start) |
| --- | ---: | ---: | ---: | ---: |
| s1, 1 | 12 | 12 | 22 (15) | 20 (17) |
| s1, 2 | 14 | 12 | 24 (16) | 19 (18) |
| s1, 3 | 12 | 17 | 20 (19) | 20 (21) |
| s2, 1 | 10 | 10 | 28 (16) | 20 (18) |
| s2, 2 | 10 | 11 | 26 (18) | 21 (16) |
| s2, 3 | 11 | 18 | 24 (19) | 18 (17) |
| s3, 1 | 16 | 16 | 27 (16) | 27 (12) |
| s3, 2 | 11 | 14 | 29 (13) | 23 (16) |
| s3, 3 | 13 | 14 | 20 (15) | 28 (12) |

The diagnostic values include 64 paired cases, not the 32 collection cases.
The [progress record](2026-09-26-hierarchical-success-onpolicy-progress.json)
retains all diagnostic success masks, starting-success losses and gains,
checkpoint and report hashes, and stage runtimes. The compact
[collection record](2026-09-26-hierarchical-success-onpolicy-collection-summary.json)
retains the 32-case masks and verifies each source report hash.

## Predeclared gate and verdict

The final completion differences (`success-on` minus `current`) are 0, -6,
and +8 of 64 across s1–s3. The mean is +0.67, below the required +4, and
only one stream improves, below the required two. Fewer starting successes
lost are -2, +2, and +3, a mean of +1 rather than the required +3, although
two streams have positive retention. Across generations two and three, the
six paired completion differences average -2.33, below the required zero.
The [primary summary](2026-09-26-hierarchical-success-onpolicy-primary-summary.json)
evaluates the exact five predeclared primary conditions; four fail. All 18
training evaluations were completed before the verdict.

The protocol's additional three-seed stress stage is a conjunction with
these primary conditions. No stress outcome can repair the failed two-stream
condition, so the stress stage was skipped after the complete primary run.
This is an early rejection of this target recipe, not a relaxation of the
promotion threshold. The original one-seed diagnostic remains sensitive to
case-ID noise; no new claim of absolute multi-seed strength is made.

The s3 rebound shows that the target is not uniformly harmful. It is also
not stable across streams or generations: s3 falls from 27 to 23 after two
generations, then rises to 28, while s2 ends six completions below current.
Source collection success does not reliably predict diagnostic completion.
For example, at s2 generation three, `success-on` collects 18/32 versus
11/32 for `current`, but evaluates at 18/64 versus 24/64 afterward. This
narrows the next question to search/target transfer across model updates;
it does not identify a unique causal mechanism. The separate rank-coupling
probe found candidate churn from index-assigned Gumbel noise, which warrants
a controlled search intervention before another long training comparison.

## Reproduction and retained evidence

- Training/search source commit `d4f65f23`; the remote run used its unchanged
  release binary. Later local action-keyed Gumbel edits were not in that
  binary. The [protocol](2026-09-26-hierarchical-success-onpolicy-protocol.json)
  retains the binary, runner, transform, helper, tracking, configuration,
  initial-checkpoint, panel, and source hashes and the numeric gate.
- Remote workspace: `~/Srv/aic-target-causal-20260925`; output:
  `artifacts/recovery-success-onpolicy`. From that workspace and the
  documented LibTorch environment, run
  `python3 tools/factory-edit-pilot/hierarchical_success_onpolicy.py prepare --output artifacts/recovery-success-onpolicy --experiment-commit d4f65f23`,
  then `run` with the same output. After all 18 records, run
  `python3 tools/factory-edit-pilot/hierarchical_success_onpolicy_primary.py --output artifacts/recovery-success-onpolicy`.
- The primary closeout script is separate from the original runner so its
  source hash in the protocol and the executed training code remain intact.
  Collection, training, diagnostic, and baseline-evaluation stages total
  5,724.8 measured seconds (about 95 minutes); these sums exclude small
  orchestration and tracking overhead.
- [W&B observer status](2026-09-26-hierarchical-success-onpolicy-wandb-observer.json)
  records remote acknowledgement through step 18; the
  [run](https://wandb.ai/acrylicshrimp-personal/endfield-aic-layout-gen/runs/6bb46ed6a258fa6e09bdcdb9)
  contains the live tracking history.
- The six final outer checkpoints are temporarily retained for the explicit
  frozen action-keyed versus rank Gumbel comparison. Their paths and file
  hashes are in the [retention manifest](2026-09-26-hierarchical-success-onpolicy-retained-models.json).
  Remove them when that comparison closes.
- After the closeout package and source identities were verified, the
  [cleanup record](2026-09-26-hierarchical-success-onpolicy-cleanup.json)
  removed 7,285,476,879 bytes of raw evaluations, collections, intermediate
  checkpoints, panels, logs, and tracking state. Filesystem free space rose
  from 169,154,551,808 to 176,441,581,568 bytes. The six explicitly needed
  final models remain outside the removed experiment root.
