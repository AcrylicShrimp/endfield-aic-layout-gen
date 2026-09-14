# Necessity of the Python boundary in declarative learning

Status: source audit complete; proposed ownership boundary, not an implemented
cutover or a measured performance result. Audited commit `0f3ddba`. The user asked
whether data must reach Python at all, why it does, and which consumers can move
to Rust. This changes the investigation from faster object transport to removing
unnecessary Python ownership.

## Finding

Most detailed physical state reaches Python because replay and evidence consumers
were implemented there. PPO does not consume full facility placement, routing or
validated-witness dictionaries. The current interface follows those consumers;
it is not a requirement of the learning algorithm or the Rust/Python connection.

Move domain computation with its data. Encoding the same objects as bytes and
decoding them for the same Python consumers would leave the core dependency in
place. The proposed destination is Rust-owned environment and successful-experience
semantics, with numerical model work and orchestration in the existing Python
learner. Rust may run inside both workers and the central process; moving a
consumer to Rust need not move it into every environment worker.

The earlier accepted native ownership cutover deliberately kept physical-state
export at chunk completion for replay identity and geometry scoring. This audit
extends that boundary; it does not pretend those cold consumers were already
native. See [the existing design](../designs/2026-09-14-declarative-native-ownership-restoration.md).

## Producer and consumer audit

Paths below are under `tools/factory-edit-pilot/` unless otherwise stated.

| Data or work | Current concrete consumer and reason | Necessary Python exposure / proposed owner |
| --- | --- | --- |
| Numerical graph, action and port features; masks | `declarative_learning_policy.py`: batched root/conditional heads, likelihood and value; PPO/SIL require historical inputs | Keep owned arrays/tensors accessible to PyTorch. Rust already builds features and masks. Preserve shared-array transport and minibatch preparation. |
| Conditional domain descriptions | `policy.branch()` reads group order, port names, graph-node indices, rotation ranges and previous choices to build exclusion/history tensors | Numeric indices, offsets and packed branch tensors suffice for model work. Move domain traversal/branch packing to a central native helper, including historical SIL domains. Keep batched heads and CPU Torch sampling order. |
| Full selected-action description | `Buffer.ingest()` hashes it; success features inspect action kind and placement identity; sparse archives retain the selected description | Model likelihood uses root/port indices. Rust can own the semantic command and expose compact action codes, indices and an evidence reference. Keep full descriptions in native evidence for explicit inspection. |
| Changed pre-action physical states | `learning/trace.rs` builds physical records; `Buffer.ingest()` reads all states in successful segments for identity; `replay_storage.compact()` stores live selected rows | Keep states native and compute replay identity there. Export label references and the resulting identity, not decoded physical dictionaries. Detailed evidence still needs bounded retention. |
| Improvement witnesses and geometry | `Buffer.ingest()` checks positive improvement evidence; `success_features.extract()` derives `has_place`, `gain`, `exterior_release` when configured; sparse storage retains witnesses | Rust already validates the witness. Compute geometry summaries in Rust and expose validity/status plus the configured summaries. Preserve full witnesses as native-owned evidence and final artifacts. |
| Successful-segment assembly and terminal returns | `Buffer.ingest()` accumulates changed preparation actions and positive segments across chunks; assigns discounted returns only at episode termination | Move per-episode accumulation with native evidence. Export ordered terminal labels/returns and pending-reference summaries. Later rewards can credit earlier zero/negative-reward preparation actions. |
| Global replay admission, deduplication and eviction | `learning_buffer.py`, `replay_retention.py`, `replay_sampling.py`; pool-wide capacities, strata and measured utility | A central Rust registry can own domain metadata and policy decisions. Keep one global order/capacity, not independent worker buffers. Python supplies the already-computed optimizer utility and receives selected label references. |
| Chunk successor observation | `Stream.observation()` decodes full JSON; `semantic()` hashes fields; `finish_stream()` sends it and successor input in the frontier | Normal `blueprint_ppo_update.credit()` needs only stream, terminal and bootstrap. The collector already has final numerical inputs. Move continuity identity/checking to Rust; full successor records belong to explicit trace consumers. |
| Pre-chunk native checkpoint | `Worker.begin` snapshots it and `finish` sends it as `native_before`; normal code reads input metadata, original facility count and curriculum request | Supply compact admission metadata/counts and a native request reference. A complete pre-chunk decoded checkpoint is not needed for ordinary PPO. Preserve explicit trace/failure evidence separately. |
| Durable post-update native state | `Stream.snapshot/restore()` decodes/encodes checkpoint JSON and checks input/semantic identity; `Learner.checkpoint()` persists streams with optimizer/RNG/buffer state | Checkpoint content is necessary for recovery, but Python need not inspect its interior. Native validation plus versioned opaque checkpoint data can use the existing publication boundary. |
| Terminal best and inherited starts | `training_quality()` reads validation and scores; `curriculum.next_plan()` selects parents, hashes geometry, and persists request/best for later episodes | Summaries suffice for metrics and parent selection. Rust can compute reserved-geometry identity and create selected inherited input artifacts. Actual best/request bytes must remain available to selected descendants. |
| Full evaluation/training traces, final results and failures | `learning_evaluate.py`, `Worker.preserve()`, opt-in publication and offline diagnostics | Explicit artifact export remains required. Native storage/export can avoid Python object expansion, but removing these artifacts would weaken existing requirements. |

