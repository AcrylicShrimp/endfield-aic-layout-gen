# Phase 3 Prior-Port Subset Cliff Diagnosis

## Purpose

Decompose the measured Phase 3 transition between fixed preceding placements and fixed preceding
facility ports. The experiment determines whether one preceding facility, one pair, or only all
three facilities must expose their exact port choices before search collapses.

## Inputs

- the minimum-rate Heavy Xiranite workload;
- cumulative Phase 3 at exact used dimensions `16 x 16`;
- introduced-facility coordinate `(8, 5)`, port assignment `5`, and rotation `0`;
- a validated exact Phase 2 incumbent;
- fixed placements for every Phase 2 facility;
- the exact sparse endpoint-support encoding; and
- five seconds of search time per subset.

## Exact subset lattice

The Phase 2 facility instance IDs are sorted lexicographically and assigned stable bit indices.
For `n` preceding facilities, every mask in `0 .. 2^n` is executed. A selected bit posts the
reference port equality for every matching facility terminal of that instance. An unselected bit
leaves all of that facility's port choices to Pumpkin.

The current diagnostic has three preceding facilities and therefore eight complete subsets:

- no facilities;
- each of three singleton facilities;
- each of three facility pairs; and
- all three facilities.

Placement equalities remain identical in every case. Routing, flow, item assignment, topology,
capacity, transport occupancy, and logistics-component state remain solver decisions.

## Output contract

The structured report records:

- stable bit-to-facility mapping and matching terminal count;
- subset mask, selected facilities, and fixed terminal count;
- outcome, construction time, search time, and first-incumbent time;
- branch decisions, backtracks, conflicts, learned clauses, and solver propagations;
- model scale and complete per-case layout or failure evidence; and
- the standard diagnostic-only marker.

JSON, HTML summary, and per-case HTML files are generated automatically for every outcome.

## Interpretation

Compare each subset with its immediate subsets:

- a singleton collapse identifies one facility's endpoint block;
- a pair collapse whose singletons remain unknown identifies a cross-facility interaction;
- only the full-set collapse identifies an interaction spanning all preceding facilities; and
- no collapse means the previous full-port transition was not stable under the current run.

Fast infeasibility proves only the selected restricted state infeasible. It does not prove global
Phase 3 infeasibility or establish routing-only cost for feasible facility states.

## Invariants

- Every subset is an explicit diagnostic restriction and is never used as a production fallback.
- All `2^n` subsets are executed; subset selection is not heuristic.
- No port value is invented. Every equality comes from the validated Phase 2 incumbent.
- No placement, path, corridor, or routing order is added beyond the reported diagnostic state.
- Unknown is never classified as infeasible.

## Failure modes

- more than 63 preceding facilities is rejected because the report mask is a `u64`;
- failure to obtain a validated Phase 2 incumbent aborts the experiment;
- a reference terminal whose facility is absent from the stable mapping is rejected; and
- invalid dimensions, coordinates, assignments, rotations, or worker budgets produce structured
  invalid-input evidence.
