# Confirmation tracking recovery

The independent-stream confirmation uses the fixed protocol in commit
`f23f3b3c0899c05164231e1e8243b229d2` and training binary SHA-256
`e594b5425074a03731524d8c91991126232d840ef44a86e8b0630e1f16800fdf`.
The curriculum training source remains commit `31555f5`; no model setting or
training result was changed during this recovery.

After S1 A-D and S2 A-B had each committed eight generations, the S2-B W&B
observer exited. Its local curriculum report was complete, all eight generation
outputs were present, and the observer had exported eight generations, but W&B
acknowledged history only through step 6. The observer reported
`final W&B history has not been acknowledged`, so the sequential supervisor
stopped before S2-C. This was a telemetry publication failure, not a training
failure. The postprocessor correctly stopped with five acknowledged arms.

The missing generation-7 metrics were reconstructed from S2-B's committed
`summary.json` and `training/report.json` using the existing
`committed_metrics` function, then published to the same
[W&B run](https://wandb.ai/acrylicshrimp-personal/endfield-aic-layout-gen/runs/41612607baed99588c33f9bd).
The remote API then returned step 7 and a generation-7 row. Resuming the
existing observer marked it `finished` with remote step 7. The resumed W&B
history includes duplicate rows for generations 0-6, so local atomic reports
remain the authoritative evidence; the duplicates do not represent training
repetitions.

Commit `188512479cfdb9ab5dae371b940f4c8ca235d587` added a `--resume`
mode to the same sequential runner. It verifies each existing arm's eight
committed panels, 3,200 updates per generation, frozen lower checkpoint,
treatment settings and final W&B acknowledgement before reuse. It also retries
the observer once if a completed new arm misses final acknowledgement. The
recovery launch verified and reused all six completed arms, then started S2-C
without repeating training. The recovery supervisor PID is `180659`; the
replacement postprocessor PID is `180845`, both under
`~/Srv/aic-policy-stability-20260923`. Their logs are
`artifacts/policy-stability-confirmation/supervisor-recovery.log` and
`artifacts/policy-stability-confirmation/analysis/postprocess-recovery.log`.
No raw training artifacts have been removed while the confirmation is active.
