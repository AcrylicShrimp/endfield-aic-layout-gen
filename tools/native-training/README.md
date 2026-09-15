# Native training

`aic-cli learning train` owns fresh initialization, input generation, persistent
native streams, complete PPO epochs, success replay, held-out evaluation and
checkpoint recovery. Python is not part of its execution path. The optional
learning feature requires a compatible LibTorch SDK and shared libraries.

From the repository root, build with the existing LibTorch environment:

```sh
cargo build --release -p aic-cli --features learning
```

Prepare a reusable balanced evaluation panel, then run a bounded smoke check:

```sh
target/release/aic-cli learning prepare-panel \
  --config tools/native-training/smoke.json \
  --output artifacts/native-training-panel --cases 4 --seed 730200000

target/release/aic-cli learning train \
  --config tools/native-training/smoke.json \
  --output artifacts/native-training-smoke --updates 1

target/release/aic-cli learning train \
  --config tools/native-training/smoke.json \
  --output artifacts/native-training-smoke --updates 4 --resume --workers 1
```

`--updates` is the **total** completed update target, including restored updates.
It can stop after a 32-action chunk; unfinished native episodes and cross-chunk
success credit are saved. The configured duration bounds each invocation and stops admission after the
current 64-action wave; `--updates` adds an earlier total-update stopping point. A running job can also be stopped
through the CLI:

```sh
target/release/aic-cli learning stop --output artifacts/native-training-smoke
```

The stop request is consumed after a complete wave is saved. Another process
cannot train into a locked output. Use `--device cpu` when CUDA is unavailable.
The default `training.json` selects 128 logical streams, 16 workers, 32-action
chunks, 256-row minibatches and four epochs. Its duration is twelve hours;
pass `--updates 4` for a short check. No long run is started by preparation.
For training, prepare a larger panel, for example 32 cases (16 per stratum).
The panel path is an external configuration field; creating a replacement panel
requires a new training run because checkpoint identity binds its actual inputs.

## Physical coordinates

The native limits/decision schema is `declarative-edit-v2`. Working geometry is
normalized to its occupied minimum; outputs use that local frame while original
source inputs retain their exact identity. Move destinations may be negative
before the whole draft is rebased, and routing uses symmetric signed padding.
Occupied-span, path-length, search-area and work limits still apply. Translating
an entire factory does not create an improvement or a new physical state.

Native physical snapshots use `declarative-edit-checkpoint-v3`. The v2 physical
reader verifies its old bindings and saved state, then converts current/best
geometry once without regenerating paths or losing pending obligations. This is
separate from learned checkpoint continuation.

## Physical observations

The current model consumes 41 node columns (31 static + 10 state), 37 edge
columns (33 static + 4 state), eight directed path-cell columns, 15 globals,
12 root-action columns and 11 conditional-port columns. Width 40 and two message
rounds are unchanged. The native [typed schema](../../crates/aic-pilot-native/src/full_factory/blueprint/declarative/learning/schema.rs)
defines column offsets; [plan 02](../../docs/plans/2026-09-15.02-authoritative-physical-observations.md)
defines field units, applicability and relation endpoints.

The graph carries complete catalog port layouts, selected and original port
relations, component capacity/direction masks, separate bridge channels, network
membership and commodity equality. Names, recipes, utilization and wall time
are not numerical features. Current route restrictions match the repair query;
baseline headings retain the constraint restored by returning to an original
port. Facility baseline headings use the unrotated catalog frame. External and
component headings use the fixed grid frame. External boundary normals remain
explicit.

Lengths use the maximum occupied width/height as scale; inverse scale and
`ln(1 + width/height)` globals preserve grid units. Rates/capacities use
`ln(1 + quantity per second)`, while native validation retains exact values.
Other logarithmic globals also use `ln(1 + x)`. Path cells carry incoming/outgoing
unit directions and endpoint bits. Non-transport relation edges do not create
routing obligations. Unchanged geometry reuses the retained view; decision and
work budgets still refresh, and previously collected rows remain immutable.

Observation limits count actual vector capacities across live/retained inputs,
once per shared input owner, before collecting another row. Compact replay
counts retained numerical lengths, branch/reference storage and the selected
assignment bank. Neither estimate
replaces the existing process RSS guard.

## Conditional port choices

Terminal factors follow their original physical facility-local port geometry,
role, transport, exact rate and immutable endpoint incidence. Port alternatives
follow base catalog geometry and original-terminal membership. IDs remain lookup
keys. Admission records explicit equivalence classes; current selection features
refine these when comparing interchangeable labels. Native execution still
receives every exact terminal-to-port assignment.

