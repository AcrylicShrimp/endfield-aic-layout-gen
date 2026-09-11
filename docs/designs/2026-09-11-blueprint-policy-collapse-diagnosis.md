# Diagnose early blueprint policy deterioration

Status: completed offline analysis. The active twelve-hour run is unchanged.

## Contract

Input: the corrected duration-v2 run's authenticated protocol, a fixed completed
update cutoff, recorded chunks/evaluation reports, and retained learner/Adam/RNG
checkpoints. Snapshot the cutoff before analysis and authenticate every input as it is read;
retain the complete input hash manifest under ignored artifacts. Do not infer stability from mutable live summaries.

Output: structured evidence for (1) observed training/evaluation behavior,
(2) policy probabilities on exactly the same initial states across checkpoints,
(3) reward and GAE credit by selected action kind and chunk terminal status,
(4) exact single-update replays and isolated loss-term counterfactuals, and
(5) a human report separating observations, causal local evidence and hypotheses.

The fixed probes are two recorded initial training states (original and overlap)
and the first recorded observation of each of the 32 iteration-zero evaluation
cases. Counterfactual updates start from the same recorded model, both Adam
states and shuffle RNG, use exactly the original frozen samples and critic targets,
and do not collect replacement trajectories. Reference replay must match the
recorded model and optimizer hashes. Removing a loss component or advantage
normalization is diagnostic only; never publish those weights to the trainer.

Registered ablations: omit self-imitation; detach critic input from the shared
encoder; zero the PPO surrogate's advantage while retaining other losses; omit
entropy regularization; and center GAE without dividing by its standard deviation.
Measure changes in facility-pose probability on fixed probes. Also measure the
first actual minibatch's separate actor-gradient components and their alignment
with increasing pose probability. Adam ablations and first-order gradients answer
different questions; neither establishes long-horizon improvement.

Check whether movement decreases before replay begins; whether candidate
availability or changed visited states explain the decline; how often successful
reward exists; whether zero-reward chunks still supply strong normalized credit;
and whether the training starts cover the evaluation factories. Investigate
follow-up explanations only where these measurements leave a specific ambiguity.

Failure modes: source/hash drift, missing retained input, failed exact replay,
nonfinite gradient/probability, or a fixed probe changing during measurement must
stop the diagnostic with the offending input/update identified. Successful
completion requires a concise report, reproduction command and compact provenance.
Generated reports, figures and copied checkpoints stay outside Git. This is an
analysis of the approved blueprint-edit experiment and changes no native or
production-constructor semantics.

The user additionally proposed explicit failure penalties as the cause. Check all
recorded rewards and rejected outcomes, distinguish reward from normalized GAE,
and verify whether the older failure-cost module is called by this runner.
