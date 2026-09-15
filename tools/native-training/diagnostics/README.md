# Explicit native training profiling

These Linux diagnostics profile the existing release CLI through LibTorch's
Kineto/CUPTI recorder. They are separate from training, require no runtime edits,
and never alter determinism, sampling, PPO coverage or checkpoints. Keep them off
for normal throughput measurements and long runs.

Use the same `LIBTORCH` and C++ ABI as the CLI. The installation must provide
Kineto/CUPTI and CUDA activity support. From the repository root:

```sh
export LIBTORCH="$PWD/.venv/lib/python3.12/site-packages/torch"
mkdir -p artifacts/rust-training-profile
g++ -O2 -std=c++20 -shared -fPIC \
  -D_GLIBCXX_USE_CXX11_ABI="${LIBTORCH_CXX11_ABI:-1}" \
  -isystem "$LIBTORCH/include" \
  -isystem "$LIBTORCH/include/torch/csrc/api/include" \
  tools/native-training/diagnostics/kineto-preload.cpp \
  -L"$LIBTORCH/lib" -Wl,-rpath,"$LIBTORCH/lib" \
  -ltorch_cpu -lc10 -ldl \
  -o artifacts/rust-training-profile/kineto-preload.so

node tools/native-training/prepare-cached.mjs \
  artifacts/native-training-fresh-final \
  artifacts/rust-training-profile/cached-kineto 2

env LD_LIBRARY_PATH="$LIBTORCH/lib" \
  LD_PRELOAD="$PWD/artifacts/rust-training-profile/kineto-preload.so" \
  AIC_KINETO_TRACE="$PWD/artifacts/rust-training-profile/cached-trace.json" \
  target/release/aic-cli learning train \
  --config tools/native-training/training.json \
  --output artifacts/rust-training-profile/cached-kineto --updates 4 --resume

python3 tools/native-training/diagnostics/summarize-kineto.py \
  artifacts/rust-training-profile/cached-trace.json \
  artifacts/rust-training-profile/kineto-summary.json
```

The source must be a retained complete four-update output with checkpoint 2 and
its actual next-wave input slots. See the parent README for panel preparation.
Use fresh output/trace paths. `AIC_KINETO_TRACE` is an absolute destination; its
parent must exist. The recorder activates only for `learning train`, so inherited
preloading does not profile constructor subprocesses.

For a shape diagnosis, prepare a second cached output, run unprofiled to update 3,
then resume to update 4 with the recorder and `AIC_KINETO_SHAPES=1`. This isolates
one actor-active update after an actual mid-wave checkpoint. Shape recording can
retain tensors and change allocation behavior; use it only to attribute the
already measured bottleneck. Stack, memory and FLOP recording remain disabled.

Training's report excludes trace export after the CLI returns. A two-update trace
was about 1 GB; offline JSON analysis also needs several GB of host memory. Check
that the summary contains actual CUDA kernels, not just CPU events. Retain raw
traces in ignored artifacts when their timings are needed as evidence; commit
only compact findings and provenance.

CPU inclusive time, CUDA API waits and GPU time overlap. Do not add those columns.
The summarizer computes the union of GPU activity intervals and attributes
kernels through Kineto external IDs. CPU self time subtracts nested CPU operators,
while keeping runtime waits inside the owning operator. Kernel tables count
launches; CPU tables count operator calls. Shapes describe gradient destinations
and incoming gradients, not necessarily model inputs. Worker CPU preparation and
native environment work are not fully covered by main-thread ATen callbacks;
use the normal phase/prepare-worker timers for that context.

## Generation phases and process initialization

`generation-overlay.py` copies the Cargo workspace's sources to a new ignored
directory and inserts phase timers there. It checks unique source anchors before
writing; it never edits production sources. `child-main-preload.c` records child
main wall/CPU time, and the overlay includes those records in its process logs.
Use Bash with the matching LibTorch build environment configured:

```sh
mkdir -p artifacts/generation-profile
python3 tools/native-training/diagnostics/generation-overlay.py \
  . artifacts/generation-profile/source
gcc -O2 -shared -fPIC tools/native-training/diagnostics/child-main-preload.c \
  -ldl -o artifacts/generation-profile/child-main-preload.so
cp target/release/aic-cli artifacts/generation-profile/aic-cli-normal
(
  set -e
  trap 'cp artifacts/generation-profile/aic-cli-normal target/release/aic-cli' EXIT
  cargo build --release --manifest-path artifacts/generation-profile/source/Cargo.toml \
    --target-dir target -p aic-cli --features learning
  cp target/release/aic-cli artifacts/generation-profile/aic-cli-profile
  cargo build --release -p aic-cli
  cp target/release/aic-cli artifacts/generation-profile/aic-cli-without-learning
)
env LD_PRELOAD="$PWD/artifacts/generation-profile/child-main-preload.so" \
  artifacts/generation-profile/aic-cli-profile learning train \
  --config tools/native-training/training.json \
  --output artifacts/generation-profile/fresh --updates 2 \
  > artifacts/generation-profile/run.stdout 2> artifacts/generation-profile/run.stderr
python3 tools/native-training/diagnostics/summarize-generation.py \
  artifacts/generation-profile/run.stderr artifacts/generation-profile/summary.json
python3 tools/native-training/diagnostics/compare-generation-child.py \
  artifacts/generation-profile/fresh/inputs/00000000/0000/input.json \
  artifacts/generation-profile/aic-cli-normal \
  artifacts/generation-profile/aic-cli-without-learning \
  artifacts/generation-profile/child-comparison
```

Use new output directories and preserve the normal executable before reusing
its build cache. The source overlay and generated build outputs are disposable;
retain the raw diagnostic records, comparison inputs and required actual state.
Successful generation may include failed subprocess attempts; the summarizer
keeps their cost instead of treating the entire completed wave as a failure.
Worker and child duration sums overlap in elapsed time. The command comparison
checks identical JSON and does not change normal training's executable choice.
