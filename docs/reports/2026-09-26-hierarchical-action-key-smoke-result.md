# Physical-action Gumbel key implementation smoke

Status: closed. The optional physical-action key compiles and runs, preserves
the legacy rank configuration, and removes case-ID input from its Gumbel draw.
This one-case smoke is not a performance screen.

## Question and method

An action-keyed Gumbel draw should follow the same physical macro action when
the model reorders actions or a panel changes only its case ID. First compare
the new binary's default rank path with a retained pre-change 16-case report
on its first case. Then run the full 128-simulation, horizon-six, 24-move search
on that one problem under the original ID, a renamed ID, and a same-ID repeat.
Keep the lower and outer checkpoint bytes fixed. The outer configuration's
search contract is copied to match each tested search configuration, as the
CLI requires.

The first prototype hashed the entire serialized candidate. A repeated
same-ID run reached the same physical root action sets but changed candidate
prediction and search-work metadata; its physical path diverged after five
matching roots. Route candidates carry prediction, uncertainty, search work,
ID and provenance fields that do not define the route's physical effect.
The corrected key hashes the ordered command witness for a route or network,
plus its action kind, and the current problem/observation state hash. The
panel case ID is omitted. The retained [prototype patch](2026-09-26-hierarchical-full-action-prototype.patch)
reconstructs the exact discarded source from commit `3ddf5ec6`, matching
source SHA-256 `2b28075c66afaae3a28f770cd6d048488e8b5db3f9391c1dc595b8ca33ae1760`.

| Variant and run | Roots | Physical path | Complete | Search time |
| --- | ---: | --- | --- | ---: |
| Legacy rank, original ID | 10 | Same as retained pre-change report | No | 3.266 s |
| Full-candidate key, original ID | 9 | Same as renamed ID | No | 2.425 s |
| Full-candidate key, renamed ID | 9 | Same as original ID | No | 2.437 s |
| Full-candidate key, same-ID repeat | 8 | Diverged after root five | No | 2.338 s |
| Physical-command key, original ID | 10 | Same across all three runs | No | 3.543 s |
| Physical-command key, same-ID repeat | 10 | Same across all three runs | No | 3.544 s |
| Physical-command key, renamed ID | 10 | Same across all three runs | No | 3.579 s |

The [compact summary](2026-09-26-hierarchical-action-key-smoke-summary.json)
retains report, configuration, panel and binary hashes, root counts, physical
path hashes and stage times. The default rank run matched all ten selected
actions, its first-root action records, and the outcome in the retained
pre-change `artifacts/recovery-search-v2/raw/s1/base-20260922.json` first
case. The corrected variant's three paths and state sequences also matched.
Their full action records were not byte-identical: small model scores and
candidate metadata varied even in same-ID repeats. This observation warns
against claiming exact end-to-end determinism from the key change alone.

## Verification and limits

- Source commit `bdf06c61`, optional `gumbel_assignment: action-identity`;
  omitted field stays `rank` and retains the old serialized configuration.
  The physical-key release binary SHA-256 is recorded in the summary.
- On `ashrimp-arch` with LibTorch, `cargo check --workspace --features aic-cli/learning`
  passed. The full feature-enabled workspace test passed with
  `RUST_MIN_STACK=16777216`; without that setting, an unrelated CLI
  visualization-parser test overflowed its thread stack before reaching the
  changed search tests. The seven hierarchical-search tests passed directly.
  Local default-feature `cargo check --workspace` and `cargo test --workspace`
  also passed.
- Reproduction: from `~/Srv/aic-target-causal-20260925`, use the first case
  of `artifacts/recovery-search-v2/inputs/panels/s1.json`; make a paired copy
  changing only `cases[0].id`. Use the tracked
  `docs/benchmarks/hierarchical-target-causal/c16-n128-search.json` for rank
  or add `root_selection.gumbel_assignment: action-identity` for the keyed
  variant. Copy the original outer model's `config.json` and replace its
  `search` member with the corresponding search config; keep its parameter
  order and checkpoint bytes. Run `aic-cli learning hierarchical-search-harness`
  with those inputs, the retained lower model and `data/game/normalized`.

This verifies config parsing, the integrated search path, and one problem's
case-ID invariance at the physical-decision level. It does not establish a
completion gain, multi-case robustness, or reduced run-to-run variance in
general. A bounded paired search probe follows before any new training run.
The [cleanup record](2026-09-26-hierarchical-action-key-smoke-cleanup.json)
lists the 30,872,842 bytes of disposable reports, model copies, panels and
test logs removed after the compact package was checked.
