# Repository-size investigation at handoff

Status: measured read-only, 2026-09-07. No cleanup or history rewrite performed.

The user observed roughly 1.4 million added lines. The exact UI comparison base is
unknown; the measurements below do not claim to reproduce that displayed diff.

## Tracked text versus code

At base `44a854f`, `git ls-files` lists 3,371 tracked files. Counting newline bytes in
their current working copies gives 5,190,404 lines. This is file-content line count,
not executable source lines or a count of newly added lines.

| Category | Files | Lines | File bytes |
| --- | ---: | ---: | ---: |
| Benchmark JSON / HTML / SVG | 2,176 | 4,796,622 | 245,811,136 |
| Other benchmark files, including scripts and reports | 757 | 87,796 | 10,053,261 |
| Rust crate files | 184 | 112,523 | 4,250,534 |
| Runtime/source data | 66 | 172,372 | 4,784,599 |
| Other tracked files | 188 | 21,091 | 1,126,119 |

About 92.4% of counted lines are benchmark JSON/HTML/SVG, not Rust implementation.
That category includes inputs and fixtures as well as generated results; do not blindly
delete every JSON file. Large generated summaries and repeated payloads are a confirmed
major contributor:

- `heavy-xiranite-bottom-up-rotation-root/summary.json`: 94,151 lines;
  its `summary.html`: 94,156 lines.
- The corresponding `-medium/` pair has the same line counts.
- Parsed the HTML's escaped embedded JSON in both pairs and compared it with the
  standalone JSON: both match exactly. These four files repeat report content and
  contribute 376,614 lines alone.
- `constructive-shape-packing/full-matrix.json`: 79,150 lines;
  `constructive-factory-closure/output-space-refined.json`: 72,266 lines.

The pre-handoff tracked dirty diff is only 50 additions / 14 deletions. The historical
constructor base `a71fddf` to `44a854f` adds 41,128 lines and deletes 32; its largest
addition is the 19,619-line `layout-rl-next/pilot-results.json`. That historical comparison
is not necessarily the user's UI base. The recent 64/256/rank reports add hundreds,
not millions, of lines. The handoff's source-overlay archive is about 81 KB of text;
it does not archive the multi-gigabyte episode data into Git.

## Disk usage is a separate issue

Rounded allocated sizes at this check:

- `.git`: 16 MiB; packed objects 14.42 MiB (`git count-objects -vH`). Repeated textual
  data compresses/delta-compresses well. The current Git object store is not 83 GiB.
- `target/`: 83 GiB, ignored by Git. It mixes compilation outputs with important
  historical experiment artifacts. `target/debug` is 13 GiB and `target/release`
  about 366 MiB; do not treat everything else as disposable cache.
- `docs/`: 877 MiB, including the ignored 624 MiB Python environment under
  `docs/benchmarks/layout-learning/.venv`.
- `data/`: 4.7 MiB; `crates/`: 4.4 MiB.

Thus review noise is dominated by verbose tracked benchmark reports, whereas local disk
usage is dominated by ignored build/research artifacts. They need different remedies.

## Recommendation, not an authorized cleanup

Keep concise findings, manifests/hashes, reproduction scripts, and necessary small test
fixtures in Git. Inventory raw results and generated visual duplicates separately;
preserve required evidence in a durable artifact archive before removing active copies.
Decouple experimental evidence from disposable Cargo outputs so `cargo clean` cannot
silently destroy the research record. The present `target/` evidence is not backed up
by this handoff commit.

Changing current tracking, deleting raw files, and rewriting Git history are three
different operations. None was authorized or executed by the size question. Removing
tracked files in a new commit would reduce the checked-out tree but not remove history;
given the current small object store, a history rewrite is not the first priority.
First decide retention requirements and destinations, then plan a reviewed migration
with reference/hash validation. Do not bulk-delete `docs/benchmarks` or `target/`.
