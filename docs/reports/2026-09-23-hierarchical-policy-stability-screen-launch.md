# Hierarchical policy stability factorial: launch record

**Status:** running on the dedicated training server. The four-arm,
eight-generation screen started on 2026-09-23. This record reports launch and
readiness only; no treatment verdict is available yet.

The [fixed design](../designs/2026-09-23-hierarchical-policy-stability-factorial.md)
crosses successful-root replay (off/on) with failed-root policy targets
(on/off). A, B, C and D use the same generation-87 initial weights, 32 fresh
two-facility training problems per generation, 256 outer states and 3,200
updates per generation, frozen lower weights, eager search, and the same
disjoint 64-case train-source diagnostic panel at generations 1, 3, 5 and 7.
The untouched historical validation panel is excluded. The common-start
diagnostic baseline is [27/64](2026-09-23-hierarchical-policy-stability-baseline.md).

The [A/D smoke](2026-09-23-hierarchical-policy-stability-smoke.md) verified the
control paths, identical first two panels, equal update budgets, 147 prior
successful roots available for D generation 1, and recovery from an epoch-20
optimizer checkpoint. Both smoke W&B runs were acknowledged through their
second generation. The launch gate checked 166 GiB free disk, an idle 32,607
MiB RTX 5090, CUDA availability, and a real smoke readiness record acknowledged
at W&B history step 1.

The supervisor at `~/Srv/aic-policy-stability-20260923` now runs the arms
sequentially. It starts a nonblocking resumable W&B observer per arm, waits for
observer readiness, and stops on a trainer, tracking, panel, update-budget,
replay or frozen-lower mismatch. Arm A has already committed generation 0:
18/32 training completions, 270 outer roots, 3,200 updates, no replay, and a
lower checkpoint identical to the initial one. Its panel hash matches the
predeclared common panel. [The A W&B run](https://wandb.ai/acrylicshrimp-personal/endfield-aic-layout-gen/runs/ee78cfa06405cde238aed635)
acknowledged this real first-generation record at history step 0.

The compact [launch manifest](2026-09-23-hierarchical-policy-stability-screen-launch.json)
records the source commit `31555f5`, release binary and input hashes, exact
launch time, preflight, W&B readiness, and first committed generation.
Configurations and the selection script are tracked under
`docs/benchmarks/hierarchical-policy-stability/`.

To reproduce the sequential screen from the isolated server source after
preparing the inputs described in the design, run:

```bash
bash tools/factory-edit-pilot/run_hierarchical_stability_screen.sh \
  "$WANDB_ENV_FILE" "$ACKNOWLEDGED_SMOKE_RUN_ID"
```

The supervisor log is
`~/Srv/aic-policy-stability-20260923/artifacts/policy-stability-screen-supervisor.log`;
arm outputs and W&B observer status are under
`~/Srv/aic-policy-stability-20260923/artifacts/policy-stability-screen/arm-{a,b,c,d}`.
If the supervisor stops, retained atomic generation and epoch checkpoints
remain the recovery authority. The screen must finish and be analyzed before
any treatment is selected or extended training begins.
