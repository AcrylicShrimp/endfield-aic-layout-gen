# Rust native collection and rollout integration

Status: implemented and verified as a bounded integration diagnostic after the
numerical learner gate. The user requested continuing the Rust transition. This work connects retained native
sessions to central batched inference and actual rollout credit. It does not
change the physical editor, restart training, or replace the complete-wave
training harness before replay/curriculum/publication continuity is established.

## Ownership and contracts

`aic-pilot-native::input` owns the existing bounded JSON/catalog loader; Python
bindings delegate to it. A default `python` feature preserves the existing
extension. A Rust consumer disables default features and has no PyO3/NumPy
runtime dependency. Direct ndarray use retains existing array implementations.

`aic-pilot-native::learning` exposes typed owned observations, shared immutable
port domains, retained selection handles, native limits, chunk evidence and
checkpoint restoration over the existing declarative Session/Learning/Chunk.
`learning/pool.rs` retains logical sessions on fixed physical Rust workers with
bounded command/response queues. One exchange batches commands by logical owner;
results return in logical stream order. No JSON encoding, tensor copying or
interpreter crosses this hot boundary. Catalogs are read/validated once per run
and shared. Invalid selection handles fail before charging; native elapsed/action/
routing limits and validated-best semantics remain authoritative.

`aic-learning::collection` owns graph/conditional packing, central model inference,
ordered CPU categorical draws, frozen historical rows and true-terminal GAE.
Torch stays in the central owner. Conditional choices batch by prefix depth,
preserving without-replacement port exclusion and CPU RNG order. Historical
observations own immutable arrays and share only immutable domain descriptors.
Retain successful-segment evidence at chunk boundaries for later replay admission;
do not discard changed preparation states or replace them with scalar rewards.

A bounded CLI diagnostic consumes source-bound actual training inputs, the
existing kernel checkpoint/model, external catalogs, explicit worker count and
native limits. It outputs actual transitions, credit, numerical-update evidence,
retained native checkpoints/chunk evidence and validated best results. Explicit
oracle exports remain outside normal collection. Input hashes and evaluation
geometry reservation are checked at admission. CPU/wall-clock elapsed features
remain actual; do not zero them or redefine timeout termination to force parity.

## Continuity checklist

| Existing benefit | Reuse and verification |
| --- | --- |
| Persistent 128-stream/16-worker collection | Retain logical ownership on fixed threads; batch each exchange and compare bounded collection at the existing dimensions |
| Shared transport and descriptor cache | Move owned arrays by Arc and share typed domain descriptors for the session lifetime; no serializing live observations or rebuilding metadata |
| Native ordering/masks/routes/guards | Reuse the exact existing builders and selected-step implementation; compare Python model decisions on the same recorded native arrays |
| GPU root and depth-batched port heads | Reuse the model with central inference and independent CPU Generator; compare recorded likelihood and subsequent RNG state |
| Historical PPO/positive replay inputs | Keep observations immutable across subsequent native edits and optimizer steps; preserve selected branch data and complete chunk evidence |
| True-terminal credit | A chunk boundary bootstraps an unfinished episode; only the actual episode horizon is terminal; resource exhaustion fails the diagnostic |
| Native recovery and final validity | Reuse native checkpoint validation and limits; continue unfinished sessions after a cold restore and validate retained best |
| Efficient normal work | Load stable inputs once, bounded queues and rollout bytes, no per-step disk work; time admission, collection, update and persistence separately |

The numerical kernel gate remains reusable. Full PPO epoch/KL scheduling, global
success-buffer identity/admission/retention, curriculum/input generation, held-out
evaluation and complete-wave publication remain required for final replacement.
This intermediate command must label its narrower scope explicitly.


## Boundary persistence and failure ownership

The collection command publishes one atomic boundary directory containing the
learner tensor archive, native session snapshots and a manifest bound to the
configuration, source kernel input and device. Restore validates these bindings,
actual original/request inputs and the existing native checkpoint schema. Logical
stream identity survives a change in physical worker count. Retained best layouts
are freshly validated before boundary publication; validation does not close an
unfinished episode.

Caller-owned rollout state retains completed historical inputs, in-flight choices
and available replies through failure. The native pool also retains successful
replies from an incomplete exchange. Cold failure export streams numerical records
and captures current learner/RNG state and partial native evidence. It never marks
an incomplete wave completed or substitutes this evidence for a valid boundary.
No per-step filesystem or serialization layer is introduced.

See [the integration report](../reports/2026-09-15-rust-native-collection.md) for
actual-input parity, recovery/rejection checks, measured memory/persistence costs
and the remaining production adoption boundary.
