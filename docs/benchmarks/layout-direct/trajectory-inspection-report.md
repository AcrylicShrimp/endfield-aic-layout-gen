# Qualitative trajectory inspection

Status: five post-hoc events inspected directly; not a representative efficacy sample.
Date: 2026-09-07. Source: the predefined update-audit-02 run, not newly optimized visual cases.

The user requested direct qualitative judgment as well as mechanical trajectory checks. The
viewer reconstructs actual pre-action and next-action geometry from stored observations,
preserves original witness field order for exact hashes, and checks the recorded transition
joins the two frames. All ten complete frames pass fresh geometry and original factory
coverage validation. Dashed rejected destinations are overlays, not accepted factory states.

## Observations, interpretation and limits

1. **Seven-facility worker 0, step 1:** the same-scale drawings show long empty stretches and
   straight routes compressed while the recognizable facility arrangement remains. Area falls
   from 2310 to 594 and transport tiles from 182 to 88, with 29 turns unchanged. This looks
   primarily like mechanical empty-strip compaction, not evidence of learned spatial planning.
   The action is reroute plus compaction; the image alone does not prove each phase's effect.
2. **Step 2:** the proposed F02/F03 bodies fit in visibly empty lower space, but the routing
   attempt is rejected with `local-no-path`. Body-legal whitespace is not equivalent to an
   embeddable network. The drawing suggests connections and fixed boundaries matter; it does
   not prove that no alternative route or placement could work.
3. **Step 3:** F01's changed top connection visibly lengthens the route. Geometry stays valid,
   area stays 594, tiles rise from 88 to 91 and turns fall from 29 to 28. Calling this action
   simply wasteful from its immediate score would ignore the next step.
4. **Step 4:** F01 moves left/up; the ensuing compaction removes a row. The factory becomes
   27 by 21 rather than 27 by 22, with 86 tiles and 26 turns. This is a concrete useful spatial
   change. Both steps 3 and 4 receive exactly zero task coefficient because their peer rank
   already equals the terminal rank. That is consistent with the current estimator, but it is
   a specific example of useful temporal structure absent from this sample's learning signal.
5. **Target worker 1, step 9:** F08's connection changes and the network loses one tile, but
   facility positions and the conspicuous larger empty regions remain. This is local wiring
   improvement, not dense global packing. Credit is again zero. No density success follows.

I directly inspected all five light-theme before/after renderings, not just their metric rows.
The viewer passed 30 browser checks: five cases, light/dark, and 360/736/1024 widths, including
event switching, expected facility counts, visible labels and rejected-overlay separation.
The skill-guided same-scale comparison prevents a shrunken after-image from hiding unchanged
spatial structure. Complete plans remain visible; no inconvenient geometry is cropped away.

## Fixed-action counterfactual after visual inspection

The visual sequence suggested step 3 might enable step 4. To test that limited hypothesis,
reset two independently validated complete states immediately before and after the step-3
port edit. Apply the identical recorded step-4 relocation/compaction through the unchanged
executor and routing budget, without policy resampling:

| Starting state | Result of the same next action | Best tuple | Route expansions |
| --- | --- | --- | ---: |
| Skip the port edit | Rejected | `[594,88,29]` | 754 |
| Keep the port edit | Accepted | `[567,86,26]` | 17 |

Both retained outputs remain valid. The prior port edit enables this particular next action
under this executor/configuration; this is stronger than a visual guess. It does not prove
the edit is globally necessary, that larger search would fail without it, or that the learned
policy deliberately planned the sequence. The counterfactual is outside training and changes
no weights or rewards. Its full transitions are in `target/layout-direct-trajectory-01/port-counterfactual.json`.

This observation motivates a controlled credit investigation. It does not authorize replacing
the current objective with immediate-reward penalties: those could suppress the enabling edit.
Compare alternative estimators on frozen behavior rollouts before changing the training method.

## Reproduction

```sh
node docs/benchmarks/layout-direct/trajectory-view.mjs \
  --plan docs/benchmarks/layout-direct/sampling.json --run target/layout-direct-update-audit-02 \
  --output-directory target/layout-direct-trajectory-01
```

Use a fresh output directory. `report.json` records the explicit post-hoc selection, source
hashes and ten independent frame audits. The earlier reconstruction check rejected the
target's different JSON property order; the viewer now preserves the root's order and requires
the exact recorded hash rather than weakening geometry identity. No training behavior changed.
