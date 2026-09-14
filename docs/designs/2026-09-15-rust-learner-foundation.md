# Rust learner foundation

Update: the user-authorized [Rust runtime cutover](2026-09-15-rust-runtime-cleanup.md)
removes the Python extension and porting CLI/exports described below. The numerical
and collection evidence remains historical; use the new design for current commands,
seed/checkpoint formats and execution behavior.

Status: first numerical/recovery slice implemented and verified. The subsequent
native collection gate is now verified too; see the linked follow-on design below.
The user authorized starting the Rust transition and required systematic code
placement with crate boundaries. This
first executable slice establishes the existing model, numerical PPO/SIL update,
independent Torch RNGs and complete learner-state recovery. It is not adoption of
a replacement long-run harness.

## Ownership and location

| Location | Responsibility |
| --- | --- |
| `crates/aic-torch/src/` | Safe LibTorch support: independent RNGs, explicit Adam parameter ownership and optimizer-state access. No factory or PPO semantics. |
| `crates/aic-torch/native/` | The narrow C++ ABI that exposes missing LibTorch APIs. Catch exceptions at the boundary; owned handles and tensor lifetime rules are documented in Rust. |
| `crates/aic-learning/src/` | Numerical graph model, joint root/port likelihood, PPO/SIL update, versioned tensor input and learner checkpoint. Split modules by these responsibilities. |
| `crates/aic-cli/src/learning.rs` | CLI parsing and dispatch only. Learning is an explicit build feature so ordinary data/solver builds do not require LibTorch. |
| `tools/factory-edit-pilot/diagnostics/` | One-time export and comparison against the existing Python implementation. Never imported by the Rust runtime. |

Use `foo.rs` and `foo/`, with no `mod.rs`. Keep source and schemas in their owner
crate. Do not put the new implementation in standalone benchmark scripts or add
another Python production learner. Factory generation, routing and editing remain
in their existing Rust modules; their native worker integration follows the
learner-state gate rather than duplicating those algorithms.

## Scope and contracts

The source audit of `tch-rs` main at
`4227b89b72059b0651ff83a38637693574e0a2d6` found weight serialization but no public
optimizer-state persistence or independent Generator wrapper. Autocast exists;
GradScaler is not required by the current full-precision learner. Reuse `tch`
tensor/autograd/model primitives and owned LibTorch Generators. For Adam, retain
Python's efficient foreach orchestration and expose ATen fused Adam through the
small `aic-torch/native/optimizer.cpp` bridge. The standard C++ Adam implementation
uses a per-parameter update path, so it is not adopted as a performance baseline.

The bridge owns named FP32 first/second moments and FP32 step counters, with
canonical int64 counters at the file boundary. It supports dense parameters on
one CPU/CUDA device, zero weight decay, separate actor/critic parameter groups,
foreach clipping, and missing-gradient skip semantics. Fused Adam is CUDA-only
and explicit; foreach remains the default. AMP, AMSGrad, differentiable/capturable
Adam and arbitrary parameter groups are not supported by this first slice.
Reject incompatible imported state rather than silently changing its meaning.
The numerical kernels and Torch RNG algorithm remain upstream implementations.

