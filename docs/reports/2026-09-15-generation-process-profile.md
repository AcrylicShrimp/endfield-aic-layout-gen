# Fresh generation repeatedly starts the learning executable

The generator already runs on 16 persistent Rust worker threads. Each worker
nevertheless starts the CLI twice for each factory attempt: recipe wiring, then
complete construction. Those children use the same executable as training and
therefore load and initialize LibTorch/CUDA libraries even for CPU-only commands.
This repeated initialization is the largest measured avoidable generation cost.

## Bounded measurements

An isolated source copy adds wall-phase timers to `Source::generate` and its
subprocess wrapper. A small Linux preload times each child's `main` and CPU
usage. Production source and the normal executable remain unchanged. The actual
128-stream/16-worker configuration generated one fresh wave and completed two
full training updates. Admission took **7.313 s**, consistent with the preceding
two-wave fresh run's 15.472 s. It ran 128 successful slots and 264 child commands:
132 wiring calls and 132 construction calls.

| Command | Mean complete child call | Mean child `main` work |
| --- | ---: | ---: |
| Recipe wiring | **222.6 ms** | **1.85 ms** |
| Complete construction, including failed attempts | 313.0 ms | 97.6 ms |

Across all workers, generation accumulated 75.143 worker-seconds. Of that,
57.574 worker-seconds (76.6%) were outside child `main`: startup, shutdown,
parent polling and output handling. Child CPU counters independently measured
39.820 CPU-seconds before `main`, confirming substantial initialization work
rather than just sleeping in the parent. These are overlapping sums across
workers, not additions to the 7.313 s elapsed admission time.

Other summed phases were only 2.639 worker-seconds of initial randomization,
1.580 of publication, and 0.153 of drawing, conversion and metadata work. Initial
random editing and file publication are not the main cost in this measured wave.

## Same-command linkage comparison

Built the same repository's CLI without its optional `learning` feature and
ran the exact same wiring command on a source plan reconstructed from the actual
slot-zero draw. Six alternating observations per binary produced identical
successful JSON results:

| Binary | Median complete invocation |
| --- | ---: |
| Normal CLI with learning linkage | **153.83 ms** |
| Same CLI without learning linkage | **5.13 ms** |

This approximately 30-fold isolated difference supports the initialization
diagnosis. `ldd` confirms that the normal executable loads `libtorch_cpu`,
`libtorch_cuda`, `libc10` and CUDA dependencies; the other executable needs only
the ordinary system libraries. This is not a measured whole-training speedup
and is not adoption of a second production executable.

## Constructor failures and the final worker

Two construction attempts, in slots 51 and 61, failed after about 2.96 s inside
child `main` each, then succeeded on another attempt. Two further successful
constructions, in slots 58 and 118, matched reserved evaluation geometry and
were correctly retried. This explains the four extra pairs of child commands.
The construction failure's internal algorithmic reason was not captured by this
phase diagnosis; a failed bounded attempt is not global infeasibility.

The pool assigns streams by `stream % workers`, and each worker executes its
batch sequentially. It waits for all worker replies. The last worker handled
slots 3/19/35/51/67/83/99/115 and accumulated 7.040 s of generation: 4.917 s in
construction calls and 1.814 s in wiring calls. Worker generation totals ranged
from 3.849 to 7.040 s. Failed attempts therefore also lengthen the final wait.

## Implication for the implementation

The extra processes are not required by the generation mathematics. They reuse
existing CLI entry points and provide a killable elapsed-budget boundary. The
next implementation should reuse the native wiring/construction functions from
the existing worker threads, preserving cancellation, bounded work, diagnostics,
input distribution, evaluation reservation and recovery. Direct calls must not
silently drop the process wrapper's current interruption/resource guarantees.
There is no need to create another thread pool just to avoid this startup cost.

The current configuration separately bounds attempts (8), placement attempts
(256), route searches (4,096), wiring time (10 seconds), construction time
(30 seconds) and total episode setup (180 seconds). These limits contain
expensive individual inputs; they do not require launching a process per call.
This profile did not demonstrate a hard process timeout: the two failed children
returned nonzero status themselves. A direct-call replacement should distinguish
algorithmic work limits from elapsed interruption and inspect cancellation
coverage in the called solvers before removing the external boundary.

This diagnosis does not change the production runtime. The generated source
overlay is explicit, and its child timing markers can appear in diagnostic
failure metadata only. No long training run was started. The normal executable
was restored and its SHA256 verified after diagnostic builds.

## Reproduction and evidence

See the generation section of `tools/native-training/diagnostics/README.md`.
The scripts preserve the existing algorithms and rejection paths while measuring
phase boundaries. The overlay built in release mode, completed the real CLI
run, and produced complete records for all 128 successful slots and all 264
children, including the two failed attempts. Same-input child comparison passed
JSON equality. Documentation/artifact maintenance validation passed.

Compact results and provenance are in
`2026-09-15-generation-process-profile-evidence.json`. Raw logs, summaries,
comparison inputs and actual checkpoints are retained under
`artifacts/generation-profile` on the existing host, with local summaries under
`artifacts/generation-profile-20260915/`.
