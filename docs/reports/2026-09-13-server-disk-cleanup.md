# Server disk cleanup

## Result

On `ashrimp-arch`, available space increased from approximately 14 GiB at the
initial inspection to **56.75 GiB**. The exact final filesystem observation is
249,845,608,448 bytes total, 176,141,778,944 used and 60,937,838,592 available (75%
used). Approximately 43 GiB was reclaimed including the initial debug-cache
cleanup. The first byte-precise observation was taken after that initial cleanup;
do not present the initial rounded figure as an exact byte measurement.

The replay-balance implementation was paused for the requested cleanup. No main
training or readiness process had started. Its unfinished source edits are a
separate work unit and are not part of this cleanup commit.

## Why the disk filled

After cleanup, AIC artifact directories still occupy **102.96 GiB allocated**.
This is mostly accumulated research evidence, rather than rendered views:
compressed Torch payloads, uncompressed tensors, recorded JSON and trajectories.
Before cleanup, the related artifact inventory found about 68.5 GB of `.pt.gz`,
18.9 GB of JSON and 11.9 GB of `.pt` payloads by apparent file size. Those suffix
figures are descriptive and must not be added to allocated-directory totals.

| Significant remaining location | Approximate size | Meaning |
| --- | ---: | --- |
| `Srv/aic-blueprint-advantage-20260912` | 40 GiB | Six-arm experiment, episode inputs, chunks, evaluation traces and checkpoints |
| `Srv/aic-ppo-area-20260909` | 11 GiB | Earlier input/area research after removing redundant archive copies |
| `Srv/endfield-aic-layout-gen/artifacts` | 9.4 GiB | Earlier diagnostic and learning evidence |
| `Srv/aic-ppo-combinations-screen-20260911` | 9.0 GiB | Eight-condition historical screen |
| vLLM Hugging Face model cache | 24.56 GiB | Downloaded model files; root-owned |
| `/var/cache/pacman/pkg` | 3.6 GiB | System package cache; root-owned |

Many smaller frozen experiments make up the rest of the AIC evidence. The current
blueprint run retains substantially more than a final model: its 512 updates,
129 evaluation checkpoints per arm, full sampled experiences and actual inputs
explain the footprint. HTML/SVG views across related experiments total only
about 0.45 GB and are not the main cause.

## Removed

- Regenerable `Srv/endfield-aic-layout-gen/target/debug`, approximately 6.2 GiB.
- Forty-five Cargo release intermediate directories (`build`, `deps`,
  `incremental`, `.fingerprint`, `examples`) in the identified AIC workspaces:
  4,036,888,922 apparent bytes. Top-level release CLI binaries and native libraries
  remain. Hard links mean apparent cache bytes need not equal reclaimed blocks.
- The unused `vllm/vllm-openai:nightly` Docker image, reported as 28,777,580,426
  bytes. Docker had no containers, volumes or build cache. Its immutable repository
  digest is retained in provenance so it can be pulled again.
- Seven server archive duplicates totaling 7,750,814,287 bytes (7.22 GiB).
  Each matched its existing local archive in size and SHA-256 before removal.
  Local archives and unpacked server evidence remain. The provenance records
  both paths; old reports' server archive paths now resolve through that record.
- The uv package download cache, using `uv cache clean`. uv reported only 9.5 MiB
  reclaimed even though a separate directory inventory had shown several GiB;
  installed environments retain shared package files. No virtual environment was
  deleted.

## Preserved and permissions

No solver/training experience, actual generated input, learned model checkpoint,
interrupted state, source checkout, installed virtual environment or application
configuration was intentionally removed. Both retained archive copies and source
data are still governed by the repository's retention policy; ignored storage is
not a backup guarantee.

An attempted removal of the inactive downloaded model cache failed on its first
root-owned entry with `PermissionError`. No successful model-cache deletion is
claimed. `sudo -n true` also reports that a password is required. The user was
given the two exact model-cache paths for optional direct administrator cleanup.
The package cache remains as well. No privileged container or other mechanism
was used to bypass the missing filesystem permission.

The inference image must be downloaded again before restarting that Docker
service. Its project settings and downloaded model files remain. The selected
training Python environment still imports Torch `2.14.0+cu130` and reports CUDA
available after cleanup. No broad training/replay workload was rerun to verify
filesystem maintenance.

## Evidence

The [compact provenance](2026-09-13-server-disk-cleanup-provenance.json) binds the
cache inventory, verified archive copies, removal records and final allocated
usage. Full records are retained under local `artifacts/server-cleanup-20260913`
and server `Srv/endfield-aic-layout-gen/artifacts/server-cleanup-20260913`.
Documentation worktree/index policy and whitespace checks accompany this unit.
