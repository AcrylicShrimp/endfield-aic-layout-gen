# Blueprint training again consumes fresh random factories

Normal blueprint duration training now generates a new product/rate request and
randomized complete valid initial factory at each true episode reset. The earlier
five-facility input was a diagnostic fixture that incorrectly carried into main
training after the blueprint cutover. Restoring this requirement precedes any
advantage-normalization comparison; the old capacity screen does not establish
the best architecture for the restored distribution.

## Changed contract

Training input is mandatory and explicit. Normal duration preparation rejects a
fixed fixture. Named mechanical/contract diagnostics retain an explicitly marked
fixture configuration. A new run owns its own input directory by default; an
experiment must explicitly choose a shared directory to pair actual episode
inputs across conditions. No completed input slot is recycled across episodes.

The existing four-family generator supplies products, rates, complete constructive
factory admission and randomized valid setup. Its catalog/source/binary hashes,
actual request and witnesses, setup draw, admission attempts and per-episode
identity are retained. A constructor rejection uses a bounded next attempt, never
a fixed fallback. A partially published or modified record causes a structured
failure. Equal random draws or geometries are allowed and reported honestly.

Each stream owns its admitted request and original factory. Native checkpoints
and learner metadata bind that original to the unfinished draft. Restoring a
chunk does not regenerate an input or reset an episode. Commit and final-witness
audits use each episode's own original, rather than a run-wide fixture. This
removes the shared-original assumption from training and physical verification.

Durable update JSON and W&B expose generation episode/attempt/rejection counts,
products and input-access cost. Recorded original setup cost is separate from
actual access cost when paired policies share an input. The standalone input
summary verifies ordered slot coverage and authenticates actual input records.
The repository policy now explicitly requires preserving fresh episode generation
across future learner/environment replacements.

## Verification

The final native readiness run retained the actual 32-action chunks and 64-action
episodes. It stopped after update one with live drafts, resumed in a new process,
and completed six updates / 1,536 actions / 24 episodes.

| Actual input coverage | Count |
|---|---:|
| Distinct generation seeds | 24 |
| Copper-liquid / medical / watered-grass / food | 7 / 8 / 7 / 2 |
| Distinct product/rate pairs and production requests | 20 |
| Distinct initial geometries | 23 |
| Generation attempts / rejections | 24 / 0 |

The full audit reconstructed all six learner/Adam/RNG/buffer updates, checked
48 native snapshot restorations and 24 nonterminal continuations, and verified
all 24 episode completions. All 96 evaluation cases completed 64 actions, for
6,144 evaluation actions; those evaluations did not consume training input slots.
There were 144 distinct independently physically validated witnesses. Small
readiness improvements are not evidence of learned generalization.

New tests reject missing input declarations, accidental main-training fixtures,
recycled seed slots, swapped or missing restoration provenance and changed input
files. Real constructor/native tests verify distinct episode inputs, no generation
on restoration, paired-input reuse, and evaluation isolation from live training
originals and sampling RNG.

The Python suite ran 420 tests: 413 passed and seven skipped. Rust workspace
checking passed; tests passed 564 cases with five ignored. No Rust or game-data
semantics changed. The input-summary CLI and actual prepare/pause/resume/audit
commands passed. Artifact/index checks are required before committing this slice.

## Reproduction and retained evidence

```sh
cargo build --release -p aic-cli -p aic-pilot-native
tools/factory-edit-pilot/.venv/bin/python tools/factory-edit-pilot/blueprint_duration.py prepare \
  --config tools/factory-edit-pilot/blueprint-success-64-12h.json \
  --native-library target/release/libaic_pilot_native.dylib \
  --output NEW_READINESS_DIRECTORY --readiness --evaluation-cohort RETAINED_COHORT_JSON
tools/factory-edit-pilot/.venv/bin/python tools/factory-edit-pilot/blueprint_duration.py run \
  --output NEW_READINESS_DIRECTORY --stop-after-updates 1
tools/factory-edit-pilot/.venv/bin/python tools/factory-edit-pilot/blueprint_duration.py run \
  --output NEW_READINESS_DIRECTORY
tools/factory-edit-pilot/.venv/bin/python tools/factory-edit-pilot/audit_blueprint_duration.py \
  --run NEW_READINESS_DIRECTORY --all
tools/factory-edit-pilot/.venv/bin/python tools/factory-edit-pilot/summarize_blueprint_inputs.py \
  --run NEW_READINESS_DIRECTORY --output NEW_INPUT_SUMMARY_JSON
```

Use `.so` on Linux. Historical fixed-fixture runs require their recorded frozen
sources/configuration; do not rerun them using the changed main configuration.
The selected local evidence is under ignored
`artifacts/factory-edit-pilot/blueprint-random-restoration-20260912/`, with
`final-readiness/`, its actual `training-inputs/`, and `final-input-summary.json`.
The initial readiness also remains with its source snapshot. Neither
artifact storage nor a seed is a backup or a guarantee of regenerating elapsed
native observations. Compact provenance binds selected evidence and verification.

Next: the matched fresh-input advantage-scaling experiment in the
[registered design](../designs/2026-09-12-blueprint-random-generation-and-credit.md).
