# Stop writing construction histories during training

Status: implemented, verified and deployed to the existing overnight learner.

## Result

Normal generated training inputs now call `layouts construct-production
--final-json`. The CLI emits only final physical geometry, success/completion
status and structured diagnostics on stdout. Admission consumes this response
in memory. It does not create `construction.json`, `construction.html` or
`construction-process.stdout`, and it never renders HTML in this mode.

The constructor algorithm still creates its existing in-memory report. This
change removes detailed report serialization, HTML generation and their disk
files; it does not claim to remove the constructor's internal history storage.
Explicit CLI report/HTML diagnostics remain available and mutually exclusive
with `--final-json`. Invalid catalog details remain in the compact generation
failure record; missing inputs still identify their path on stderr.

The input loader, randomized setup, native learner, action semantics, PPO/SIL,
16-worker execution, CUDA inference, compact replay and fixed evaluation are
unchanged. The [design](../designs/2026-09-14-training-construction-output.md)
records the closed continuity checklist.

## Disk cleanup

Only accepted attempts with a completed `episode.json` were eligible. The
operation takes each input's existing slot lock and removes exactly the two
obsolete constructor filenames. Actual request/layout/input references, failed
attempts and unpublished inputs remain. It performs no report rehashing or
historical replay, and no periodic cleanup process was installed.

| Explicit pass | Files removed | Logical bytes removed | Allocated bytes unlinked on last link |
| --- | ---: | ---: | ---: |
| Initial space recovery | 25,502 | 45,905,039,576 | 43,710,746,624 |
| After the old generator stopped | 1,274 | 2,181,072,654 | 2,183,557,120 |
| Total | 26,776 | 48,086,112,230 | 45,894,303,744 |

The total is **42.74 GiB of allocated space**, excluding files whose earlier
hardlinks still exist elsewhere. Logical bytes are not a claim of physical disk
recovery. The final pass preserved 274 rejected attempts. Old input banks and
all actual inputs remain available, including the original imported prefix.

## Verification

- `cargo fmt --all`, `cargo check --workspace` and `cargo test --workspace` pass.
  Existing ignored tests retain their prior platform/diagnostic status.
- Nine actual CLI integration tests cover existing report mode, final-only
  success with identical geometry, partial construction, invalid planner/catalog
  inputs, missing inputs and mutually exclusive output arguments.
- Two Python admission tests verify in-memory stdout consumption, no stdout
  copy, the explicit final-output command and preserved nested failure details.
- Two cleanup tests verify accepted/completed scope, retained actual inputs,
  rejected/unpublished attempts, repeat invocation and symlink/input protection.
- Two source-switch tests verify the complete saved learning state, archived
  original checkpoint, explicit old input locations, morning deadline, and
  rejection of unrelated model changes or unfinished waves.
- One isolated release-mode check generated five fresh inputs covering food,
  medical, watered-grass and copper-liquid. All restored through the normal
  `Stream`/input loader. Their constructor geometry exactly matched the previous
  release CLI on the same actual source plans and budgets. It ran in 0.710 s;
  this is a bounded compatibility check, not a throughput benchmark. Temporary
  explicitly requested comparison report/HTML files were removed afterward.

An initial in-process argument-parser test overflowed the test thread stack
under workspace feature unification. Argument rejection is now checked through
the actual CLI integration process instead; the complete workspace then passed.
Source transfer also included macOS metadata sidecars. Unbound transfer metadata
was removed, while metadata already present in the prior source binding was
copied unchanged. An initial switch rejected that mismatch before publication;
no learning or input state was changed by the rejected attempt.

## Active continuation

The same run paused cleanly at update **1044**. A direct checkpoint restore
preserves model, both Adam states, RNG, replay and all selected next starts.
Only the checkpoint's launch binding changed. Historical launch/checkpoint
bindings remain available; historical input locations are now explicit in the
job ledger. No old input contract or payload was rewritten.

New source root:
`/home/ashrimp/Srv/aic-declarative-storage-20260914`

Same run output:
`/home/ashrimp/Srv/aic-declarative-efficiency-20260913/artifacts/resumed-four-hour`

Future input bank: `training-inputs-final` inside that run. Global slots, seed
mapping, products/rates, native budgets and start curriculum remain unchanged.
The new bank separates the changed constructor source contract from old actual
inputs. It is not a replacement pool or regenerated historical dataset.

The learner restarted at **2026-09-14 00:54:30 KST**, PID **36144**, using the same
[W&B run](https://wandb.ai/acrylicshrimp-personal/endfield-aic-layout-gen/runs/mltpnnfq).
The budget was adjusted for the pause to preserve the requested **10:00 KST**
stop, plus normal bounded wave/evaluation finalization.

At 00:56:13 KST, update **1050** was published. The first 104 completed generated
inputs occupied **37,566,350 bytes**, with **zero** prohibited report/HTML/stdout
files. Disk space available was **106,452,209,664 bytes (99.14 GiB)**. The process
tree used about 22.30 GiB RSS, below the unchanged 48 GiB ceiling.

| Normal update | Collection, s | PPO, s | Publication, s | Completed episodes |
| ---: | ---: | ---: | ---: | ---: |
| 1045 | 4.370 | 4.012 | 2.378 | 0 |
| 1046 | 3.144 | 3.546 | 1.271 | 32 |
| 1047 | 7.105 | 3.706 | 4.140 | 0 |
| 1048 | 3.244 | 3.637 | 1.050 | 32 |

Replay remained at 4,096 entries and full training traces remained disabled.
The complete warm update 1046 took 11.25 seconds in the ordinary run log. These
startup observations show continued operation; no learning-quality improvement
is attributed to this storage change. Build and verification processes ended
before the resumed normal learner, with no extra diagnostic hooks or workers.

## Reproduction and retained evidence

The operational commands are explicit and have no normal-run cleanup schedule:

```sh
python tools/factory-edit-pilot/operations/prune_construction_outputs.py --inputs /path/to/old-bank --summary /fresh/cleanup.json
python tools/factory-edit-pilot/operations/verify_construction_output.py --launch /path/to/run/launch.json --old-cli /path/to/old/aic-cli --output /fresh/verification
# Pause at a completed wave before switching source/constructor identity.
python tools/factory-edit-pilot/operations/switch_construction_output.py --output /path/to/run --verification /fresh/verification/verification.json
python tools/factory-edit-pilot/declarative_duration.py run --output /path/to/run --environment /path/to/tracking.env
```

The switch operation is intentionally limited to the constructor-output source
change, an already verified release, a completed wave and the existing morning
deadline. It does not rebuild replay or create another learner. Compact local
receipts are under ignored `artifacts/construction-storage/evidence-20260914`;
the compact provenance manifest records their hashes. These artifacts are not
a backup. Actual learner inputs and checkpoints remain on the server.
