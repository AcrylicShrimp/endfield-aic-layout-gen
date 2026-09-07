# Compound geometry/compaction capability, schema 1

Status: development experiment; original one-step measurements remain frozen.

Input is an explicit root dataset, hash-checked development witnesses, action levels and
the same proposal/aggregate-routing/per-action-routing/deadline/validator budgets used by
the capability probe. Each level additionally requires `compact_after: true|false`.
The constructor, original RL environment and one-step probe contracts remain unchanged.

Actions:

- `compact`: only event-free whole-grid strip contraction, as a control.
- `translate`, `rotate`, `port`: the previously declared raw domains, followed optionally
  by compaction. Do not call these compound results pure one-step improvements.
- `void`: translate a singleton or declared dependency group so its body envelope aligns
  with each of four corners of each fitting selected empty rectangle. Both X and Y may
  change, without a cardinal-distance cap. Select up to eight largest non-contained empty
  rectangles using the existing deterministic occupancy diagnostic; this is not all voids.
- `contact`: translate a declared group adjacent to each retained facility connected to it
  by an internal graph edge. Try four sides, gaps zero/one, and both cross-axis alignments.
  These are dependency contacts, not every possible spatial neighbor or arbitrary subset.

Void/contact membership uses the same singleton, internal pair and bounded depth-four
dependency-cone generator as the prior probe. Deduplicate action parameters; retain raw
collision candidates, report generated counts and stop if the 100,000-action guard is
exceeded. All generation and filtering CPU time is charged. These finite heuristic domains
do not establish global infeasibility or completeness over legal layouts.

Each trial starts from the frozen root. Route pipe first, with original-chain near-distance
priority, terminal-flexible repair, declared junction ownership, no new crossings and
retained external positions. Validate the intermediate complete factory with Rust and the
original request before attempting compaction. Normalize coordinates before contraction.
Validate the compacted result independently as well. The compaction-only control also
revalidates its normalized intermediate root; no candidate validity is inferred from geometry.

Preserve the production identity used by the previous probe, including original facility
inventory, rates, requirements and external obligations. Keep any timely, independently
valid intermediate improvement even if later compaction fails or expires. Compaction may
never conceal an invalid intermediate, mutate the root or replace the incumbent with a
worse score. Area, physical transport tiles and directed turns remain lexicographic.

Output: per-level trace, best witness, summary and provenance. Each attempted phase records
status, score/hash when valid, validation identity, compaction changes and timing. Report
intermediate improvements separately from improvements newly enabled by compaction, raw
domain coverage, actual route/validation calls, expansion totals and all elapsed work.
Known local static/routing failures remain diagnostics; unknown exceptions abort. Late
results cannot be accepted. Partial exploration is reported as resource-limited, never
as no legal opportunity. No training or sealed-test evaluation occurs in this experiment.
