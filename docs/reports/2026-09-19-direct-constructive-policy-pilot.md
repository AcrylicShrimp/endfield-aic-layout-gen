# Direct constructive attention-policy pilot

## Result

The first router-free learned construction path is executable end to end. A cold
attention policy starts from an empty native session, chooses facility poses and
every route segment, receives reward from certified connection-completion delta,
and updates through PPO. No complete layout dataset, A* call, repair action or
hidden route generator participates in collection or evaluation.

The bounded two-block pilot produced valid layouts before training and showed a
learning signal at the larger tested step size. With learning rate `0.003`, the
fixed-seed held-out stochastic evaluation improved from **7/128 (5.47%)** before
training to **25/128 (19.53%)** after 32 updates. The matching `0.0003` arm ended
at **5/128 (3.91%)**. This one-seed screen supports retaining `0.003` for the next
bounded experiment. It does not establish robust generalization or solve the
larger factory domain.

## Implemented path

The native environment owns immutable logical blocks and connections, optional
poses, completed routes, one active explicit route, exact occupancy and a
persistent last-command focus. The learned command domain is conditional:

1. command kind;
2. block then complete pose, connection then source and target ports, or
   direction then extension length; and
3. a native command bound to the exact observed revision.

Only immediately locally legal choices are included. A stale choice is rejected.
The executor remains the authority for every state transition and independently
validates a complete layout. The route domain now removes port choices whose
connection cell is covered by a facility body or same-layer route; it does not
run a reachability search.

The model retains the small typed graph message-passing shape. It separately
encodes every occupied body/path cell and the active endpoint and target. A query
made from the persistent focus entity and geometry cross-attends to the combined
entity and exact-cell memory. The context scores each conditional choice and feeds
a pooled critic. The executed command likelihood is the sum of selected factor
log probabilities.

Collection batches all active sessions at each turn. Reward is:

```text
1.0 * connection-completion delta
+ 5.0 once for independently validated completion
- 0.05 for a rejected command
- 1.0 for a terminal unfinished episode
```

The CLI reads separate logical training and evaluation sets and rejects exact
problem reuse across the split. Evaluation has a deterministic greedy view and
a 128-episode stochastic view using a reset seed that never consumes the training
sampler. The final learner snapshot uses a constructive-only model/Adam/RNG namespace.

## Bounded experiment

Both arms used initialization seed 101, width 32, two graph rounds, 24 native
turns, 64 training episodes per update, four PPO epochs, 32 updates, and the same
four training and two held-out logical problems. All facilities in this first
screen have 3 by 3 footprints and belt endpoints. The action canvas is 8 by 8;
straight extension lengths are 1, 2 and 3. Model size is 24,610 parameters.

| Learning rate | Initial held-out stochastic | Final held-out stochastic | Training successes, all 2,048 | First 8 updates | Last 8 updates | Mean PPO KL across epochs | Maximum clip fraction |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 0.0003 | 7/128 | 5/128 | 100 | 22 | 24 | `5.70e-6` | 0.0% |
| 0.003 | 7/128 | 25/128 | 202 | 26 | 79 | `4.65e-4` | 3.43% |

The `0.003` held-out stochastic success count reached its maximum 25/128 at
update 32. Its mean over the first eight post-update evaluations was 7.88; the
last eight averaged 18.75. The greedy two-case view succeeded only at update 29
and returned to zero afterward. This shows that probability mass
moved toward successful trajectories without making the modal decoder stable.

The `0.0003` arm never clipped and its mean KL across update epochs was about
`5.70e-6`; its held-out and training results did not improve. The larger step size
had mean KL about `4.65e-4`, mean clip fraction `0.17%`, and maximum observed KL
`0.00434`, remaining below the established `0.02` reference target.

The stochastic evaluations reset the same generator seed, but autoregressive
branches can consume different draw counts after policies choose different
prefixes. They are a reproducible fixed protocol, not perfectly paired individual
trajectories.

## Reproduction and evidence

The retained source inputs are
[`config.json`](../benchmarks/direct-constructive-pilot/config.json) and
[`problems.json`](../benchmarks/direct-constructive-pilot/problems.json). The
registered config now contains the selected `0.003` learning rate. It has been
migrated to training schema v2 with an explicit full-batch
`ppo_minibatch_size`; this preserves the pilot update while rejecting the old
implicit v1 setting.

```bash
export LIBTORCH=/path/to/torch
export DYLD_LIBRARY_PATH="$LIBTORCH/lib"
cargo run -p aic-cli --bin aic-cli --features learning -- \
  learning train-constructive \
  --config docs/benchmarks/direct-constructive-pilot/config.json \
  --catalog-root data/game/normalized \
  --problems docs/benchmarks/direct-constructive-pilot/problems.json \
  --output target/direct-constructive-pilot \
  --device cpu
```

The `0.0003` and `0.003` runs each completed in about 24 seconds including 33
held-out evaluations. Their compact `metrics.jsonl` SHA-256 values were
`5240380f29bed041e7ea65272f976dd7d650ed97827693dfa7f6332858361510`
and `566544aff58007f683df6964c45037bc40ce3748a225dba2c545fe331e864ea4`.
Both used logical-problem SHA-256
`a135404d24969d417fec47663748690ca3db0e3a66d4903e0c609a1ab33b5dcd`
and catalog identity
`d64fc5916e2d57c3052dd8f7eeefd85b4211ad222553c450391b601d64a51cca`.

## Next decision

Do not scale to larger factories yet. The next controlled question is whether
the policy can make the greedy decoder stable on this pairwise domain. Replicate
the selected rate with additional initialization seeds and a larger held-out
panel. If the probability gain replicates but greedy completion remains unstable,
test a completion-trajectory replay or supervised successful-prefix auxiliary
loss while keeping the native environment, reward and action domain fixed. Add
multi-connection logical problems only after that result.
