# Touching reciprocal strip experiment

Input: normalized graph, runtime catalogs and a discovered productive reciprocal SCC.
Extract its original cyclic members; all outgoing graph edges remain exact pending margins.
No fake external terminals, changed utilization or deleted outgoing requirements.

Enumerate every member order, allowed catalog rotation and horizontal/vertical touching
strip for cores of at most three members. This bound is an explicit research neighborhood,
not a game invariant. No planter, seed collector, recipe or item name is a selection rule.
Five-member cores return strip-member-bound diagnostics until a larger strategy exists.

Compare retained allocations with compatible-commodity reallocations over the complete
producer/consumer bank, including downstream consumers outside the strip. Enumerate
source/consumer orderings for banks of at most three per side and greedily exhaust exact
supplies/demands. Preserve each original per-facility per-item input/output total, not
arbitrary producer-consumer fragments. Persist replaced edge IDs and new edges; apply the
same graph delta when integrating downstream, never mix old and new cuts. This is a bounded
allocation portfolio, not a globally optimal transportation solver. A retained three-member
region need not remain one SCC after compatible reallocation, but its inputs stay closed.

For every resulting internal edge, enumerate compatible unused output/input catalog ports
whose connection cells are adjacent and outside every body. Each path uses exactly two
transport cells. Backtrack port choices, with a 10,000-node explicit assignment budget.
Keep the first feasible port assignment for each enumerated layout; this is not exhaustive
port-assignment optimization. No shared transport cell or implicit splitter/merger is allowed. Exact original rates must
fit the loaded line capacity. The geometry either covers every internal edge or is rejected.

Independently validate candidates and exact pending/recipe margins. Select by actual area,
transport tiles and turns. Selection is only within touching-strip/two-cell geometry, not
a proof of global minimum area or integration into the complete factory. Preserve the
complete factory incumbent while this new local candidate family is investigated.

Output: structured candidate counts, semantic/geometry rejections, exhaustive-neighborhood
completion, validator reports, covered/pending scope and native rendering. Test enumeration,
port uniqueness, immutability, capacity, renaming invariance and pending obligations.
