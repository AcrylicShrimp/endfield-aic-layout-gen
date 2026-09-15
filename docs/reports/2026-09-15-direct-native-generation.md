# Fresh generation directly reuses native worker threads

Removing repeated constructor CLI launches reduced fresh generation time from
**15.47 to 7.87 seconds** across two waves. The complete four-update training run
improved from **33,846 to 45,587 new samples/minute (+34.7%)**. This measurement
includes actual fresh factory construction, collection, all PPO minibatches and
checkpoint publication.

## Change

`Source` now calls the existing Rust throughput, wiring and production functions
inside its existing persistent worker. It reuses the shared validated catalogs
and calculated wiring. The executable/catalog path fields, process polling,
stdout/stderr reader threads, child JSON serialization and duplicate wiring
computation have been removed. Final construction history is released before
randomized setup; only the final witness moves into the editor.

Existing construction options, work limits, generation draws, random setup,
reserved evaluation geometries, actual input identity and publication/recovery
contracts remain. A scoped work control checks elapsed and resource limits inside
search. The same microlp solver resumes its existing state between cancellation
checks. See `docs/designs/2026-09-15-direct-native-generation.md` for the continuity
checklist and cooperative interruption contract. There is no alternate executable
or subprocess fallback.

## Integrated measurement

Same RTX 5090 host, configuration and 128 streams / 16 workers. Each fresh run
completed 16,384 new transitions and four updates. The baseline is the retained
post-gather-optimization run; the after run used the new release binary without
profiling or cached input preparation.

| Measurement | Before | Direct native generation |
| --- | ---: | ---: |
| Complete elapsed time | 29.045 s | 21.564 s |
| Fresh input admission | 15.472 s | 7.871 s |
| Collection | 6.109 s | 6.283 s |
| PPO | 5.119 s | 5.078 s |
| Checkpoint publication | 0.111 s | 0.133 s |
| New samples/minute | 33,846 | **45,587** |
| Retained output bytes | 29,204,254 | 29,204,951 |
| Observed peak process-tree RSS | 15.55 GB | 8.36 GB |

Generation elapsed time fell **49.1%**. CPU worker durations overlap; the earlier
76.6% sum of time outside child main was not a prediction of elapsed speedup.
The remaining two admission waves took 3.85 and 4.02 seconds. The five slowest
retained fresh slots took 3.17–3.30 seconds each and recorded a failed
`growth-exhausted` construction before succeeding on another attempt. Those
construction/retry tails remain after removing startup overhead.

A separate actual checkpoint-2 resume completed the next two updates in 7.649 s,
or **64,261 samples/minute**, preserving the existing cached path's approximately
64,326 samples/minute. It reused actual next-wave inputs and did not measure fresh
generation. This number is not the fresh training rate above.

These are bounded observations, not repeated statistical trials or a learning
quality experiment. Four updates do not reach periodic evaluation at update 128.
Reported peak RSS is the existing sampled process-tree metric, not exact allocation
accounting.

## Correctness and verification

- All 116 still-retained fresh input files match the baseline in request, final
  witness, draw, generation seed, source identity, constructor geometry and setup
  edit records. Timing fields and the changed failure-diagnostic representation
  are excluded. Earlier-wave files were already removed by normal retention.
- All four updates retain 64 minibatches, four critic epochs and actor epochs
  4/4/1/4. Episode quality and replay sample/label counts match. Floating PPO
  statistics are not bitwise identical across these complete fresh runs; maximum
  observed KL difference is about 0.0000612. This comparison does not claim exact
  historical training replay.
- Default workspace tests: **558 passed, 3 ignored**. Learning-feature workspace:
  **575 passed, 5 ignored**, including CPU/CUDA tests. Both workspace checks pass.
  The existing large CLI parser tests use `RUST_MIN_STACK=16777216` for their test
  workers; normal training settings are unchanged.
- New tests cover cancellation inside construction, worker reuse, scope restoration
  after cancellation/panic, thread isolation, real fresh input publication/reload,
  and rejection of a cancelled incomplete slot. The final source adjustment also
  passed the focused real generation test.
- Actual fresh CLI execution and checkpoint resume completed. All diagnostic/test
  processes ended; no long training run was launched.

## Reproduction

Use the existing training configuration and prepared evaluation panel, with the
matching LibTorch environment described in `tools/native-training/README.md`.
Choose new output directories:

```sh
cargo build --release -p aic-cli --features learning
target/release/aic-cli learning train \
  --config tools/native-training/training.json \
  --output artifacts/native-direct-generation/fresh-after --updates 4

node tools/native-training/prepare-cached.mjs \
  artifacts/native-direct-generation/fresh-after \
  artifacts/native-direct-generation/resume-after 2
target/release/aic-cli learning train \
  --config tools/native-training/training.json \
  --output artifacts/native-direct-generation/resume-after --updates 4 --resume
```

Compact reports, source hashes, binary/configuration hashes and comparison results
are in `2026-09-15-direct-native-generation-evidence.json`. Actual checkpoints,
inputs, comparisons and test logs remain in `artifacts/native-direct-generation`
on the existing host. Local copies of summaries/logs are under
`artifacts/native-direct-generation-20260915/`. The existing baseline binary and
actual baseline outputs were preserved; redundant transfer/build copies were
removed.
