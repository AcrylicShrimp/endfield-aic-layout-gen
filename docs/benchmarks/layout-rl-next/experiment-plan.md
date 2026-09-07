# Breadth and long-horizon experiment

Status: frozen initial design, 2026-09-07; corpus feasibility and pilot run are the first gate.

## Hypotheses

1. Broader product families and constructor variants improve held-out performance.
2. Longer credit horizons improve late-rollout gains at the same training action budget.
3. Improvement can continue beyond the old 128-proposal evaluation allowance.

None implies global optimality or learned superiority before matched controls pass.
The prior corpus has 14 correlated small training roots and no large training factories.
Its best individual diagrams do not establish typical performance.

## Gates and matrix

1. Generate and freshly validate broader roots; retain the prior train/validation/test split.
   Report recipe-family count, node/edge/depth distribution, unique geometry, independent
   family split and constructor attrition. The first stage must add at least four training
   product families and both validation and test families before training starts.
2. Run a bounded one-seed pilot at horizons 16/32/64 with the same 8,192 sampled actions
   per arm (64/32/16 groups of eight). Use the broader training pool in every arm.
   Include an H16 zero-signal control with the same 8,192 attempts.
   Run validation, not test selection, and verify weights change, exact checkpoint reload,
   valid final incumbents, nonzero terminal signal and complete action accounting.
   Pilot validation uses 128/512/1,024 proposals, rollout seed 101, plus retained-initial,
   uniform, greedy and bounded best-first controls. These one-seed validation results
   are diagnostic only, not a model-selection or generalization claim.
3. Full preregistered comparison: narrow-H16; broad-H16; broad-H32; broad-H64. Three seeds
   (7,17,29), 65,536 training actions per arm/seed. Add broad-H16 zero-signal control.
   Match per-root training action exposure across full H16/H32/H64 controls. The small pilot
   reuses uniform random root sampling and therefore has unequal distinct-root coverage;
   its outcomes cannot isolate horizon causally. Fewer, longer episodes also imply fewer
   gradient updates at equal actions, which must remain an explicit training-regime tradeoff.
   Narrow means the original 14 training roots only; both arms use the same frozen corpus
   for validation/evaluation. Model width, optimizer, routing and action set are fixed.
4. Curriculum-order ablation follows the corpus/pilot gate, holding H16 and total actions
   fixed: begin with small factories and introduce medium ones while retaining small-root
   rehearsal. Compare against broad-H16, not only against narrow data. Freeze its schedule
   from corpus structure before viewing full test outcomes; do not call size alone difficulty.
5. Final-checkpoint evaluation: retained initial, uniform nonmonotone, zero-signal, greedy,
   bounded best-first, learned policies. Use 128/512/1,024 proposals and 1/5 second acceptance
   deadlines, with separate hard guards, three rollout seeds (101,202,303), and unchanged
   routing allowance. Group comparisons by original family, not replicated layouts.

Long-rollout curves must identify whether a policy repeatedly improves after step 128 or
merely makes one early lucky move. Report exact paired wins/ties/losses and per-family medians;
never use a free best-of-seeds portfolio. Training, import, audit and residual-probe work are
separate from inference costs. Final learned incumbents will seed a separately budgeted
residual-search probe before claiming that layouts are operationally tight.

This turn starts gates 1 and 2. Full training and residual-probe implementation are gated on
the corpus and pilot evidence, rather than silently expanding compute or changing the action
space. The gate report states what is running, complete, or not yet started.

## Motivation and limits

[Wu et al., Learning Improvement Heuristics for Solving Routing Problems](https://arxiv.org/abs/1912.05784)
studies learned iterative improvement and diversification in routing. It motivates the
improvement-policy framing, not a guarantee for factory placement/routing.
[Portelas et al., Teacher algorithms for curriculum learning of Deep RL](https://proceedings.mlr.press/v100/portelas20a.html)
studies sampling learnable tasks according to learning progress. It motivates separating
learnability from nominal size; this first controlled experiment does not implement ALP-GMM.

Known remaining limitations: fixed rotations/ports, small local moves, lossy route observations,
limited generator topology, and no temporary footprint expansion. More training cannot supply
actions the environment does not offer. Any later operator/observation change needs its own
contract and ablation before attributing gains to curriculum or horizon.
