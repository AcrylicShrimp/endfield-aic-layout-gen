# Lower-restored integrated study: held-out preparation failure

Status: closed before training. No diagnostic result or held-out outcome was observed.

## Question and method

Prepare fresh, disjoint panels for the accepted restored-lower integrated check,
including a 128-case held-out panel reserved before training. The first runner
requested 2,048 held-out candidate graphs, intending to select the first 128
graphs not present in prior or new panels. The training panels (nine sets of 32)
and diagnostic panel (64 cases) were generated, but preparation failed before
the held-out panel and experiment protocol were written.

## Result and decision

The CLI rejected the held-out candidate request:

> `aic-learning.constructive-problems: partition validation produced only 641 unique full graphs for 2048 requested problems after 262144 attempts`

The request exceeded the observed unique-graph supply for this partition and
sampling configuration. It is a setup error, not a search or learning result.
The corrected runner requests 512 candidates and still requires 128 graph
identities disjoint from every prior and new study panel. The first result does
not justify any performance conclusion or use of the held-out panel.

## Reproduction and provenance

- Runner source commit: `97ec6910`, exact runner SHA-256
  `b952c6a99feddc660298fa9e2b26a606722dad40c1eef9441092aed3e94509ef`.
- Uncommitted correction to that source at closeout: one literal replacement in
  `reserve_heldout`, `"--cases", 2048` to `"--cases", 512`; the corrected runner
  is committed with this report before the next preparation attempt.
- Generation config: `docs/benchmarks/hierarchical-update-budget/generation-100-lower.json`,
  SHA-256 `c6339c7a6a4e901b8fafedafa00961ee274fb4e9afade755e19a77297e4c4a42`.
  Search and sampling inputs are bound by the runner and the accepted
  [design](../designs/2026-09-25-hierarchical-recovery-research.md).
- Server CLI SHA-256:
  `66f72a2dbdf45aadbc50e4dfe8f3634012ef5f5a76d8159c698de20f85d7a4fa`.
- Server workspace: `~/Srv/aic-target-causal-20260925`; command, after
  setting the documented LibTorch environment:

  ```bash
  python3 tools/factory-edit-pilot/hierarchical_lower_restored.py prepare \
    --output artifacts/recovery-lower-restored-v1 \
    --experiment-commit 97ec6910
  ```

The generated preparation state was removed after preserving this failure
message. The [cleanup manifest](2026-09-26-hierarchical-lower-restored-preparation-cleanup.json)
records the exact path and byte counts.
