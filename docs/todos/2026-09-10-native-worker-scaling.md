# Native collector worker scaling

## Status

Complete. The user requested a fresh worker-count comparison after native cutover;
24 workers was a controlled before/after condition, not an established optimum.

## Contract and completion criteria

- Keep the frozen `current-history-16` seed-17 batch-4 workload: 32 fresh factories,
  32 actions each, identical actor/critic weights, samples and physical semantics.
- Compare 1, 2, 4, 8, 12, 16, 24 and 32 persistent workers on the same server in
  a recorded randomized order. Change only worker count. No optimizer updates.
- Record startup, first/cold collection, three further collections using the same
  worker pool, peak aggregate RSS and per-repetition semantic hashes. Every batch
  must match all 1,024 retained actions. Keep the existing resource limits.
- Recommend based on repeated collection time and memory; distinguish startup
  cost and bounded evidence from a claim of universal optimality. Do not silently
  modify previously frozen experiment protocols.
- Retain one complete repetition per worker count, compact summaries for matching
  regenerable repeats, source identities and an archive on both machines. Report
  results, complete repository checks and commit the work unit.

## Activity

- Native cutover and second profile committed as `447c7f5`; the preceding native
  core is `efd3bd2`. The measured 24-worker batch improved from 29.046 to 4.131 s,
  but a worker-count sweep was not part of that controlled comparison.
- Extend the standalone replay command with explicit worker and repetition flags;
  ordinary collection and learner behavior remain unchanged.
- All eight counts completed four collections with exact frozen parity. Eight
  workers: 4.163 s median / 10.41 GiB RSS; 12: 4.105 s / 15.22 GiB; 24:
  4.078 s / 27.47 GiB. The 1–4 worker results show the throughput crossover.
- The user explicitly selected eight workers. Update the three ordinary native
  PPO run configurations; keep frozen protocols and the small readiness probe.
- Retain the exact sources, first repetitions, repeat hashes and reference inputs
  in matching local/server archives. Results and provenance are in
  `docs/reports/2026-09-10.12-native-worker-scaling.md` and its companion manifest.
