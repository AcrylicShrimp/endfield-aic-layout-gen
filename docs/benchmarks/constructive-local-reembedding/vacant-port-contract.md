# Failure-directed vacant endpoint ports

Input: complete witness, extracted chain schema 2, runtime catalogs, simultaneous
facility translations, explicit failed chain IDs, candidate limit and combination
budget. Output: deterministic source/target port assignment candidates, projected
endpoint distance, enumerated count and truncation diagnostics. Geometry is not
accepted by this stage.

For a singleton ordinary facility endpoint, enumerate its retained port and all
currently unclaimed runtime ports with matching transport and input/output direction.
Project connection cells after facility translation and reject cells inside final
facility bodies. A junction or ambiguous shared endpoint remains fixed. Cross-product
the two endpoint choices, excluding the unchanged pair, and rank by projected endpoint
distance with stable tie-breaks. Interleave failed chains under the explicit budget.
Preserve terminal identity, commodity and rate; do not infer a new logical allocation.

This neighborhood does not swap occupied ports or synthesize component positions.
Existing paired-slot controls remain distinct. Only the shared reembedding engine
checks final port ownership, component occupancy, paths and original graph validation.
Its movement screen still precedes port changes; a rejected candidate is not proof
against a different joint move/port/component assignment.

The priority replay exposes `--port-policy vacant` for single- and dual-endpoint
changes of the initially failing chains, with near/far orders. Replay candidates are
explicitly unvalidated until checked by the independent full-factory gate. No facility
labels, recipe IDs or chosen game port indices are embedded in the proposal engine.
Tests cover dynamic catalogs, vacant-only compatibility, simultaneous endpoint choices,
distance order, immutable inputs, budgets and the unsupported ambiguous-endpoint case.

The common experiment runner also accepts `--port-policy vacant` in move/joint modes.
After an ordinary routing failure, it retries up to eight distance-ranked assignment
combinations for that first failing chain in the same parent geometry and route order.
Retries do not recurse. Each retry is its own recorded stage referring to the failed
parent stage. Record domain size, examined combinations, shortlist limit, route result
and independently validated score. An unchanged or already successful route triggers
no speculative port repair. All work is charged to the normal batch timing.
