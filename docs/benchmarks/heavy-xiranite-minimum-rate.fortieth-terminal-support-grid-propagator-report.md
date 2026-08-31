# Heavy Xiranite Minimum-Rate Terminal Support Grid Propagator Report

## Result

The first active two-dimensional grid inference is exact, validates successfully, and reduces the
controlled cumulative Phase 2 search tree. The accepted implementation forces the only remaining
material-carrying arc entering a selected demand cell.

Across two repeated release runs, compared with the accepted broad-lazy possible-graph baseline:

- backtracks fall from 11,511 to 8,806 (`23.5%`);
- conflicts and learned clauses fall from 11,505 to 8,806 (`23.5%`);
- native solver propagations fall from 7,890,787 to 7,219,908 (`8.5%`);
- median first-witness time falls from 4,190 ms to 3,798 ms (`9.4%`); and
- every produced witness passes the independent exact validator.

This is meaningful evidence that a grid-aware custom propagator can remove search which the local
cell constraints leave implicit. It is not yet a solution to the cumulative Phase 2 cliff: the
controlled case still fixes used dimensions, facility placement, port assignment, and terminals,
and still performs 8,806 backtracks.

## Final Repeated Comparison

| Metric | Broad lazy run A | Grid run A | Broad lazy run B | Grid run B |
| --- | ---: | ---: | ---: | ---: |
| First witness | 4,202 ms | 3,919 ms | 4,177 ms | 3,677 ms |
| Branch decisions | 43,226 | 43,545 | 43,226 | 43,545 |
| Backtracks | 11,511 | 8,806 | 11,511 | 8,806 |
| Conflicts | 11,505 | 8,806 | 11,505 | 8,806 |
| Learned clauses | 11,505 | 8,806 | 11,505 | 8,806 |
| Native solver propagations | 7,890,787 | 7,219,908 | 7,890,787 | 7,219,908 |
| Forced grid predicates | 0 | 90 | 0 | 90 |
| Grid forcing conflicts | 0 | 0 | 0 | 0 |
| Maximum grid reason | 0 | 9 | 0 | 9 |
| Validation | passed | passed | passed | passed |

Branch decisions rise by `0.7%` because the exact propagation changes the first-witness search path.
The diagnostic runs stop at the first feasible witness rather than optimizing it, so their
different secondary objective vectors (`144/99/45/12/15` and `144/104/44/12/21`) are not a
solution-quality comparison. The feasible set and configured production objective remain
unchanged.

The active case adds no decision variable. It adds two layer propagators and 8,080 subscription
incidences:

| Metric | Broad lazy | Active grid |
| --- | ---: | ---: |
| Variables | 23,972 | 23,972 |
| Recorded constraints | 82,656 | 82,658 |
| Incidences | 299,544 | 307,624 |
| Model construction | 201 ms | 210 ms |

## Iterations And Findings

### 1. Passive layer analyzer

The passive analyzer found 27 distinct unresolved terminal predicates on nine support arcs without
changing the native search tree. It established that active terminal support was worth testing but
added about 250 ms of observation overhead.

### 2. Layer-wide explanation

The first active implementation used a complete possible-route graph to justify one terminal arc.
It forced 401 predicates, but one explanation contained as many as 1,085 predicates. First-witness
time regressed from 3,881 ms to 4,569 ms. The inference was exact, but its proof was much broader
than the local conclusion.

### 3. Local exact explanation

The rule was reformulated around physical occupancy at the demand cell. A selected demand with no
possible local supply and exactly one possible incoming material arc must use that arc. Its reason
needs only the selected demand, blocked alternative incoming arcs, and blocked supply options on
that same cell. Maximum explanation size fell from 1,085 to nine.

This local rule deliberately does not discard an incoming arc merely because its predecessor is
currently unreachable. A controlled unit test verifies that such an arc remains an alternative,
preventing unsound propagation from a hidden global assumption.

### 4. Targeted active execution

The final implementation stops constructing and traversing the whole possible-route graph in active
mode. It examines only selected demands and their incoming arcs. The same deterministic search-tree
reduction is retained, while both repeated elapsed-time measurements improve.

## Conclusion

The research question has a positive but bounded answer:

> Explicit two-dimensional physical adjacency can provide useful exact propagation, but the useful
> unit is currently a small local cut, not a layer-wide rescan with a global explanation.

The next credible grid experiment is an exact interior cut or articulation inference with compact
reasons. Blind recursive chain forcing should not be added: the evidence shows that explanation
size and wakeup cost can erase the benefit of stronger deductions. Any next propagator should be
accepted only when it reduces the fixed controlled search tree and then survives a free-placement
Phase 2 comparison.

This slice should remain diagnostic-only until that broader comparison. It provides a working
direction and a measured reduction, not sufficient evidence to change the authoritative production
solver path.

## Artifacts

- `heavy-xiranite-phase2-terminal-support-grid-5s/summary.json`: rejected layer-wide reason.
- `heavy-xiranite-phase2-terminal-local-support-grid-5s/summary.json`: local reason trial A.
- `heavy-xiranite-phase2-terminal-local-support-grid-repeat-5s/summary.json`: local reason trial B.
- `heavy-xiranite-phase2-terminal-local-support-grid-targeted-5s/summary.json`: targeted run A.
- `heavy-xiranite-phase2-terminal-local-support-grid-targeted-repeat-5s/summary.json`: targeted run B.

