# Expanded-action sequential learning, schema 1

Status: actual online training experiment, not a production readiness claim.

This is an enumerated-candidate selection policy, not a direct action proposer. Deterministic
code builds a finite list of fully specified mutations; the network predicts a categorical
distribution over that list. It cannot propose an unlisted coordinate, group or port. A later
factorized proposer must be a separately named experiment, not a relabeling of this model.

Input: explicit dataset/catalog/validator provenance, non-test training and development
validation roots, a mandatory strong Heavy Xiranite training target, action-family bounds,
and rollout/proposal/route/deadline limits. The sealed test roots are never exposed to the
worker. Heavy Xiranite results are target-training results, not out-of-family generalization.

The policy receives directed facility/network/component graph tokens and pre-action geometry
features. Actions include bounded connected-group translations, catalog rotations, compatible
port reassignment, dependency-contact XY moves and standalone strip compaction. Optional
move-then-compaction variants are explicitly distinguished in action features and logs.
The finite domains are the measured heuristic domains, not all legal factory changes.

Screen facility body/bounds conflicts before offering actions; mask occupied/incompatible
port destinations using current geometry. Do not route or validate candidates to build
inference features. Charge raw generation and static masking CPU work. Exclude already
attempted identical state/action pairs, with every original attempt still charged. Report
domain exhaustion instead of inventing additional actions or presenting it as infeasibility.

Every accepted transition is an independently validated complete factory preserving the
original inventory, production rates, requirements and external obligations. A valid worse
state may become the private current state. The best score/witness is separate and only
improves lexicographically (area, transport tiles, directed turns). Multi-phase actions
validate intermediate and final states separately. Failed or late work cannot lose the
best timely validated layout. Known coverage failures reject candidates; infrastructure
errors abort. Route pipe first; cap shared expansions; never invoke the global exact solver.

Use the existing small graph-policy implementation as a controlled starting architecture,
with a versioned expanded feature/action contract. Train on sampled multi-step trajectories
with terminal lexicographic peer-comparison returns. No scalar weighted score replaces
incumbent selection. Track actual optimizer steps, informative signals, gradient norms,
weight changes, operator use, valid deterioration and repeat/invalid action waste.
Entropy regularization, if enabled, is explicit and must not be confused with task learning.

Save the exact initial model as the untrained control and the updated model with hashes,
configuration, source provenance and training trajectories. Evaluate both from identical
roots at 128 and 1,024 selected-action budgets with fixed rollout seeds; sample policies
use matched seed schedules. Freshly audit every retained output. Include zero/unchanged/
revisit waste and preprocessing cost, not only final scores. This pilot does not establish
the final 300-case generalization, 1% Heavy Xiranite density, or production CLI gates.

The first run trains 32 optimizer groups of four 64-action trajectories, with the target
every fourth group. It is a short pilot, not a deeply trained long-horizon policy. The 1,024
budget belongs to evaluation. Report actual proposals and deadline/domain termination; a
nominal long evaluation budget does not prove that every episode executed that many actions.
Evaluation resets the RNG per model/horizon/seed, with a common root/batch order. It does not
claim independently coupled random numbers per root when earlier episodes terminate early.