[tch PR #952](https://github.com/LaurentMazare/tch-rs/pull/952) demonstrates a
small optimizer save/load binding. Its opaque C++ optimizer archive does not
provide the explicit Python foreach/fused state import needed here. Do not fork
`tch` for this boundary; own the narrow bridge and run the same-version numerical
and recovery gate on upgrades. `native/NOTICE` records the adapted PyTorch
operation sequence and upstream license.

On Linux, retain the CUDA registration library when linking under `--as-needed`.
The bridge holds a symbol reference using the installed CUDA hooks header; it
does not require `LD_PRELOAD` or initialize a device for ordinary CPU execution.

The server's existing Python is Torch 2.14. The first build uses an isolated Torch
2.13 environment matching `tch` 0.26; do not suppress the version guard, replace
the existing environment, or claim cross-version exactness. Export tensors and
run the Python numerical oracle with the same backend version as Rust. Retain an
explicit source-version record for any existing checkpoint converted for the
diagnostic.

Inputs are versioned, bounded manifests plus named tensor data. They contain the
actual prepared graph/conditional branch arrays, selected indices, frozen old
log probabilities, advantages/targets and successful replay labels/weights.
These are a diagnostic interchange boundary, not a per-step runtime file loader.
Load and validate once, then reuse numerical storage during measured updates.

Outputs are a standalone learner checkpoint and a structured verification report.
Checkpoint state includes named model parameters, both Adam accumulators and
step counters, independent sampler/shuffle/replay RNG states, update identity and
the model/config schema. Model weights alone are never a resumable checkpoint.
Missing, incompatible, non-finite or misowned state must fail with a diagnostic
identifying the stage and input path. Publish a complete checkpoint atomically;
no per-step journal or second redundant recovery snapshot is introduced.

Verification exercises nonzero optimizer state, nonempty successful replay,
conditional port likelihood and a critic-only update that leaves actor parameters
and actor optimizer state unchanged. Compare continuous execution with separate
process save/restore, including the next random choices and update. Compare
Python and Rust forward outputs, loss, gradients and optimizer state with explicit
tolerances where backend execution order differs; do not relabel a numerical
difference as exact parity. Resource timings include checkpoint save/load.

## Continuity checklist

| Established benefit | Preservation and check |
| --- | --- |
| Native feature/mask construction and actual inputs | Export retained authentic inputs or use the existing native session; no fabricated successful training data. Controlled fixtures remain explicitly labeled diagnostics. |
| Graph batching and conditional branch caches | Consume packed arrays and reuse frozen branch preparation; preserve ordered domains, selected prefixes and without-replacement port choices. |
| Existing PPO/SIL objective | Keep clipping, entropy, stage-balanced positive-advantage imitation, detached critic weighting, separate actor/critic clipping and actor-safe critic tails. |
| Independent sampling/shuffle/replay streams | Use owned LibTorch generators; preserve stream ordering and state. No global-seed swapping. |
| Historical input ownership | Preserve detached immutable inputs; updates mutate only model/optimizer state. |
| Model/Adam/stream recovery | Prove learner state first. Existing environment, replay-buffer and complete-wave recovery remain mandatory for the later native-worker cutover; this slice does not replace the normal launch. |
| Efficient normal storage and work | Keep explicit diagnostics separate, load inputs once, checkpoint at the requested boundary, and measure rather than assume benefits. |

The latest normal baseline remains the recorded 128-stream/16-worker workflow in
`../reports/2026-09-15-response-restoration-throughput.md`. A numerical learner
probe cannot be compared to its whole-run sample rate. Native worker integration
must later measure the same useful-work denominator, storage, recovery and final
native validity before adoption. Existing long runs remain stopped.

## First-slice evidence and next boundary

See `../reports/2026-09-15-rust-learner-kernel.md` for input provenance, numerical
and exact recovery results, measured costs, build commands and limitations.
The checkpoint is explicitly bound to one diagnostic input, model, optimizer
backend and device. It is not yet the normal training checkpoint format.

The implementation sequence after this foundation is native persistent worker
collection and batched inference, actual rollout freezing/GAE, then PPO epoch/KL
scheduling, success-buffer sampling/publication, full-wave persistence and
integrated CLI launch/resume. Reuse existing native pool, masking, packed feature
and storage designs. This slice does not remove the working Python harness or
restart training. Preserve its useful-work throughput and recovery contracts
before adopting the complete Rust path.


The following [native collection integration gate](2026-09-15-rust-native-collection.md)
now implements persistent native workers, batched inference, actual rollout/GAE
and boundary resume. Its [report](../reports/2026-09-15-rust-native-collection.md)
records parity and bounded costs. Full PPO scheduling, global success-buffer
ownership, curriculum/evaluation and normal wave publication remain the adoption
boundary described above.
