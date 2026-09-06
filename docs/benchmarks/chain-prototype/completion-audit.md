# Chain-reasoning research completion audit

Date: 2026-09-07. Status: verified against current source, tests and CLI artifacts, not merely
historical milestone status. The original goal is the staged separate prototype and research
conclusion for three named recipe families; it is not universal construction or a production
replacement. The findings include unsuccessful controls without redefining them as success.

## Requirement-by-requirement evidence

| Original requirement | Inspected evidence and result |
| --- | --- |
| Separate staged prototype; preserve production and unrelated work | `docs/benchmarks/chain-prototype/` contains explicit experimental Node CLI entry points. `git diff --name-only 0ad02a7..HEAD` through M4 contains only prototype/reference/tracker files; M5 likewise adds comparison/report files only. No Rust, Cargo, production CLI or exact solver change. The four pre-existing dirty paths remain unstaged and unchanged by these commits. |
| Contracts before substantial stages | `README.md`, `chain-contract.md`, `group-contract.md`, `factory-contract.md`, `replay-contract.md`, `comparison-contract.md` define DTOs, supported topology, transactional invariants, failures, reports and verification gates. The tracker records each contract and staged implementation. |
| Independently checkable, rate-correct battery/citron/iron references | Three schema-2 JSON references and `reference-validation.test.mjs` use explicit utilization and lane rates. Current release-probe tests verify exact pending margins, reject missing internal routes and overallocated flow, and preserve the original battery witness. M0 `target/chain-reference-v2-*-verified/` reports accept internal geometry only; omitted external margins are not factory success. |
| Different facility inventories remain separate | `iron-nine-factory.json` preserves the observed nine facilities; `iron-eight-factory.json` uses the separate eight-facility graph. Original-request coverage rejects substitutions; comparison semantic signatures and per-case summaries prevent pooling them. |
| Flexible straight/bent/rotated/sliding/spaced connected chains | `geometry.mjs` and `factory-geometry.mjs` derive relative poses from catalog ports and existing geometry, including body contact and 180-degree folds. Chain/group/factory tests exercise each family, mixed approaches, rotations, ID/array invariance and failure retention. No screenshot coordinates are generation inputs. |
| Spatial port assignment at branches and merges | `rankedPorts`, attachment proposals and closing-pair geometric distance/position ordering use real rotated port geometry. Factory tests construct the multi-input battery and thickener graphs with independent directional/capacity validation. It is heuristic ordering, not a claim of globally best simultaneous port assignment. |
| Immediate routing and valid transactional growth | `factory-kernel.mjs` routes attachment and induced closing edges, checks pending access and invokes the independent validator before recursive growth. It yields only completed groups. Chain/group/factory tests reject failed or late validation, omitted induced routes and partial-cycle commits, and inspect every accepted private step. There is no all-facilities-first routing pass. |
| Seed cycle and branch growth | M2 constructs the citron seed split and reciprocal return privately with exact rates, then commits the closed group. M3 extends this to both iron inventories. Missing-return and rejected-cycle tests prove closure is a gate, not just a trace label. |
| Multi-input composition and explicit full boundaries | `prepareFactory`, `realizeBoundary` and `checkFactoryState` preserve exact input/output obligations, split external lanes by catalog capacity, route each lane privately and require complete recipe coverage. Rust checks physical perimeter sides; original-request checks identities, inventory, utilization and rates. Missing export, wrong side, altered utilization and wrong inventory tests reject. |
| Evaluate remaining-body fit plus actual completion | `remaining-fit.mjs` uses a frozen occupied mask and missing catalog shapes; fragmented-hole tests distinguish equal empty area from usable shape. Temporary poses never enter the real witness. `replay.mjs` actually grows and routes continuation including boundaries, recording trial ancestry. Real iron internal-area reversal and complete-score improvement tests pass. Fit's measured failures remain explicit. |
| Bounded connected rollback; preserve unrelated valid regions | `replay-preservation.mjs` enforces connected whole-cycle suffix ownership, facility limit and exact frozen-prefix geometry/ports/routes modulo translation. `replay.mjs` fixes the episode floor and charges all trials to one Work account. Real iron G2/F2 replay preserves the plant cycle and first iron chain; rejected, worse and late candidates cannot replace the complete incumbent. |
| External data, no power, no production W/H limit or global infeasibility claim | CLIs load and hash five catalogs each execution. Strict input fields reject absolute poses/canvas sizes. Boundary frames are per-attempt routing neighborhoods and score uses only occupied geometry. Unsupported topology/transport, route/proposal/validation failure and work/window exhaustion are structured outcomes, not proofs. Power and bootstrap are explicitly out of scope. |
| Complete independently validated factories for all three families | The M5 release matrix contains I/P/8 successes for battery, citron, iron-nine and iron-eight, each separately verified against the original request. Sizes are 11×20, 18×9, 17×20 and 14×20. All 76 successful matrix rows have independent accepted audit artifacts and no pending recipe margins. |
| Reproducible CLI artifacts and decision traces | M1–M4 reports contain executable stage commands. M5's comparison command regenerates all 192 rows, immutable inputs/reports, decisions, prefix validations, selected witnesses and post-run audits. Source and witness hashes are frozen and rechecked. Initial/release runs reproduce all deterministic outcomes and work. CLI tests exercise success, stopped and invalid/reused-output cases. |
| Matched inventories and equal budgets; area/cells/turns | `comparison.mjs` verifies original semantic signatures and identical assigned vectors within every case/checkpoint. It reconciles proposals with decisions, expansions with A*/access traces, fit checks with pools and validation counts with artifacts. Common scoring includes port arms; incomplete rows cannot enter quality tables. No free portfolio or cross-inventory best score is claimed. |
| Diagnose stalls and test small hypotheses before adding complexity | M2 documents the zero-gap endpoint bug; M3 tests aligned-first/access/clearance controls including an unsuccessful ablation; M4 documents duplicate-family work and body-fit failure; M5 separates enumeration, ranking and trial-count factors under unchanged total caps. Reports distinguish pattern/proposal/port/route/evaluation/closure/budget causes. No unresolved user intent required another example. |
| Tests, report and focused commit per milestone | M0 `f1a5b2b`, M1 `1466469`, M2 `c1b4bd4`, M3 `18deaba`, M4 `a0b305c`; each includes its report and tests. M5's focused commit contains this audit, `comparison-report.md`, contract, runner, plan, explicit uniform policy and nine comparison tests. The exact M5 hash is recorded by Git and the final handoff. |
| Final findings and research recommendation; no production replacement | `comparison-report.md` reports both positive and negative equal-budget evidence, reproduction/generalization limits and a specifically scoped next fan-in-window experiment. It recommends retaining experimental continuation, not promoting greedy body-fit or changing production without approval. |

