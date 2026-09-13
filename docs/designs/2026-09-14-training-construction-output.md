# Training construction output without report files

Status: complete and deployed. The user authorizes cleanup and requires
that normal training never create construction report JSON or HTML files.

## Scope and contract

The active declarative learner accumulates roughly 14 GiB/hour in its generated
input bank. At 00:32 KST the bank occupied 48.4 GiB and the complete run 50.0 GiB.
A later 105-input sample attributed 64.3% of bytes to `construction.json` and
26.7% to `construction.html`. These files are not read by completed input loading.

Add an explicit `layouts construct-production --final-json` mode. It writes a
versioned JSON result to stdout containing status, completion, final physical
geometry and structured failure diagnostics, without serializing the construction
history or rendering/writing HTML. Existing explicitly requested diagnostic
report/HTML output remains available. The training admission caller uses this
mode and consumes stdout in memory; it must not save an equivalent stdout dump.
Small process status/stderr and actual admitted inputs remain durable.

An explicit one-off cleanup removes exactly the two obsolete construction files
from accepted attempts of completed input records. It preserves actual input
references, failed attempts, unpublished inputs, checkpoints, replay, curriculum
and evaluation. It uses completion markers and fixed filenames, not filename
extension deletion or historical reconstruction. No recurring cleanup worker is
added to normal training.

## Continuity checklist

| Established behavior | Preservation and verification |
| --- | --- |
| Fresh products, rates and randomized complete starts | Same draw, wiring, constructor, setup and admission; compare old diagnostic and final-only CLI output on representative products |
| Original directed topology and native validation | Unchanged native library, constructor algorithm and declarative action semantics |
| Actual episode input binding and unfinished episode recovery | Keep old bank and inputs; switch banks only at a completed wave, with explicit historical input locations and unchanged next-start selections |
| Model, Adam, RNG, successful replay and current curriculum | Directly restore the current checkpoint; only rebind the launch identity |
| Efficient collection and storage | Retain 16 workers, CUDA inference/update, compact replay, publication and existing retention; measure ordinary resumed updates |
| Fixed evaluation and best models | Keep panel, evaluation records and all selected checkpoints |
| Source identity and input-store contract | Use a new source directory and new future input bank; archive the prior launch, never rewrite old input contracts |
| Morning stop and resource controls | Preserve the requested 10:00 KST deadline and all resource ceilings |
| Existing explicit diagnostics | Keep report/HTML CLI mode; final-only failures retain structured diagnostics on stdout |

The earlier `cleanup_construction` helper was inspected. Its ownership checks are
useful precedent, but it requires another generator's `root.json`/`roots.json`
and hashes every removed report. The new operation targets the actual completed
fresh-input format and avoids those inapplicable checks and expensive rehashing.

## Bounded verification

Test final-only argument exclusivity and output shape, real constructor success
and failure without report/HTML files, training admission consumption without a
stdout dump, and cleanup preservation boundaries. Run the required workspace
format/check/tests. Use one isolated comparison per selected product and a
bounded normal generation/restore check before changing the main launch. Stop
all diagnostics before the resumed normal run. Do not replay PPO history or
rescan/hash all retained inputs as part of resumption.

## Execution log

- 2026-09-14: inspected the active input ledger, input loader, native admission,
  constructor CLI, curriculum/replay retention and prior cleanup implementation.
- 2026-09-14: removed only accepted completed attempts' obsolete files in two
  explicit cleanup passes, reclaiming 42.74 GiB of allocated storage. The second
  pass removed files generated while implementation and verification proceeded.
- 2026-09-14: all continuity rows closed. Existing workspace tests and nine CLI
  integration tests pass. Four real products have identical old/new constructor
  geometry; five new inputs restore through the normal native input path.
- 2026-09-14: paused at update 1044 and resumed unchanged learning state in the
  new source directory and future input bank. Update 1050 is published; the first
  104 completed generated inputs contain none of the three prohibited outputs.
  The morning deadline remains 10:00 KST. See the matching report for evidence.
