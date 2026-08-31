# Exact Used-Dimension Partition Diagnosis

## Status

Accepted experiment contract. This is a research-only diagnostic for the first cumulative SCC phase
of the minimum-rate heavy Xiranite workload. It does not change production solving.

## Question

The current factored shared-layer model cannot find a first witness for network pair `0,1` within
five seconds when the actual used width and height remain solver decisions. Determine whether the
immediate cliff is the circular coupling among:

1. facility and transport geometry;
2. the exact used bounding box;
3. external terminals selected on that bounding-box boundary; and
4. routing that contributes geometry back to the bounding box.

Each diagnostic case fixes `used_width = w` and `used_height = h` by equality. Placement, rotation,
port selection, external-terminal side and cell, routing, flow, topology, capacity, collision, and
all other decisions remain solver-controlled.

## Exactness

For request ceilings `W` and `H`, let `F(w,h)` be the complete original feasible set whose actual
used bounds are exactly `w` by `h`. The unfixed feasible set is:

```text
F = union of F(w,h) for every legal 1 <= w <= W and 1 <= h <= H
```

Fixing one pair is therefore an exact partition, not a preferred-size heuristic. The diagnostic may
execute a prefix of cases to answer the causal question, but the machine-readable candidate list
must retain every dimension pair not rejected by a proven necessary condition.

## Proven Lower Bounds

Candidate enumeration may skip a dimension only when a game rule proves it impossible. The first
implementation uses these inexpensive necessary conditions:

- `w` is at least the largest per-facility minimum rotated width;
- `h` is at least the largest per-facility minimum rotated height;
- `w * h` is at least the sum of all facility footprint areas;
- when at least one selected transport network exists, `w * h` is at least the facility-area sum
  plus one, because some transport geometry must exist and transport cannot occupy a facility cell.

The independent width and height bounds are deliberately weak when different rotations attain
their minima. Weakness adds cases but cannot remove a legal solution. No estimated packing ratio,
preferred aspect ratio, historical witness, or workload-specific size is allowed as a cutoff.

A later experiment may solve an exact relaxed model containing a subset of game rules to derive a
stronger lower bound. Such a relaxation is safe only when every complete layout maps to a feasible
relaxed witness.

## Upper Bounds

A completely validated layout with used area `A` proves that the primary optimum is at most `A`.
After such an incumbent exists, dimensions with area greater than `A` cannot improve the primary
area objective. Other shapes with area `A` may still improve transport-tile or turn objectives.

For this causal diagnosis, the first validated fixed-dimension witness is sufficient to show that
dimension selection participates in the cliff. It is not proof that the witness area is optimal.
Proving the area optimum would additionally require proving every smaller candidate case
infeasible; a timeout or `unknown` result is not such a proof.

## Inputs

- the minimum-rate heavy Xiranite benchmark workload;
- its existing 12 by 12 request ceilings;
- cumulative SCC phase zero only;
- selected shared-network indices `0,1`;
- factored placement/port endpoints and shared boundary terminals;
- feasibility-only search;
- an exact used-width and used-height pair;
- an equal per-case search budget.

## Outputs

The research report schema records:

- request ceilings;
- selected network indices and stable network identifiers;
- proven lower-bound components and their reasons;
- the complete ordered candidate dimension list;
- the subset of cases actually executed;
- exact fixed width, height, and area per case;
- model construction time, search time, first-incumbent time, status, and model metrics;
- validated witness bounds when a case succeeds;
- whether the run stopped after obtaining a diagnostic witness.

Every executed case writes JSON. A successful case also writes the normal self-contained HTML
wireframe. Failure to find a witness remains `unknown` unless Pumpkin proves infeasibility.

## Ordering And Stop Rule

Order candidates by area, then maximum side, then width, then height. This order only controls which
independent exact partitions run first; it does not remove any candidate.

The diagnosis runs cases until either:

1. a validated witness is found, which answers the current causal question; or
2. every candidate has run with the configured equal budget.

The first outcome establishes an upper bound but not optimality. The second outcome means exact
dimension fixation was insufficient under the tested budget, not that the complete problem is
infeasible.

## Interpretation

- If a fixed case finds a witness within five seconds while the unfixed baseline does not, the
  used-bounds, boundary-terminal, and route feedback loop is the immediate first-witness cliff.
- If no fixed case finds a witness, the next diagnostic moves inward to terminal side/cell, port,
  shared routing, flow, topology, and capacity coupling while retaining fixed dimensions.

Success does not by itself prove that Pumpkin's branch order is the cause. It proves only that
partitioning the dimension decision changes search enough to cross the measured cliff.

## Non-Goals

- no production dimension portfolio;
- no heuristic dimension cutoff;
- no placement, port, terminal, or route fixation beyond exact used dimensions;
- no objective or branching changes;
- no claim of optimality from a first feasible witness;
- no fallback layout or router.

## Verification

- unit tests for candidate completeness, ordering, and proven-bound filtering;
- a model test showing the two research equalities appear only in fixed-dimension solving;
- witness validation requires reported bounds to equal the requested fixed dimensions;
- `cargo fmt --all`;
- `cargo check --workspace`;
- `cargo test --workspace`;
- isolated release-mode cases with equal five-second budgets;
- comparison against the existing unfixed feasibility-only baseline.