The normal declarative buffer uses `Evidence.sample()`, not the old blueprint
buffer's `Evidence.successor()` geometry traversal. Legacy blueprint audits still
read successor observations. Their explicit trace needs do not make full successor
objects necessary on every normal declarative collection boundary.

The collector also checks sample blueprint/revision against the retained numerical
input. Keep that small binding check; removing physical dictionaries does not
remove input ownership validation.

## Proposed runtime boundary

Rust in each persistent worker owns the live session, ordered domains, commands,
validation, physical history and episode-local success accumulation. It produces
owned numerical observations and compact outcome/control records. Changed states
already move into native chunk ownership; extend that mechanism rather than
rebuilding or replaying an episode to reconstruct evidence.

Rust in the central process owns global replay metadata, identity/deduplication,
retention rules and native evidence access. It accepts records in the established
logical stream/step order, independently of worker completion order. It provides
label references, scalar summaries and numerical branch inputs to Python. This
placement preserves one global replay policy without sending physical trees into
Python or introducing worker-local approximations.

Python keeps model construction, batched PyTorch forward/backward work, the two
Adam states, established sampling/shuffle generators, update scheduling, CLI,
metrics and publication coordination. Small identifiers and summaries remain
ordinary Python values. Detailed native checkpoint/evidence bytes may pass through
Python without becoming nested Python objects; necessary data movement across
processes and into tensors does not disappear merely because Rust owns semantics.

The model implementation could also be replaced, but this audit provides no
evidence that replacing it would remove more overhead than the domain boundary
changes. It would introduce a separate parity problem involving ordered sampling,
joint conditional likelihood, actor/critic updates, replay gradients and optimizer
restore. The purpose of retaining it here is to preserve the existing verified
learning implementation while removing Python's dependency on detailed factory
state, not to assert that training must intrinsically use Python.

## Continuity and decisions that must be resolved before cutover

| Existing benefit or contract | Reuse / replacement and check |
| --- | --- |
| Persistent workers, logical batching, centralized GPU heads | Reuse pool and transport. Native branch preparation must batch locally in the central process, not add a worker round trip for each conditional choice. |
| Owned historical observations and sparse selected port branches | Reuse array ownership and sparse input format where compatible. No reconstruction under the current policy; compare historical joint likelihood and nonempty SIL updates. |
| Stable successful-segment identity | Current `digest()` walks sorted keys and scalar encodings and includes physical state, action and rounded terminal return. It is not simply SHA-256 of JSON. Preserve that exact identity through a native equivalent or provide an explicit schema migration; substituting hashes of per-state hashes silently changes it. |
| Geometry feature semantics | Port existing before/after placement logic, witness extent (including network geometry), clipping and validation. Compare optional modes, not only the baseline that disables them. |
| Terminal credit and cross-chunk references | Keep changed preparation actions until future success/termination resolves them. Check a success spanning chunks and restore with pending segments; bound memory by the existing action/resource limits. |
| Global admission and sampling | Preserve terminal admission order, duplicate behavior, strata, capacities, eviction tie-breaks and no current-update replay. Keep Torch random draws or prove exact replacement; avoid a second competing registry during normal operation. |
| Ordinary checkpoint and sparse storage | Reuse one publication transaction and retained-source ledger. Persist native pending evidence/registry state with model, Adam, RNG and streams. Choose a versioned schema/legacy reader at the input boundary; do not add per-step files or a redundant journal. |
| Input reservation, identity and curriculum | Compute identities once at admission/selection, preserve reserved geometry rejection and descendant input provenance, and retain selected best/request content. No per-update revalidation of stable inputs. |
| Final validity, explicit traces and failure evidence | Keep native validation, useful diagnostics, complete requested exports and available partial evidence. Opaque normal records require an explicit bounded inspection path. |

A concrete implementation design still needs to settle the native evidence
container and retention ownership, exact identity encoding, checkpoint schema
migration, and transfer of selected curriculum parents. These are interface and
recovery decisions, not reasons to keep decoded physical objects in Python.
They should be settled together so that successful-experience identity, features
and persistence have one owner rather than independent adapters.

## Evidence and limits

The [latest measured workflow](2026-09-15-response-restoration-throughput.md)
reported 8.364 seconds of collection, 8.869 seconds of PPO and 2.406 seconds of
publication in the normal measured pair. The separate instrumented pair reported
9.155 seconds of collection, including 4.031 seconds of GC. Nested response and
chunk-finish timings overlap; they do not isolate all GC to physical evidence.

This audit establishes consumer necessity from source, not the speedup of a
proposed cutover. Costs can shift into native computation, publication or restore.
Adoption therefore requires a bounded comparable normal run including storage,
with the existing inputs, 128 logical streams, 16 workers, 32-action chunks and
64-action episodes, plus resource and semantic checks appropriate to changed
contracts. Reuse retained baseline evidence; do not restart long training or rerun
the same successful diagnostics merely to support this source audit.

No runtime source, launch configuration or training state changed during this
audit. Verification is source tracing and the repository worktree/index artifact
checks; no performance experiment or learner test rerun was needed for this
documentation-only finding.
