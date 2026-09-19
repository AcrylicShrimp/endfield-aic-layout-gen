# Training server

GPU training and CUDA performance measurements use the dedicated server:

```bash
ssh ashrimp@ashrimp-arch
cd ~/Srv
```

All source checkouts, Cargo targets, temporary experiment inputs and retained
outputs must stay under `~/Srv`. Do not place project work under another home
directory or `/tmp`. Use a task-specific checkout or worktree below `~/Srv` so
the baseline checkout at `~/Srv/endfield-aic-layout-gen` remains available.

The server currently provides an NVIDIA GeForce RTX 5090 with 32,607 MiB device
memory, approximately 62 GiB usable host memory and 24 CPU cores. The retained
Rust learning environment is shown below for the account's default fish shell:

```fish
set -lx LIBTORCH "$HOME/Srv/aic-rust-learning-20260915/.venv/lib/python3.12/site-packages/torch"
set -lx LIBTORCH_CXX11_ABI 1
set -lx LD_LIBRARY_PATH "$LIBTORCH/lib"
```

That environment reports Torch `2.13.0+cu130`, CUDA availability and the RTX
5090. Confirm the live environment before each substantial run:

```bash
df -h ~/Srv
nvidia-smi
"$HOME/Srv/aic-rust-learning-20260915/.venv/bin/python" -c \
  'import torch; print(torch.__version__, torch.cuda.is_available(), torch.cuda.get_device_name(0))'
```

Nsight Systems `2026.3.2.476` is installed as the Arch `nsight-systems`
package. Process-tree CPU sampling and CUDA tracing work without root; the
server's perf paranoid level 2 does not permit system-wide CPU profiling.

Keep enough free space for the planned build, snapshots and retained evidence.
Cargo `target/debug` directories and the `build`, `deps`, `incremental`,
`.fingerprint` and `examples` children of `target/release` are regenerable cache.
They may be removed after confirming that no build is active. Preserve top-level
release binaries, virtual environments, experiment inputs, checkpoints, replay
buffers and other retained evidence unless a separate evidence review approves
their deletion. Record the paths and byte-precise free space before and after a
cleanup.

On 2026-09-20, a cache-only cleanup under `~/Srv` removed 60 Cargo cache
directories and increased available space from 991,420,416 to 44,855,083,008
bytes. The complete removal record is retained on the server at
`~/Srv/endfield-aic-layout-gen/artifacts/server-cleanup-20260920/cargo-cache-cleanup.json`.
After the GPU experiments and verification rebuilt debug caches, a second
cache-only cleanup recovered 13,702,201,344 bytes and increased available space
from 31,001,071,616 to 44,703,272,960 bytes. Its removal record is retained at
`~/Srv/aic-direct-constructive-20260920/artifacts/server-cleanup-20260920/post-training-cache-cleanup.json`.
The experiment checkout then retained approximately 45 MiB of outputs and
checkpoints and 53 MiB under `target`. The final workspace check rebuilt one
debug cache, and the schema-cutover CLI check added one disposable output.
Removing both recovered another 3,625,295,872 bytes and increased available
space from 41,077,956,608 to 44,703,252,480 bytes. That record is
`~/Srv/aic-direct-constructive-20260920/artifacts/server-cleanup-20260920/final-verification-cache-cleanup.json`.
The later Nsight profile retained its 90 MiB `.nsys-rep` and compact summaries.
Removing its regenerable SQLite export and two temporary training outputs
recovered 269,254,656 bytes; the record is
`~/Srv/aic-direct-constructive-20260920/artifacts/server-cleanup-20260920/profile-cache-cleanup.json`.
