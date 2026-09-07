# Broader production-load stream contract

Status: next main-path contract, not yet implemented or measured.

The first dynamic training sample reaches only ten facilities. Its target rate is at most one
root recipe's full utilization, and explicit intermediate procurement has probability 0.25.
This is useful small-factory exposure but cannot establish the medium/large release strata.

## Input and sampling

Replace the sampling schema-1 `utilizations` field with schema-2 `target_loads`: positive exact
rational multiples of one selected root recipe's output rate. Loads may exceed one because
they request more facility instances, not illegal utilization of one machine. Continue sampling
all catalog target items/producer choices, original co-products, source-cycle references and
explicit pre-geometry procurement. Keep existing target-family partitions and 128-instance
operational guard. Reject old schema explicitly; do not silently reinterpret utilization.

The initial declared load distribution should include 1/4, 1/2, 1, 2, 4 and 8. These are
experiment distribution inputs, not game balance or a fixed recipe list. Record the chosen
load in every attempt. Do not alter the sampled rate, source graph or external obligations
when construction is hard. Instance wiring continues to determine the actual facility count.

## Acceptance and diagnostics

Run ordinary connected transactional construction for the original generated request. Report
attempts, exact-flow outcomes, instance guards, construction outcomes and elapsed/work budgets
for requested small (2–16), medium (17–64) and large (65–128) solved instance counts. Requests
whose flow is rejected remain an unknown-size source stratum; do not guess a facility count.
All accepted reset states must pass original coverage and independent geometry checks.

Constructor success and learning support are different fields. Measure grid/token requirements
of each complete factory before returning it to the learner. A complete but observation-over-
budget factory remains construction success with a structured learning-ineligible reason; do
not crop geometry, delete networks, increase procurement or crash halfway through a rollout.
The report must count such attrition. The same applies to explicit minimum training size.

First connect the broadened load distribution directly to training with all failed attempts
recorded. Do not build another fixed witness corpus and silently sample from it. If larger
strata still do not enter actual training, investigate the specific construction/representation
failure before claiming breadth or spending a large training budget. No prior witness fallback.

## Evaluation

Keep the small-load credit experiment's evidence separate. New load-distribution evaluation
roots must be shared across initial, uniform and trained controls. Report quality/waste by
actual size, not only a pooled mean dominated by two-facility factories. Target replay is still
the original complete 59-facility request and is not new diverse exposure. Sealed tests remain
closed until dataset feasibility and a statistical decision rule are frozen.
