# Hierarchical outer-policy update-budget probe

Status: protocol fixed before this probe's training or diagnostic outcomes.
The [three-stream replay confirmation](../reports/2026-09-24-hierarchical-policy-stability-confirmation-result.md)
rejected both replay treatments and directed the next bounded test toward
update magnitude. This probe diagnoses one generation; it does not select a
production model or authorize integrated or ultra-long training.

## Question and controlled trajectory

Does continued fitting to one frozen MCTS collection cause the outer policy to
replace previously solved cases without enough net completion gain? Run three
new 32-case two-facility training streams from the same generation-87 lower and
outer checkpoints. Collect each stream once with the existing eager Gumbel
search. Train four independent prefixes from that same collection and starting
weights: 25, 50, 100 and 200 epochs, corresponding to 400, 800, 1,600 and
3,200 outer optimizer updates. Keep 256 outer root-state slots per epoch,
16-state batches, learning rate 0.0003, gradient clip 1.0, seed 20260925,
failed-root policy targets enabled, no replay, and lower training frozen. All
budgets use the identical epoch order. The 200-epoch arm is the current control.

| Stream | Training seed | Namespace |
| --- | ---: | --- |
| S1 | 20261210 | `hierarchical-update-budget-s1-v1` |
| S2 | 20261230 | `hierarchical-update-budget-s2-v1` |
| S3 | 20261250 | `hierarchical-update-budget-s3-v1` |

Generate each training panel with exactly two facilities, at most six terminals
and six connections, 10,000 turns and extension 32. Keep the established
sampling input, search configuration, catalogs, and eager model configuration.
The direct generation trainer reads each stream's one published collection for
all four budgets; repeated search collection is not part of the treatment.
Compare the output model checkpoint hashes and require the original lower
checkpoint bytes in all arms.

## Diagnostic separation

Prepare 512 train-source diagnostic candidates with seed 20261300, namespace
`hierarchical-update-budget-diagnostic-v1`, and the same bounds. Select the
first 64 distinct problem identities absent from all three probe training
panels and the two prior stability diagnostic panels. Reconstruct the prior
panels deterministically from their tracked seeds, source configurations and
selector, and verify their recorded SHA-256 before using them as exclusions.
Use the same selected panel for the common start and every trained checkpoint.
It never enters optimizer updates. Keep the historical 128-case validation and
any final held-out panel untouched.

## Measurements and interpretation

Report every stream and budget separately: total completed of 64, paired gains
and losses versus the common start and 200-epoch control, at-most-four-connection
completion, failure terminations, training loss, model movement, collection and
checkpoint hashes, exact updates, and integrated time including validation and
storage. Do not choose a budget from the highest single score. The primary
contrast is 100 versus 200 epochs; 25 and 50 characterize the shape.

The update-overshoot hypothesis earns a multi-generation confirmation only if
the 100-epoch model loses at least three fewer common-start cases than the
200-epoch model on the three-stream average, this retention advantage is
positive in at least two streams, and its three-stream mean completed count is
no more than two cases below the 200-epoch model. These descriptive thresholds
are fixed before results and are not confidence claims. If the criterion fails,
inspect the within-generation curve without promoting any budget; uncertain
search policy targets and cross-generation data shifts remain live mechanisms.
Even a pass requires a fresh-panel, multi-generation retention check before
lower training is restored or a production change is considered.

## Continuity and resources

Use the existing panel preparation, published search collection, direct
generation trainer, search harness, atomic optimizer recovery and W&B project.
Share each frozen collection across budgets, validate stable input hashes once,
and keep diagnostics out of the training loop. Run on `ashrimp-arch` under
`~/Srv` after checking disk and GPU usage. Attach a nonblocking resumable W&B
observer, verify a real readiness record is acknowledged before the matrix,
and verify final records before closeout. Stop on input identity mismatch,
changed lower weights, update-budget discrepancy, incomplete search output or
resource/tracking failure. Close out the experiment with compact case-level
evidence, source/configuration identity, interpretation and a cleanup manifest;
remove replayable panels, collections, optimizer checkpoints and build caches.
