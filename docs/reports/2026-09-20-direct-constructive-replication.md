# Direct constructive policy replication

## Result

The selected `0.003` learning rate improved held-out stochastic completion in all
five initialization seeds on an expanded eight-problem evaluation panel. The
aggregate result rose from **63/1,280 (4.92%)** before training to **219/1,280
(17.11%)** after 32 updates. This confirms that the probability gain in the
first pilot was not a single-seed accident.

Greedy completion remained unstable in this small fixed-problem experiment.
Seed 101 completed all eight greedy cases only at update 29, seed 202 did so at
updates 31 and 32, and the other three seeds never completed a greedy case.
This established the next research requirement: broaden the logical-goal
training distribution and test whether a larger batched model can turn the
replicated stochastic gain into a stable modal policy.

## Protocol

Every arm used width 32, two graph rounds, a 24-turn horizon, 64 training
episodes per update, four PPO epochs and 32 updates. The four training problems
were unchanged from the first pilot. The evaluation panel grew from two to eight
held-out pairwise problems and each update evaluated 256 stochastic episodes
with the same reset seed. Only the cold model initialization seed changed.

| Seed | Initial | Final | First 8 mean | Last 8 mean | Peak | Training successes |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 101 | 10/256 | 35/256 | 14.13 | 31.50 | 39 | 202/2,048 |
| 202 | 12/256 | 52/256 | 15.13 | 48.25 | 56 | 223/2,048 |
| 303 | 9/256 | 49/256 | 16.50 | 43.00 | 52 | 197/2,048 |
| 404 | 19/256 | 43/256 | 15.00 | 39.50 | 46 | 192/2,048 |
| 505 | 13/256 | 40/256 | 14.13 | 30.25 | 41 | 213/2,048 |

Mean approximate PPO KL across epochs ranged from `4.00e-4` to `9.34e-4`.
Maximum clip fraction ranged from 3.43% to 10.94%, so all five runs made
substantive but bounded updates at the selected rate.

## Evidence and reproduction

The retained inputs are
[`config.json`](../benchmarks/direct-constructive-replication/config.json) and
[`problems.json`](../benchmarks/direct-constructive-replication/problems.json).
Set `initialization_seed` to 101, 202, 303, 404 or 505 for each arm.
The tracked configuration is migrated to training schema v2 and declares a
full-batch `ppo_minibatch_size`; the reported runs used the equivalent implicit
full batch before that complete schema cutover.

```bash
export LIBTORCH=/path/to/torch
export LD_LIBRARY_PATH="$LIBTORCH/lib"
aic learning train-constructive \
  --config docs/benchmarks/direct-constructive-replication/config.json \
  --catalog-root data/game/normalized \
  --problems docs/benchmarks/direct-constructive-replication/problems.json \
  --output target/direct-constructive-replication \
  --device cpu
```

The logical problem-set SHA-256 is
`e5183a424bd82f4900031d55658ace5ad85458ffffc980a4cd7c6beba9b280fd`.
The metrics SHA-256 values for seeds 101, 202, 303, 404 and 505 are,
respectively:

- `c4dc6f04ce7f1e955291b027d8309659475ff7780bf8ae5b1d3d6f0b9e2d677a`
- `a90124570a2cf51253b5e8027e09d41c6629517010769e6c1d6d559a533a1f37`
- `a447170dfcb1f4806d578ff3dbcbc08de48d2b032c4bd2005c166c2a0534b4e0`
- `28d61233c086a6e479062455f671d18dda2602e509b4157fe8ad94c43af276cd`
- `809b3388a822292353809747b967ecb48f4f5d1826ef5b91ecfa1ea7b74082d3`

The replication answers the learning-rate question but also exposes the fixed
problem pool as too narrow: every case still has two 3 by 3 facilities, one belt
and one connection, and the model sees only two distinct rate features. The
recipe-derived experiment therefore replaces this authored pool rather than
adding more repetitions to it.
