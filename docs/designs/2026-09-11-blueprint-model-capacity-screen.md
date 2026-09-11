# Blueprint graph model capacity screen

Status: registered before implementation and model training.

The user requested testing whether greater representation capacity helps the new
blueprint-edit contract, and explicitly requested stopping the monitored training
before running this comparison on the server. The existing Mac trainer paused at
update 1396; its final tail audit may finish independently. The remote server's
older padded-curriculum run was already stopped. No previous learner is resumed
or imported into this comparison.

## Experiment contract

Inputs: the current 64-action blueprint duration configuration, frozen native
runtime/catalogs/training fixture, authentic 32-factory evaluation cohort, and the
explicit matrix in `tools/factory-edit-pilot/blueprint-capacity-screen.json`.

Use the same graph input, actor/critic ownership, two-layer message/update rule,
nonlinearity and pooling. Parameterize only hidden width and number of message
rounds. Compare widths 40 and 80 crossed with two and four rounds. Two-round,
width-40 behavior must match the existing model exactly under the same state and
seed. The optional round setting defaults to two; this extends the architecture
configuration without replacing the input contract or adding a checkpoint adapter.

Run seeds 17 and 29 from fresh weights, both empty Adam states and empty success
buffers for each architecture: eight independent runs. Each run receives 512
optimizer batches of eight 32-action chunks, or 131,072 training actions and
2,048 complete 64-action episodes. Across the matrix this is 1,048,576 training
actions. Keep original/overlap starts, learning rate .0003, GAE gamma 1/lambda .95,
batch advantage normalization, four PPO epochs, KL threshold, entropy coefficient,
replay coefficient .05 and all action candidates unchanged.

Evaluate the exact 32 historical factories at iteration zero, every four batches,
and the final boundary. Each case keeps the same authentic input, action seed,
64-action cap and two-second native guard. Export the actual iteration-zero W&B
point before any learning. Do not replace the fixed cohort with training cases or
silently change the budget for a slower model. Report action-cap coverage and any
elapsed-guard differences so computational cost cannot masquerade as capacity.

Execute all comparison runs on the same server CPU/runtime, one Torch thread per
run, with up to four concurrent workers on explicitly recorded CPU slots. Reuse
the established CPU learner; this slice does not introduce GPU execution or mix
hardware within the model comparison. Run seed-17 architectures in matrix order
and seed-29 architectures in reverse order to reduce a fixed launch/core ordering
bias. Concurrency and actual per-run CPU slots remain recorded evidence.

Bounds: a four-hour active wall-clock guard plus three-minute finalization grace
per run, eight-GiB per-run output ceiling and eight-GiB server free-space reserve.
The 512-batch update limit is the intended stop. Resource-limited or failed runs
are reported explicitly and cannot be ranked as completed runs. Check largest-
model native readiness and measured storage before launching the full matrix.

## Output and analysis

The matrix protocol binds source, input, model, child protocol and execution
identities. Matrix state reports queued/running/completed/failed arms and actual
process exit diagnostics. Every child retains the existing durable checkpoints,
chunks, evaluations, supervised online observer and final two-update audit.
Canonical W&B keys remain available; tracking metadata identifies matrix, width,
rounds, seed and parameter count for overlays.

Primary descriptive endpoint: mean validated area reduction on the final
32-factory evaluation, reported separately for both seeds and averaged afterward.
Also report each arm's paired change from its own untrained evaluation and mean
quality over the predeclared last 128 batches (evaluations 388 through 512).
Show the full learning curve, paired case wins/ties/losses against width-40/two-
round baseline, inference/collection/update costs, parameter counts, observed
success counts and action/elapsed guard coverage. Never rank by a post-hoc peak.

This is an initial capacity screen, not evidence of convergence or a search for
an optimal architecture. Training still uses one five-facility fixture in two
starts; evaluation uses distinct factories. Keep that limitation explicit rather
than changing training diversity simultaneously with model capacity. A null or
mixed 512-batch result can motivate a longer controlled comparison without being
presented as proof that representation capacity never matters.

## Verification and failure modes

Before launch, verify explicit/default two-round parity, packed/single forward
and gradient equivalence for the larger/deeper model, all message-round parameter
gradients, and exact learner/Adam/RNG restoration for non-default depth. Reject
nonpositive/noninteger architecture settings. A largest-model readiness run must
complete true 64-action episodes, all 32 evaluation cases and an exact replay audit.

Source or cohort drift, configuration differences outside the registered factors,
tracking failure, failed physical validation, elapsed/resource guards, failed
reference replay or missing child evidence produce structured diagnostic failure.
Preserve the stopped run's evidence. Generated matrix outputs stay ignored under
artifacts; commit source/configuration, concise findings and compact provenance.
All production-constructor and exact-research semantics remain unchanged.
