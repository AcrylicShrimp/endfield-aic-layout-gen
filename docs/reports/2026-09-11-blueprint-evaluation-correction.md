# Restore historical evaluation and restart fresh training

The previous duration run used six original/overlap cases and a thirty-minute
trigger. The corrected duration-v2 contract evaluates the exact historical
32 factories at iteration zero, every four batches, and termination. One batch
is eight 32-action chunks (256 actions); episodes remain 64 actions. The training
starts remain original/overlap. This restores evaluation inputs and metric names;
it does not change the blueprint policy into the older model architecture.

The previous trainer stopped at update 1778 / 455,168 actions after 2,643.185
seconds. Its tail audit passed and its observer acknowledged all 1,781 events.
Both old launchd registrations were removed. Its protocol, checkpoints, optimizer
states, experience, sparse six-case evaluations and frozen runtime are preserved
under `artifacts/factory-edit-pilot/blueprint-success-64-12h-20260911/`.

## Restored input and metric contract

`export_blueprint_evaluation_cohort.py` exported the original request, initial
and original files for seeds 232700000 through 232700031, preserving their exact
bytes, initial scores, order, catalogs and action seeds. The source protocol is
SHA-256 `fb6061b7513d3d558eb6beb21b0ab1a1346978d8c42d24023e820cb2b31b7531`.
The retained input is
`artifacts/factory-edit-pilot/blueprint-evaluation-cohort-20260911/cohort.json`.
Prepare accepts `--evaluation-cohort` for an explicit external retained manifest,
including when executing from a frozen checkout. Every execution authenticates it.
The [port-turn admission correction](2026-09-11-blueprint-port-turn-correction.md)
allows these unchanged valid layouts through the blueprint session validator.

Reuse `evaluation/all_admitted/by_{actions,batches,seconds}/area_reduction_percent`
and `delta_from_initial_pp` with the original axis keys. Match all 32 baseline
identities and inputs; report coverage, paired cases and rejected-action fraction.
Common root PPO keys and completed-episode training metrics replace the custom
final-epoch chart names. Missing episode measurements remain absent. The shared
metric implementation serves tracking and evidence checks.

Fresh jobs initialize seed-17 weights, both Adam states and the replay buffer.
No previous trained state is imported. Iteration zero is an actual evaluation;
its score need not be zero. The observer must export it before the first update.
Normal online jobs start and supervise their own observer. A preflight prevents
v1 or completed-job invocations from overwriting historical completion evidence.
V1 execution and audits remain with their preserved frozen runner.

The existing dedicated W&B view is updated using canonical keys and a filter for
the fresh protocol. Personal workspace panels are not rewritten: their existing
canonical keys can display the fresh run when selected. Panel definitions and
server history are read back; this is not a browser-rendering check.

## Verification before launch

- Rust workspace check and tests passed (564 passed, five ignored); formatting
  and release native build passed.
- Full Python suite: 406 tests, seven skipped; the subsequent preflight regression
  also passed with the other two tracking-job tests.
- Development readiness paused after one nonterminal chunk and resumed to six
  updates, validating checkpoint continuation and terminal boundaries.
- Final online readiness completed six updates, 1,536 training actions and
  evaluations at 0/4/6, each on 32 original factories. Full replay verifies all
  six learner updates, model/Adam/RNG/buffer state and evaluation witnesses.
- W&B server history confirms iteration-zero actions/batches are zero and
  paired change is zero. Area reductions at 0/4/6 are respectively
  0.8165631064%, 0.3986135990%, and 0.15625%. These six updates are readiness
  evidence, not a claim of learning effectiveness.

Final readiness artifacts are retained under
`artifacts/factory-edit-pilot/blueprint-evaluation-correction-final-20260911/`.
The automatic tail audit remains `audit.json`; the additional full replay is
`full-audit.json`. Runtime and input hashes bind both to the final source.
The twelve-hour relaunch is recorded separately once its initial online
measurements and detached process are verified.
