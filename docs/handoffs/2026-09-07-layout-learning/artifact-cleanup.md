# Generated-artifact cleanup

Status: completed scoped maintenance, 2026-09-07. Training and diagnosis remain paused.
The user authorized discarding regenerable raw output and excluding it from commits.

A subsequent [source-only cutover](../../maintenance/artifact-cutover.md) completed
the tracked benchmark migration deferred below. These measurements describe the
earlier disk cleanup, not the later relocation of tracked artifacts.

## Outcome

`du -sk target` decreased from 86,806,476 KiB to 49,381,960 KiB:
82.79 GiB to 47.09 GiB, reclaiming approximately 35.69 GiB of allocated directory size.
These are directory measurements, not a guarantee of filesystem free-space growth
when filesystem snapshots or other retained copies exist.

- `cargo clean --profile dev`, after its dry run, removed 50,241 build files.
  Cargo reported 22.3 GiB of apparent file size; the prior allocated `target/debug`
  size was approximately 13.16 GiB. These describe the same files, not additive savings.
- Removed 749 regenerable intermediate candidate directories from 17 old constructor
  experiments: 23,886,779,803 file bytes, approximately 22.25 GiB of apparent size.
  These were per-candidate placement/routing/validation JSON, rendered views and logs.
- Preserved release builds, measured at 374,992 KiB, and all paused training runs.

The selected runs were `output-space-shelf60`, `output-access-shelf60`,
`output-space-shape60`, `output-access-shape60`, `layered-crossings-refine`,
`approach-columns-refine`, `shape-packing-fill60`, `shape-packing-shelf-fill60`,
`cluster-pocket-refine`, `approach-columns-fresh-refine`, `clone-portfolio`,
`two-sided-refine`, `cluster-pocket-refine-2`, `approach-columns-fresh-nesting`,
`cluster-packing-portfolio`, `layered-crossings-final`, and `adjacent-bank-factory`.

## Retention and recovery

Only explicitly resolved, non-symlink `packing-N` and `cluster-N-stage-N-N`
subdirectories were removed. Every run's root report/view, incumbent and entire
top-level directory containing its selected result were kept. A scan of 497 root
reports and 1,114 tracked documentation JSON files protected candidates referenced
through provenance or selected-directory fields. This was not a universal link audit.

The ignored local deletion manifest is
`artifacts/maintenance/2026-09-07-cleanup-plan.json`. It lists exact paths, byte counts
and retained file hashes; it is not a backup and is intentionally not committed.
184 retained report/view/incumbent/selected-result file hashes matched before and
after deletion. Root reports still describe all historical candidates: links to
removed intermediate details will no longer open until those outputs are regenerated.

Removed files were not moved to trash or archived. Dev builds can be rebuilt with
Cargo; candidate output requires rerunning the relevant constructor experiment with
the recorded inputs/configuration and historical source versions as needed. This is
regeneration, not a promise of byte-identical timing or restored execution history.
No rerun was started as part of cleanup.

The three interrupted learning streams are not assumed reproducible from a seed or
the 256 checkpoint. Their raw evidence and snapshots remain untouched. All 28 path/hash
entries in `artifacts.json`, all 54 frozen source/input/binary hashes in the study's
provenance, and all 13 overlay/archive hashes in `remaining-worktree-manifest.json`
passed after cleanup. No unrelated source changes were deleted or integrated.

## Future storage and current limits

`AGENTS.md` now prohibits committing regenerable raw runs by default. Disposable
outputs go under ignored `target/`; locally retained checkpoints/evidence should go
under ignored `artifacts/`, outside Cargo cleanup. Existing training evidence was not
moved in this slice, so full `cargo clean` would still destroy it. Neither directory
is a durable backup. Commit small findings, scripts, reproduction configuration and
hash manifests instead; preserve necessary runtime data and intentional fixtures.

`target/` already had zero tracked files; `artifacts/` is now ignored too. Existing
tracked benchmark payloads were not bulk-untracked: some are actual later-stage inputs
or fixtures. Their reference-aware migration and the remaining smaller/other experiment
directories are not completed by this scoped cleanup. No Git history was rewritten.
Ignore rules do not retroactively remove tracked files or prevent force-adds.

Verification: deletion preflight and post-deletion hash checks, `git diff --check`,
`git check-ignore -v --no-index` for both output directories, and zero output from
`git ls-files target artifacts`. No application code or Rust behavior changed;
Cargo check/tests were not rerun merely to recreate the removed build cache.
