# Remaining uncommitted source changes

Status: inspected and archived; subsequently retained in a user-requested source
checkpoint on 2026-09-07. The inventory and recommendations below describe the
pre-checkpoint state. Committing these files does not complete the experiments or
approve their production-policy integration. Learning and diagnostic runs remain paused.

"User changes" in earlier handoffs meant pre-existing or unrelated work that must not be
overwritten. It was not proof that the user personally authored every file. Authorship
of the older constructor changes is not established by the current diff. The two
choice-fit files were earlier assistant diagnostic drafts deferred during this thread.
They are not a user-authored customization and have not been executed to completion.

## Inventory and recommendation

| Files | Actual change | Current recommendation |
| --- | --- | --- |
| `docs/benchmarks/constructive-bound-interconnect/view.mjs` and `model.test.mjs` | Replace a Heavy-Xiranite-specific partial-graph warning with a generic partial-factory warning; update its assertion. | Small, coherent wording change worth preserving. It can be separately reviewed/tested/committed later; not necessary to discard for RL diagnosis. |
| `docs/benchmarks/constructive-factory-closure/model.mjs` | Add bounded A-star fallback for the explicit search family, route-search work accounting, west-boundary candidates, full-boundary realization, and opt-in pending-external-input coverage. | Do not blindly revert. This exact dirty file is a hashed dependency of the completed learning runs. Review the behavior and production-policy implications before separately integrating or replacing it. |
| `docs/benchmarks/constructive-recipe-breadth/` (seven files) | In-progress recipe-breadth contract, graph/seed/region construction, runner, tests, and three explicit request fixtures. Uses the closure additions above. | Preserve as an archived, unfinished constructor experiment. It is not imported by the current tracked RL scripts in the inspected references. It can be removed from the active workspace after an explicit decision, but should not be treated as garbage or completed production coverage. |
| `docs/benchmarks/layout-direct/choice-fit-contract.md` and `choice_fit.py` | Proposed diagnostic to fit known improving actions on complete states; 128 supervised updates, with limited teacher/contrast execution. | Preserve the draft because it may help the next diagnosis; do not run or merge it as validated code. It is not imported by the current learner, so it can be removed from the active workspace after an explicit decision without changing current RL behavior. |

The constructor file is not merely an unrelated nuisance: its recorded SHA-256 is
`91a82ac5d7a4a800a2888467813fc82bcd1dc0437118efc545d71b7515bfac76`, matching the
54-file frozen provenance of the interrupted experiment. Restoring the committed version
without retaining this overlay would break exact-source checks for that evidence.
The other eleven files do not appear in that provenance. This does not certify that
every other historical experiment or tool is independent of them.

The breadth code and closure extension are related but are not approval of the separate
parallel-component admission policy. The existing contract says in progress; local unit
test files do not establish end-to-end success or production-policy compliance.

No deletion is currently recommended before the user chooses how to handle each group.
If the user wants a clean active workspace later, an explicit archival-and-removal step
can be considered now that the overlay is preserved. Do not perform that step implicitly.

## Recoverable archive, without integrating incomplete code

[remaining-worktree.patch](remaining-worktree.patch) captures all twelve files: diffs for
the three tracked files and full additions for the nine untracked files. The exact
base and hashes are in [remaining-worktree-manifest.json](remaining-worktree-manifest.json).
Base commit: `44a854f045eb9daa6aafc56d989bc69648ccf773`.

Verification performed:

- Reverse-check of the patch against the original dirty workspace passed without applying it.
- Loaded the base tree into a separate temporary Git index, applied the patch there, and
  compared all twelve reconstructed file blobs with the current originals: byte-identical.
- All 54 frozen learning-source hashes still matched after creating the archive.
- The original files remained in place; they were not staged, reverted or deleted.

On an appropriate separate clean checkout of the recorded base, recovery begins with
`git apply --check docs/handoffs/2026-09-07-layout-learning/remaining-worktree.patch`
after making this archived patch available there. Only then apply it if recovery is
actually intended. Do not apply it to the current workspace: those changes already exist.
The archive contains no learning weights or episode data; preserve the local `target/`
evidence separately as explained in the main handoff.
