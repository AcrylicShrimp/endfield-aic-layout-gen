# Direct native factory generation

Fresh training and evaluation input construction runs in the existing native
worker pool. `Source` calls recipe throughput, instance wiring and production
construction directly, sharing the already loaded and validated catalogs. It no
longer owns an executable path or catalog paths and never spawns a constructor
CLI. Training input, model, checkpoint and configuration schemas are unchanged.

## Contract and continuity

| Existing benefit | Direct implementation | Verification |
| --- | --- | --- |
| Persistent workers and bounded worker count | Reuse the same pool and logical stream ownership | Actual 128-stream/16-worker run |
| Catalog validation and external data identity | Reuse the shared validated catalogs loaded for this execution | Existing input tests; source identity comparison |
| Same generated problem distribution | Keep draw order, seed namespace, construction options, setup edits and reservation retries | Compare actual generated inputs with retained prior run |
| Algorithmic work limits | Keep construction, placement, routing and generation attempt limits | Existing constructor tests and integrated configuration |
| Elapsed and resource interruption | Scoped cooperative checks inside construction, routing, boundary flood and LP solve | Cancellation tests and unchanged configuration bounds |
| No partial result accepted as a complete factory | A stopped scope returns an error even if an inner candidate returns a value | Mid-search cancellation tests; no manifest on cancellation |
| Durable actual inputs and restart behavior | Keep input/manifest publication, actual slot reuse and incomplete-slot rejection | Fresh generation and cached reload tests; integrated resume |
| Efficient publication and memory | Remove child JSON pipes and duplicate wiring; move final witness out and drop construction history before setup | End-to-end time, output size and RSS |
| Evaluation separation | Keep reserved geometry checks and panel generation through the same source | Existing reservation test and actual generated identity comparison |

## Computation and interruption

A `WorkControl` scope installs one shared deadline and the existing resource
check on the current thread. It restores the previous scope on normal return or
panic. Only explicitly joined boundary analysis workers inherit that control;
unrelated threads remain independent. The first interruption is sticky and the
outer scope rejects its result. Cancellation is never classified as infeasibility
or admitted as a partial factory. Normal calls outside a scope retain unlimited
elapsed execution and their existing work-count limits.

Constructors check between growth steps, candidate attempts and external
connections. A* and boundary flood check every 256 queue entries. Boundary
analysis workers inherit the scope and join before it returns. Workspaces retain
the existing size limits. This replaces OS process termination with cooperative
interruption: responsiveness is determined by a bounded native operation or LP
solver checkpoint, not an OS hard deadline or an exact millisecond guarantee.

The contextual flow model continues to use the existing good_lp model builder
and microlp solver with the same variable order, defaults and optimum requirement.
The now-direct microlp dependency exposes resumable solves already supplied by the
locked microlp 0.6 dependency. During a controlled call the solver yields in
10 ms slices, checks the caller and continues its existing simplex state. It does
not restart optimization or accept an unfinished feasible result. Uncontrolled
calls solve without time slices. `cargo search microlp --limit 1` confirmed 0.6.0
before declaring the broad `0.6` requirement; no dependency version was upgraded.

## Failure and recovery

Source errors retain structured generation diagnostics with code, severity,
stage, entity and message. A cancellation or deadline reports
`aic-generation.interrupted`; a resource guard error propagates promptly out of
the retry loop. An exhausted individual construction may try the next canonical
seed within the existing attempt/setup bounds. Failure metadata now includes the
native construction status and diagnostics instead of child stderr. Actual input
files larger than 16 MiB are rejected before committing their manifest. Existing
committed inputs remain authoritative and are not regenerated from seeds.

The old process timing overlay is historical diagnostic tooling for the pre-cutover
source. It intentionally rejects the new source anchors; it is not a runtime
fallback or a prerequisite for training.
