# Production training dataflow validation

## Verdict

The hierarchical curriculum now uses its owned panel and search report as the
normal handoff from sampling through search into generation training. Durable
JSON publication remains an atomic recovery boundary, but a fresh process no
longer reads back the panel or collection it just wrote. The interrupted
110,988,756-byte generation-4 collection also resumes through the same owned
training core without an encoded-size exception.

This validates the accepted
[production dataflow design](../designs/2026-09-23.00-production-training-dataflow.md)
at implementation commit `8831bbcbfa81b6fcdee8b117615da8a57d746e2a`.
Search report v7, panel v1, generation recovery bindings, standalone commands
and the curriculum's committed artifact layout remain unchanged.

## Resulting flow

`hierarchical_search` now owns the shared deserializable panel and report
hierarchy. Buffered atomic publication hashes the exact serialized bytes while
writing the temporary file, and buffered recovery hashes while deserializing in
one pass. Curriculum search returns `Published<SearchReport>` to the caller;
the curriculum derives its compact metrics and moves the same report and panel
into the trainer. Example conversion consumes cases and root records instead
of cloning the complete collection behind a second private schema.

The path-based search and generation CLI commands are adapters over these owned
cores. Recovery still loads committed artifacts once, validates their exact
digests and lineage, and restores the existing model and optimizer checkpoints.
The small-input guard remains on configuration-style inputs; a hierarchical
collection is bounded by its structural and lineage contracts instead of a
fixed 64 MiB encoded size.

## Verification

Local verification on the exact implementation passed:

- `cargo fmt --all`
- `cargo check --workspace`
- `cargo test --workspace`
- `node docs/maintenance/check-tracked-files.mjs --worktree`
- `node docs/maintenance/check-tracked-files.mjs --index`

The isolated server source was transferred to
`~/Srv/aic-production-dataflow-20260923`. With the retained CUDA LibTorch
environment, `cargo test -p aic-learning --features libtorch` passed 66 tests
with four ignored, and `cargo check --workspace --features aic-cli/learning`
passed.

A fresh one-case CUDA curriculum exercised panel preparation, search,
publication, in-memory training handoff, held-out publication and final summary
commit. Training completed 1/1 case with seven root states, 3,111 outer action
samples and 25 lower route samples. The training report's panel and collection
hashes exactly matched the published files. A subsequent `--resume` recognized
the completed state without repeating the generation.

The recovery test used the canonical interrupted generation-4 artifacts. Its
collection was 110,988,756 bytes with SHA-256
`9eba92da250fc594ecce450ae67eef72ff59b1ec826191bedf6f9f1be2fcc932`.
The streaming loader accepted it, reconstructed 273 root states, 168,927 outer
action samples and 1,689 lower route samples, and completed all 200 CUDA epochs
in 78.287 seconds. The resulting lower and outer checkpoint hashes were
`22257a0db0536274ba72d3ab354dede56202fe822ec884475568d84549bf13c8`
and `fdc278b16129e58e1f9ea2bcd23de601ce6f81a35140071d310f32b8ae5e4af8`.

## Canonical run

The corrected canonical curriculum under
`~/Srv/aic-gumbel-curriculum-20260922` resumed from four committed generations.
Generation 4 reused its already committed collection, proving the intended
one-time recovery path. Its trainer accepted the same 110,988,756-byte artifact,
completed the 200-epoch update in 78.770 seconds and committed the fixed held-out
evaluation. Generation 5 then exercised the fresh path with a newly published
122,015,566-byte collection and completed its 200-epoch update in 78.816
seconds. Generation 6 started with a newly published in-memory panel, and the
curriculum process remained active.

The independent W&B observer is active for the corrected input binding. It
backfilled the pre-resume generations and W&B acknowledged history through step
5 at
[run 7718b4f7291e77b636a0702b](https://wandb.ai/acrylicshrimp-personal/endfield-aic-layout-gen/runs/7718b4f7291e77b636a0702b).
This is separate from the stopped regression run because the corrected search
configuration and binding have a different durable identity.

This is a dataflow validation, not evidence that the curriculum's promotion
rule is sufficient. The run promoted level 0 after 115/128 aggregate successes,
but its four level-0 generations solved 54/54 two-connection cases, 61/70
three-connection cases and 0/4 four-connection cases. The small number of
ceiling cases allowed promotion without demonstrating mastery of that stratum.
At level 1, generation 4 completed 18/32 training cases and 37/128 held-out
cases; generation 5 completed 16/32 training cases. Those results are valid
training evidence, while stratified sampling or promotion is a separate
curriculum decision.

The change removes redundant panel and collection reads, duplicate report
deserialization and the full encoded JSON buffer from the fresh stage boundary.
It does not change MCTS or optimizer mathematics, so the evidence establishes
lower handoff I/O and peak-memory work rather than a claimed search or training
compute speedup. A comparable end-to-end timing was not available because the
previous implementation rejected the canonical collection before training.
