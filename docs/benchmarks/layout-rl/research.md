# Why terminal credit, and what it does not prove

The motivating distinction is between improving the next state and improving the eventual
returned factory. Local-rewriting policies are an established RL formulation: Chen and Tian's
[NeuRewriter](https://arxiv.org/abs/1810.00337) learns which local region and rewrite to apply.
Our experiment uses a much smaller action-conditioned graph policy with a deterministic
factory repair kernel. It is not a reproduction of their architecture or results.

For a same-root group of `B` trajectories, let `q_i` be trajectory `i`'s final best validated
score tuple. Smaller is better. Our terminal signal is

```text
A_i = sum(j != i, sign_lex(q_j - q_i)) / (B - 1)
loss = -mean(i, t, A_i * log pi(action_i,t | observation_i,t))
```

`sign_lex` compares integer tuples directly, not a weighted scalar subtraction. The same
detached signal credits every action in the trajectory, including a temporary deterioration.
There is no discount. Other trajectories provide an on-policy relative comparison population;
the learner increases the likelihood of trajectories that beat their peers. This is not a
fixed absolute utility, a global convergence theorem, or a guarantee of maximizing every
lower-priority objective in expectation. The incumbent comparator itself is exact lexicographic.

Potential-based shaping has conditions under which it preserves an MDP's optimal policies;
see Ng, Harada and Russell's [policy invariance paper](https://ai.stanford.edu/~ang/papers/shaping-icml99.pdf).
We do not invoke that theorem for our changing peer-relative signal. We avoid a dense geometric
bonus in the terminal learner. The myopic control changes both credit horizon and the rejected
action penalty, so it is not a pure discount-factor ablation. Both methods also have a lossy
observation of the physical state: route summaries are not a full occupancy grid.

The necessary-detour audit is separate from learning. It exhausts the non-increasing reachable
component of the declared deterministic action/router system, including equal-score states,
then finds a valid improving nonmonotone path. Its proof says nothing about omitted rotations,
port changes, movements, routings or larger neighborhoods. The learner receives neither the
proof path nor candidate outcome tables.

CPU parallelism has two independent controls: concurrent Node/Rust environments and intra-op
PyTorch threads. The official [CPU threading notes](https://docs.pytorch.org/docs/2.14/notes/cpu_threading_torchscript_inference.html)
distinguish intra-op and inter-op work and discuss avoiding oversubscription. We set one
inter-op thread and measure 1/2/4/8 intra-op threads before the main run. Four was fastest for
the measured arithmetic, not universally optimal. Python feature-to-tensor conversion remains
serial; more matrix threads cannot directly remove that cost.

Sources checked 2026-09-07. Conclusions about this factory environment come from the local
experiments, not from transferring another paper's performance claims.