## Acceptance scope and verification limits

The unchanged Rust probe owns physical geometry, directional ports, capacities and recipe
margins, not the original request. Its own `factory_validated: false` is expected. The separate
original-request coverage gate combines those results with exact graph/external coverage and
sets the experimental factory acceptance. Neither gate proves in-game cycle startup, power,
warehouse integration or global optimality.

Full-factory support is bounded to the documented single-transport component-tree graph class,
with reciprocal two-node cycles and independent capacity lanes inside compatible item/kind
network DTOs. Shared physical junctions, larger SCCs and mixed-kind full factories are not
silently approximated. These are reported generalization limits, not missing instances from
the requested battery/citron/enriched-iron experiment.

The accepted `docs/designs/2026-09-02.00-constructive-layout-planner.md` and AGENTS policy were
checked against the final slice: routed connected transactions, isolated estimates, protected
valid incumbents, external-data loading, explicit boundary obligations and failure scope hold.
This experimental subset does not claim to implement every future production strategy.

Current final verification: 355 Node tests pass without skips; workspace check passes; 524
Rust tests pass with two existing ignored tests. All 192 release comparison rows completed
execution and all 76 complete rows passed standalone audits. Deterministic results repeat in
the initial run. The four uniform-policy witness hashes match M4's inspected layouts; all four
existing native-browser reports show `all_passed: true` (24 viewport/theme cases total).
No new selected geometry or renderer was introduced in M5.

The generated `target/` artifacts are local reproducible outputs, not committed fixtures.
Contracts, inputs, algorithms, regression tests and human-readable findings are committed.
The goal completion gate is satisfied by the requested research artifacts and measured
factories, not by assuming that every heuristic control or every future graph must succeed.
