# External Connector Port-Domain Cliff Diagnosis

## Status

Approved diagnostic-only follow-up to the external connector subset report. This experiment may
restrict legal port choices to locate a search cliff, but it does not change production solver
semantics and none of its restricted witnesses may be used as a production layout or optimization
baseline.

## Falsifiable Question

For one Heavy Xiranite phase-zero Xiranite Powder belt input, does the five-second no-incumbent
cliff first appear when the compatible port domain spans two facility sides, or can same-side port
multiplicity reproduce it without the orthogonal port?

If a one-port case also fails, the port-domain explanation is rejected and the local target becomes
the connector-template and dynamic-used-bound encoding itself.

## Controlled Formulation

Every case starts from phase-zero route index 1 and uses
`joint-shared-transport-layer-external-connectors-v1`. Facility placement, facility rotation, the
selected port within the case domain, all three connector templates, used geometry, and every
objective stage remain solver decisions. No coordinate, rotation, template, or objective value is
fixed.

The only matrix axis is the set of compatible input-belt port IDs retained in the research model:

| Case | Port IDs | Unrotated geometry | Classification |
| --- | --- | --- | --- |
| `one-collinear` | `input-belt-2` | one south-side interior port | `diagnostic_only` |
| `two-collinear` | `input-belt-1`, `input-belt-2` | two south-side ports | `diagnostic_only` |
| `four-collinear` | `input-belt-0` through `input-belt-3` | all four south-side ports | `diagnostic_only` |
| `one-orthogonal` | `input-belt-4` | one east-side port | `diagnostic_only` |
| `mixed-two` | `input-belt-2`, `input-belt-4` | one south and one east port | `diagnostic_only` |
| `faithful-all` | `input-belt-0` through `input-belt-4` | four south and one east port | faithful baseline |

Facility rotation rotates the retained port geometry normally. The labels describe the catalog's
unrotated frame only.

The research model is rebuilt after filtering the selected edge's facility endpoint. Excluded port
definitions must not remain in endpoint domains, geometry literals, connector options, collision
state, or objective auxiliaries. A case with no retained compatible port is invalid input rather
than infeasible.

## Budget And Outputs

- Hard ceiling: 12 by 12 for this experiment only.
- Search budget: 5,000 ms per case.
- Execution: optimized release binary, one isolated process per case.
- Memory: macOS `/usr/bin/time -l` maximum resident set size.
- Outputs: stable case metadata, JSON exact-model report, self-contained HTML for success or
  failure, and external time/RSS record.

The report compares endpoint domains, connector variables, constraints, terms, factor incidences,
construction time, search time, first incumbent, primary proof, final objective, validation, and
RSS as deltas from the faithful baseline and between adjacent same-side domain sizes.

## Decision Rules

- If `one-collinear`, `one-orthogonal`, and `four-collinear` solve but `mixed-two` fails, the local
  target is the cross-side port/template disjunction.
- If `four-collinear` fails while `one-collinear` and `two-collinear` solve, the local target is
  same-side port multiplicity or port-choice symmetry.
- If both single-port cases fail, the local target is the three-template/dynamic-bound connector
  encoding independent of port choice.
- If every restricted case solves but only `faithful-all` fails, the interaction between the
  orthogonal choice and the larger collinear domain is the local target.

These rules locate an improvement target; they do not authorize a port restriction as the
improvement. Any subsequent implementation must preserve all five legal ports and the complete
objective.

## Stopping Point

Commit the controlled matrix, report the narrowest evidence-backed exact-reformulation target, and
pause. Do not implement a new encoding, symmetry breaker, search strategy, or heuristic in this
checkpoint.
