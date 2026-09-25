# Policy-target learner confirmation protocol

Status: frozen before the first learner comparison. This specifies the learner
part of the [causal study](2026-09-25-hierarchical-target-causal-study.md).
The contrast label is filled by the study's independently replicated
continuation gate;
no downstream diagnostic result may change it.

## Common controls

Use the generation-87 lower and outer checkpoints recorded in the causal-study
protocol. Freeze the lower model. Keep the 16-candidate, 128-simulation search,
eight guaranteed anchor candidates, 24-move limit, catalog, optimizer, batch
order seed, and diagnostic search seed equal across arms. Train for 100 epochs
per generation with 256 sampled outer states per epoch, batches of 16, outer
learning rate 0.0003, gradient clip 1.0, failed-root policy targets enabled,
and no replay. These are 1,600 optimizer updates per arm per generation. The
100-epoch duration is the existing update-budget comparison point; that study
did not establish that it is an optimum. Record the exact configuration hash.

Use only the `policy_target` transform from the causal-study contrast analysis.
The source search report and synthetic collection must differ only in those
fields. Record both SHA-256 values and the transform manifest. Preserve the
native report lineage, case IDs, exact action order and witnesses, searched Q,
visits, and terminal outcomes. A synthetic collection is research input, not
an MCTS result.

Create two new 64-case training-partition diagnostic panels and three streams
of three new 32-case training panels. Generate 256 candidates per training
panel using seeds `20261800 + 100*stream + generation` (streams 1, 2, 3;
generations 0, 1, 2), and 512 candidates per diagnostic using seeds 20262210
and 20262220. In that order, take the first unseen canonical problem identities
from each candidate panel. Exclude duplicate canonical problem
identities within and across these panels and the 48 frozen target-study cases.
Prepare all panels before examining learner outcomes. The screen and retention
diagnostics are disjoint. Neither historical validation nor final held-out
cases enter selection. Generation panels are common to both arms, but each arm
collects its own on-policy search report after the first generation.

## Frozen-collection screen

For each of the three 16-case target-study streams, train independent `current`,
`contrast-first`, and `contrast-all` arms from identical generation-87 weights.
All three consume the same frozen baseline 16/128 search report. Evaluate their
output models on the common 64-case screen diagnostic panel. Compare full
case-level success masks with the common starting model. Select the contrast
scope only if it loses at least three fewer starting-model successes on average
than `current`, improves retention in at least two of three streams, and gives
up at most two completions on average. If both scopes pass, select the one with
fewer mean losses; then more mean completions; then `first`. If neither passes,
carry `first` forward as a stress contrast only, with no efficacy claim.
Report immediate gains and losses, easy (at most four connections) and hard
strata, and loss, along with the paired case masks. A case-count gain alone is
insufficient evidence of retention.

## Fresh on-policy confirmation

Start three independent paired streams again at generation-87. In each stream,
run three generations from the common 32-case panels, comparing `current` and
the fixed contrast and scope. For generation zero both arms share a frozen
source search report; only target fields differ. For generations one and two,
search separately with each arm's preceding outer checkpoint, then transform
only the contrast-arm report. Train both arms for exactly the common update
budget. Evaluate the same disjoint 64-case retention diagnostic at start and
after every generation. Store per-case success masks and conditional layout
quality, source and output hashes, elapsed time, and search terminal classes.

At generation three, require at least three fewer lost starting successes on
average, positive retention in two of three streams, and no more than two
fewer total completions on average before calling the contrast a retention
candidate. Inspect the generation-by-generation trajectories and discordant
cases even if this gate passes. This remains a lower-frozen diagnosis. An
independent lower-training restart and an untouched final held-out test are
required before changing production curriculum settings.

## Execution and closeout

Check server disk and active GPU processes before the integrated run. Validate
one training/report rewrite and one evaluation end to end before the long
matrix. Attach a nonblocking observer to the configured W&B project and verify
remote acknowledgement of its readiness record before leaving training
unattended. The runner must preserve a resumable progress prefix, reject input
hash changes, and keep publication atomic. Close out the frozen-collection
screen before launching the on-policy confirmation. Retain a compact English
report, configuration and provenance hashes, case-level masks, W&B status,
reproduction commands, and a byte-precise cleanup manifest under `docs/`.
