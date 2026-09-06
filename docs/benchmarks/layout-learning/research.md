# Research-informed architecture decision

Reviewed on 2026-09-07, before model implementation. The user requested recent research to
inform architecture, not a benchmark-winning reproduction. This is ordinary targeted research;
all sources below are primary papers/proceedings. Claims are scoped to their original tasks.

## Relevant recent work

| Source | Relevant evidence | Decision for this experiment |
| --- | --- | --- |
| [GOAL, ICLR 2025](https://proceedings.iclr.cc/paper_files/paper/2025/file/826aea2253363fe04e8c4991b2a8869e-Paper-Conference.pdf), sections 3.3–3.4 | A shared graph backbone handles node and edge features, with task-specific input/output projections and no sequence positional encoding. | Use shared geometry-token embeddings and directed graph messages; preserve permutation invariance. Do not implement its generalist adapters or claim its transfer results. |
| [CaDA, ICML 2025](https://proceedings.mlr.press/v267/li25bi.html) | Combines constraint-aware representation with global and local attention for routing variants. | Keep logical connectivity and exact proposed ports alongside global spatial context. Omitting raw flow is not permission to omit capacity-derived connections. |
| [Boosting NCO for Large-Scale VRP, ICLR 2025](https://proceedings.iclr.cc/paper_files/paper/2025/file/264f2e10479c9370972847e96107db7f-Paper-Conference.pdf), section 4 | Uses representative-node cross-attention and locally reconstructed pseudo-labels to reduce expensive all-node interactions and labeling. | Let candidate queries read shared state tokens; compare sampled feedback with full-table supervision. Our small dense graph-message implementation is not a reproduction of its linear-complexity architecture. |
| [Preference Optimization for CO, ICML 2025](https://proceedings.mlr.press/v267/pan25e.html) | Uses objective-based comparisons and local search to obtain training preferences without requiring an external reward model. | Use verified finite-candidate score order as the signal, including ties. Do not claim the ordinal-reward bandit implements the paper's preference loss. |
| [MaskCO, ICLR 2026](https://proceedings.iclr.cc/paper_files/paper/2026/file/d85816bf41e54d9b0847e02249f4fcc6-Paper-Conference.pdf) | Learns from masked parts of reference solutions and uses mask/reconstruction refinement at inference. Reference solutions remain necessary. | A credible next step for multi-facility reconstruction once useful examples exist. Do not pretend masking a few examples creates independent topology diversity. |
| [Learning from Algorithm Feedback, ICLR 2026](https://proceedings.iclr.cc/paper_files/paper/2026/file/a04c50c0bd6937760a3b89ee10010c96-Paper-Conference.pdf) | Trains GNN guidance for SAT from solver feedback rather than expert action labels, using a one-shot decision formulation. | Start with a short-horizon signal test using our real route/validation engine. SAT speedups do not establish factory quality or runtime. |
| [LNS meets Iterative Neural Constraint Heuristics, March 2026 preprint](https://arxiv.org/abs/2603.20801) | Separates neighborhood destruction from neural repair and studies their interactions across constraint tasks. | Explicitly scope what is movable and what remains frozen. Region selection and multi-facility repair are later experiments, not features proven by a one-facility bandit. |
| [Native Adaptive Solution Expansion, ICLR 2026](https://proceedings.iclr.cc/paper_files/paper/2026/hash/3d681cc4487b97c08e5aa67224dd74f2-Abstract-Conference.html) | Contrasts global prediction's constraint conflicts with globally informed incremental expansion. | Global information does not require committing all facility positions before routing. Preserve connected transactional construction. |
| [FrontierCO, ICLR 2026](https://proceedings.iclr.cc/paper_files/paper/2026/hash/c131c8875c7b1133ffdad2b53cb10e91-Abstract-Conference.html) | Highlights the gap between small synthetic evaluations and irregular real-world combinatorial problems. | Keep geometric holdout, out-of-family tests and cheap nonlearned controls separate. This study cannot establish Heavy Xiranite generalization. |
| [Chip Placement with Diffusion, 2024 preprint](https://arxiv.org/html/2407.12282v1), sections 4.3 and 6 | Synthesizes connectivity conditioned on geometry and studies learned placement; full downstream routing is outside the synthetic placement labels. | Useful direct spatial analogue, but we cannot invent game recipes/edges to fit a convenient geometry. Generate poses for catalog-backed graphs and verify actual routes instead. Full-grid diffusion is deferred. |

## Selected architecture

Implement a small **directed graph encoder plus action-to-state cross-attention** in PyTorch.
This replaces the provisional plain-MLP primary model in the initial contract. It is a compact
adaptation of general architectural ideas, not a reimplementation of any paper above.

- State tokens describe every current facility and transport cell in the small chain. Relative
  positions, extents, orientation, token kind and movable/target roles are explicit. Directed
  logical and physical connections define message-passing adjacency. No instance-ID embedding,
  output label, candidate result or numeric recipe rate enters the network.
- Two small directed message-passing blocks encode connectivity. Candidate embeddings encode
  the proposed pose and selected port geometry. Each candidate attends to all valid state
  tokens, so its score can depend on frozen facilities and routes, not only the target.
- A shared head emits one logit per candidate. Softmax defines the sampled policy. Token and
  action order are not positional signals; padding is masked. Width 32 and two attention heads
  are initial experiment settings, not a claim that they suffice for large factories.
- A pooled-MLP control uses the same inputs without graph-message/cross-attention blocks. Its
  purpose is to check whether the extra architecture is useful; parameter counts are reported,
  not described as matched when they differ.
- Train sampled-action REINFORCE, fixed shuffled-reward and zero-reward controls; also train
  a supervised best-action control that consumes all outcomes. Same held-out problems and
  proposal sets are used throughout. These algorithms have different label access, disclosed
  in their reports. Cached outcome tables are not online environment executions.

This first model selects a concrete relative-position/rotation/port edit from sampled actions.
It does not emit an entire n-by-n output, choose a repair region or learn route geometry.
Those are material subsequent contracts. The immediate goal is a trustworthy gradient/signal
and cost experiment, not a claim of learned human-style global reasoning.

## Alternatives deferred

Full-grid CNN output and diffusion can represent coordinated placement, but introduce output
legality, fixed-grid encoding and multi-step sampling before the learning signal is established.
Masked reconstruction needs useful reference solutions and a multi-facility transaction scope.
PPO/GRPO variants and sophisticated preference losses are unnecessary for the initial one-step
bandit; they should be justified by a measured failure rather than added together. GPU access
is useful for later scaling, but its benefit must be measured separately from CPU routing.
