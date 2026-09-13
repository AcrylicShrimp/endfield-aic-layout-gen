# Declarative training extension to the next morning

Status: duration amended and normal training resumed. The user explicitly
requested training until 2026-09-14 10:00 KST (01:00 UTC).

The optimized learner paused through its existing signal handler at update 192.
The new operational CLI extends only duration, matching arm/matrix wall limits,
and the forecast. It preserves the native binary, optimized execution source,
learning settings, actual inputs, replay, both optimizers, both RNG states,
selected next starts, evaluation history and best checkpoints. The existing
execution verification remains applicable to those unchanged contracts; this
operation does not claim that the previous receipt authorized the longer budget.

The command archives the exact previous launch for historical checkpoints and
saves the current checkpoint with the amended launch identity, under the existing
exclusive run lock. It adds no work to normal training and requires no replay,
input regeneration, new input bank, or repeated execution-readiness experiment.
The operation lives outside the normal source-bound Python module set.

Continuity checks: two focused tests verify state preservation, unchanged resource
ceilings, matching deadline/arm/matrix budgets, preserved old checkpoint bindings,
and rejection of running, past, timezone-free and non-extending requests. The
actual paused-run command succeeded and normal execution resumed. Rust workspace
check and tests passed under repository policy; no Rust source changed.

```sh
python tools/factory-edit-pilot/operations/extend_declarative_duration.py \
  --output artifacts/resumed-four-hour --until 2026-09-14T10:00:00+09:00
python tools/factory-edit-pilot/declarative_duration.py run \
  --output artifacts/resumed-four-hour --environment "$AIC_TRACKING_ENV"
```

The server source root remains
`/home/ashrimp/Srv/aic-declarative-efficiency-20260913` and the output directory
remains `artifacts/resumed-four-hour`. New main PID: 9037. W&B continues run
`mltpnnfq`. The amendment calculated 44,045.424169 seconds remaining at preparation.
The unchanged duration engine uses elapsed learner time and a complete-wave stop;
restart overhead and finishing the last wave/evaluation can put final shutdown
slightly after 10:00. The existing 300-second finalization allowance remains.

Amended launch SHA-256:
`9421167e96e58e73b5d86f905d6013972a80aaa1785425230cb54481c2c6e27e`.
Initial amended checkpoint: `initial/deadline-20260914T010000Z-0192.pt.gz`, SHA-256
`f32af13c0bef63f3f5683424b7dcf2a6c8f0d2ba0f8f2b1ed24e02a59326336c`.
The launch's `duration_amendments` entry records the prior launch reference and
old/new budgets. RSS, disk reserve and retained-output ceilings are unchanged.
A successful extension is not a completed training result.

Startup verification confirmed update 193 was published and the original W&B
run resumed successfully. No diagnostic workers or monitors were started.
