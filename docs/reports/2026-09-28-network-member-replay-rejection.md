# Local network member replay rejection

## Question and method

Can a live lower network page widen a Gumbel root without aborting the whole search when one chosen member route fails native replay? A local CPU fixture used `docs/benchmarks/hierarchical-network/panel.json` with its full prefix, a seeded two-head guidance model, 16 Gumbel simulations, one root move, a one-step horizon, and two allowed network variants. The candidate bounds were 20 by 20 and the work allowance was the existing default except for `max_variants = 2`. No server was used.

## Result

The root search returned an error from `NetworkPager::finish_member` rather than retaining the previous completed network or trying another member rank:

```text
network member replay: Invalid(Diagnostic { code: "constructive.topology", stage: "direct-construction", message: "A physical arm carries opposing directions", entity: "\"7,0\"" })
```

The local output is retained at `artifacts/plan06-live-gumbel-other.log`. The same root-identity work passed separate controlled append tests for initial and late network witnesses, a real live late route test, and a real live late repair test. The failure therefore occurs in the network candidate producer before its late witness reaches root selection. It does not prove the whole factory infeasible.

## Ownership and next check

Plan [`.10` candidate publication](../plans/2026-09-27.10-resumable-candidate-publication.md) already requires a recoverable native rejection to consume the selected rank and try the next untried rank while preserving earlier complete candidates. Its implementation review must reproduce this case, verify that the invalid member cannot abort the search, and rerun the live Gumbel network lineage check. Plan [`.05` node widening](../plans/2026-09-27.05-node-lower-candidate-widening.md) must rerun the same integrated lineage check after the producer cutover. No smaller variant cap is proposed as a remedy.