Each selected action retains one bank of `(terminal graph node, 11 port features)`
records. A factor at depth `d` uses records `[0,d)`. A nonlinear width-40 pair
encoder combines each terminal embedding with its selected port before taking a
prefix mean and appending `ln(1+d)`. Empty prefixes are zero. Collection encodes
each new pair once; PPO and sparse replay recompute every pair with gradients
once, then use prefix sums and gather one summary per factor for its alternatives.
The cumulative sum uses double precision to avoid cancellation between packed
banks; model features and parameters remain float. No train-time embeddings or
repeated copies of growing histories are saved. See [plan 03](../../docs/plans/2026-09-15.03-terminal-bound-port-prefix.md)
for the binding, ownership and restore checks.

## Outputs and recovery

Progress lines on stderr report new transitions, never repeated PPO/replay uses.
`updates/*.json` records epoch coverage, actor KL stopping, replay admissions and
utility, timing, train-area improvement and resource totals. `report.json`
contains the last invocation's throughput, including initialization or restore,
fresh input construction, training, evaluation when due, and checkpoint work.
Compare cached and fresh-input measurements separately.

`latest.json` points to one atomic `checkpoints/NNNNNNNN` directory. It owns the
model, Adam and RNG state, current wave or selected next starts, replay metadata,
and live native sessions when needed. Numerical replay inputs are immutable
sparse binary chunks written at checkpoint cadence. Stable input hashes are
checked once at admission/restore. A checkpoint also includes `best.safetensors`
when a complete held-out evaluation has improved the retained best score.
Only model tensors are retained for that best; normal optimizer snapshots are
not duplicated. Recovery keeps the configured recent checkpoints and explicit
`keep_updates`. Selected next starts include their actual layouts and root
obligations; restore does not regenerate them from seeds or replay trajectories.

Use the same config and `--output ... --resume`; worker count may change through
`--workers` while logical streams stay fixed. Changed catalogs, input-generation
config or source plans, evaluation panel, model, optimizer, device or logical
stream settings are rejected. Recovery is continuation from the selected actual
checkpoint; bitwise reproduction of a whole historical training trajectory is
not a promised recovery requirement. Failures preserve partial learner/native
states and numerical evidence. Later uncommitted outputs and interrupted
construction are preserved outside normal retention before retrying.

Memory, observation, replay, elapsed-time, free-space and retained-output limits
are explicit config fields. Background resource polling is separate from tensor
work. Normal evaluation does not collect training traces or change training RNG,
replay, optimizer or curriculum. Incomplete evaluation cannot replace the best.

## Migration and bounded comparison

Training configurations use `aic-training-v1`. Checkpoint manifests use
`aic-training-checkpoint-v6`; this is the single learned-state format version.
When model input, action or replay meaning changes, increment that version even
if tensor shapes stay the same. Existing config binding covers runtime settings.
Restore checks the manifest version before loading learner, Adam, RNG, best-model
or replay payloads. An unsupported/missing version reports
`aic-learning.checkpoint-version` with the path and expected/actual version.
Old checkpoints require their historical build or fresh training; do not relabel
them with the current version. Native physical snapshots keep their separate
schema and admission rules. Failure evidence is not resumable. There is no
independent production model/replay import entry point.

The temporary `learning collect` command, collection checkpoint format and
external learner/replay seed import have been removed. They did not provide a
complete training schedule. Start a new `aic-training-v1` run; those diagnostic
checkpoints are not resumable training checkpoints. Retained tensor comparisons
remain explicit Rust tests, outside the executable. Existing Python source is
unchanged.

The generator reuses the existing native construction and seven setup-edit
algorithms. Its versioned `aic-native-generation-v1` source uses SplitMix64 for
uniform draws and shuffling, so Python generation seeds are not relabeled as
native input-bank identities. Setup still chooses a valid randomized current
layout rather than selecting an optimized best. This preserves the existing
training distribution's setup operations; redesigning that generator is a
separate algorithm decision.

For a cached-input comparison after a four-update fresh run, the optional Node
helper copies checkpoint 2 and the actual next-wave slots into a distinct output:

```sh
node tools/native-training/prepare-cached.mjs \
  artifacts/native-training-fresh artifacts/native-training-cached 2

target/release/aic-cli learning train \
  --config tools/native-training/training.json \
  --output artifacts/native-training-cached --updates 4 --resume
```

This is explicit experiment preparation. The normal training command still
constructs fresh canonical inputs when they are absent and verifies cached slot
identity when present. The helper does not synthesize, relabel or replay inputs.
