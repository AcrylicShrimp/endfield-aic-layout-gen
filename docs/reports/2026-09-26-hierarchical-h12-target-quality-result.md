# Horizon-12 on-policy collection target audit

Status: complete bounded audit of the nine retained horizon-12 on-policy
collections. No additional search or training was run.

## Question and method

Did deeper search make the policy target substantially more concentrated on
tested root actions as training progressed? Read the nine already validated
32-case collection reports from the [on-policy study](2026-09-26-hierarchical-h12-onpolicy-primary-result.md).
Verify each report hash against its completed progress record. For the first
root of each case, measure the number of legal actions, number visited,
target mass on unvisited actions, target mass on the selected action, and
effective target action count divided by legal action count. Also aggregate
all recorded roots. The [script](../../tools/factory-edit-pilot/hierarchical_horizon_target_audit.py)
rejects invalid target distributions and mismatched collection outcomes.
Its SHA-256 for this audit is
`bdca474802bb044d330e8985fe9b00048f5e62869830b120eb3bb2a04528a949`.

## Results

All 288 first roots were present. Each had 16 visited actions out of a mean
2,716 legal actions. Mean first-root target mass on unvisited actions was
**99.08%**, with the nine collection means ranging from 98.99% to 99.13%.
The selected action held a mean **0.54%** of target mass. The target's
effective action count was **98.18%** of the full legal action count.
Across 2,826 recorded roots, the collection-average unvisited mass was
65.00% and selected-action mass was 10.94%.

The [compact summary](2026-09-26-hierarchical-h12-target-quality-summary.json)
contains each collection's input hash, root counts and measurements. First-root
unvisited mass stayed near 99% in all three generations of every stream;
there is no directional concentration trend across these nine collections.

The 12-move search improves completion in the paired evaluation, but it does
not make the first-root policy target strongly focused on actions it tested.
This is an observation about target concentration, not proof that an
unvisited action's target is incorrect or that diffuse targets caused any
specific model failure. A same-search model comparison is needed to learn
whether the policy weights improved despite this target shape.

## Reproduction

From the recorded server workspace with the original collections retained:

```sh
python3 tools/factory-edit-pilot/hierarchical_horizon_target_audit.py \
  --output artifacts/recovery-horizon-onpolicy-v2
```

The raw collections remain only while the ongoing cross-evaluation and
lower-training continuation may need their evidence. Their eventual cleanup
will be recorded with the final on-policy closeout.
