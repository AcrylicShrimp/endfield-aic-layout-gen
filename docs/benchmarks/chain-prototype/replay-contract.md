# Milestone 4: completion-aware chain replay

Status: implemented and verified; see the [milestone-4 report](replay-report.md) for conditional
results and limitations. This contract was established before implementation. Experimental only;
milestone-3 commands remain an explicit first-completion baseline. Production and exact research
are unchanged.

## Inputs and work ownership

Input is an unchanged schema-2 factory request plus a separate schema-1 search policy. The
factory graph, exact rates, allowed geometry proposals and external obligations remain the
original acceptance contract. Search adds no screenshot pose, final W/H or inventory rewrite.

The search policy specifies a candidate shortlist limit, a per-frontier proposal/route/validation
work slice, a maximum number of routed continuation trials, a maximum rollback facility count,
and a global body-fit check allowance. Ranking controls are `partial-area` and `remaining-fit`;
both must evaluate final candidates by complete routed validation and the same final score.
The proposal-order control distinguishes M3 family exhaustion from interleaved family proposals.
This allows the later comparison to separate proposal generation from candidate ranking.

The factory request's existing proposal, expansion, validation and elapsed caps are one shared
account for the entire run, never reset per candidate, continuation or rollback. Work slices
are ceilings within that account, not extra budgets. Body-fit checks have their own reported
shared allowance in both ranking controls. Time covers all search and fit work. A locally
exhausted frontier slice may yield control to another candidate without exhausting the global
account; reports distinguish those causes explicitly. Initial tests retain the M3 caps.

## Reusable routed transaction kernel

Factor the existing M3 transaction logic into one kernel that can yield independently validated
singleton or reciprocal-cycle candidates. Keep all induced edges routed before adding the next
facility. Only output-closed groups are eligible for a committed state. Both M3 first-completion
and M4 replay use that kernel; do not duplicate it or invoke the old production constructor.

A shortlist contains a bounded number of real routed, validated transactions. Enumeration may
interleave the existing straight/left/right/fold proposals to prevent one approach monopolizing
work. This changes proposal priority, not legal game rules. Rejected overlap, port, access,
route and validation choices retain their diagnostics. The existing conservative pending-port
heuristic remains explicitly heuristic, not proof of impossibility or completed fit.

The bounded shortlist keeps one completed transaction per initial relative family, up to its
size limit. Once represented, that family's generator stops. This deliberately omits later
same-family port/pose variants; it is not completeness-preserving pruning.

## Remaining-body fit heuristic

For each shortlisted routed prefix, load the shapes and allowed rotations of all missing
facilities. Estimate accommodation using the actual frozen body/transport mask and temporary
body-only placements, including growth beyond every side of the current bounds. Charge pose
checks to the global fit allowance. Distinguish completed body packing from an exhausted or
partial estimate. Inspect shape dimensions and occupied cells, not just total empty area.

These temporary placements are never factory proposals, routed states, committed geometry or
independent validation input. They omit future port/route interactions and may use a greedy or
sparse pose enumeration. A fit score is not a minimum, sound lower bound, feasibility proof or
reason to remove an alternative from the legal domain. A partial estimate cannot masquerade as
successful accommodation. It only orders the already validated shortlist; actual completion
and final quality require routed continuation and the original full-factory checks.

## Continuation, rollback and incumbent contract

Continue each chosen candidate by the same connected, immediately routed transactions. Keep
the selected path's validated snapshots and alternative candidates. Record which starting
candidate led to a complete factory or which frontier/budget prevented completion. Never use
an incomplete prefix's area as a complete score. Completed candidates are compared lexicographically
by actual used area, physical belt/pipe cells, then a consistently defined turn count.

At a stopped frontier or completed trial, a replay episode may restore a preceding snapshot
and retry alternatives within a bounded recent connected suffix of supplier transactions.
Reciprocal cycles are atomic. The union of removed facility nodes must be connected and no
larger than the declared rollback facility count; the sink seed is not removed. Record the
removed nodes/requirements, frozen prefix, anchor, restored artifact and selected alternatives.
An episode cannot walk its rollback floor arbitrarily backward after repeated failures.

Previously accepted unrelated prefix facilities, utilization, port assignments and internal
routes must survive byte-for-byte modulo canonical translation. Changing the internal used
frame affects the external perimeter transaction: its lanes are replayed privately as one
explicitly owned closure group, not silently retained with invalid side semantics. No unrelated
internal route is ripped up. This first replay stage does not claim arbitrary-region relocation.

Keep a separately owned best complete incumbent. Failed, late, incomplete, worse or equal-score
trials never replace it. Before any completion exists, retain the deepest validated output-closed
growth prefix separately from private rollback paths; replay cannot reduce its covered count.
Every complete replacement passes the unchanged independent geometry check and original-request
coverage checker. Termination returns the complete incumbent if one exists, otherwise a
structured stopped result and the retained valid prefix, never global infeasibility.

## Reports and verification gates

Reports contain input/policy/source provenance; global and slice work; candidate generation,
ranking and fit evidence; trial ancestry and actual completion outcomes; rollback ownership and
preservation checks; complete incumbent history; retained/internal versus private state; exact
external closure; and stage/entity diagnostics with stable `chain-` codes. CLI outputs are fresh
and immutable. A final witness must independently verify using the original factory request.

Required tests and experiments:

- M3 regression geometry and work remain reproducible after factoring the kernel.
- Nontrivial generated rollback/replay preserves an unrelated accepted chain and routes every
  reintroduced facility before expansion; no replay exceeds its fixed episode floor/count.
- A shape-sensitive fit counterexample distinguishes equal or misleading empty-area signals;
  budget exhaustion and temporary body witnesses cannot become factory success.
- Real routed continuation can reject a smaller prefix or select a better complete result;
  unsupported or failing controls remain in the report, not removed from the objective.
- Global work includes rejected candidates, fits, private cycles, rollback, boundary work and
  independent validation. Tight caps and late results retain an existing complete incumbent.
- Final scores use a common turn definition, with hand-checked port-arm/segment cases. Earlier
  M3 segment-only scores are not silently compared with the new metric.
- Run original battery/citron/both-iron requests, explicit proposal/ranking controls and separate
  witness verification; inspect CLI diagnostics and rendering; run broader Node/workspace tests.
- Report measured completion, quality and work, confirmed fixes and remaining limitations;
  compare the accepted planner design/policy before a focused milestone commit.

Failure research starts with the M3 evidence: straight-first battery spent 941 of 1,009 proposals
at G1, while right-first completion spent most later proposals on body overlaps. Test whether
bounded frontier slices, interleaving and connected replay distribute existing work usefully
before increasing any cap or adding another heuristic. This milestone does not finish the
equal-total-work comparison or final research recommendation required by milestone 5.
