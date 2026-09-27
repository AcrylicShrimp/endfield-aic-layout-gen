# Full-factory pose vocabulary scale closeout

## Question and method

The full-factory caller rejected an otherwise valid observation when its used
bounding box exceeded 200,000 cells. We tested whether removing that guard could
preserve existing actions and keep candidate enumeration within practical time
and memory at the declared upper facility count. The old code also limited
translation groups to 512, compound groups to 128, and selected facilities to
16. A separate replica stress case was constructed to activate the group limits.

Baseline source was commit `4502add4` with only the ignored measurement test
added via `docs/benchmarks/full-factory-pose-scale/baseline-measure.patch.txt` in a
detached worktree. Final source identity is commit `03ac8753`, exported with
`git archive` into a clean server source directory. No training or CUDA workload
was launched. Server measurements used
`ashrimp-arch` CPU, with all files under
`~/Srv/plan23-vocabulary-20260927/`. Available space before the run was 152 GiB;
available memory was about 60 GiB and no active GPU compute process was listed.

The source fixture is
`docs/benchmarks/constructive-bound-interconnect/fixtures/crossing-witness.json`.
`docs/benchmarks/full-factory-pose-scale/generate-stress.py` reconstructs:

- `tall`: five facilities, a 12,006-point detour, 35 × 6,022 used box (210,770
  cells), SHA-256 `36a5e21dc89df128612d88130798983cb718d46d6e8edc26974d2eb9cc35b011`.
- `replicated`: 51 isolated replicas merged by commodity, 255 facilities,
  35 × 6,022 used box (210,770 cells), SHA-256
  `0b4054c2c3d421b9a32e30fd7c436c59caebff43d982c87a7da54e2c3ebc8027`.
  This repeated construction is a stress input, **not** evidence of a diverse
  training distribution.

Both stress witnesses passed `aic-witness-probe` covered-geometry validation and
`FactoryEditSession::new` full-factory admission. The probe CLI always prints
`factory_validated: false`; its covered-geometry field is the probe verdict.
The retained historical 59-facility witness at
`artifacts/factory-edit-pilot/full-factory-pose-historical-input/witness.json`
passed full-factory admission as the existing-action control.

## Result

| Case | Source | Pose/compound groups | Actions | `describe` | Process RSS | Selected pose route |
| --- | --- | ---: | ---: | ---: | ---: | --- |
| Historical 59 | baseline | 119 / 67 | 2,261 | 103.168 ms | 53,432 KiB | not run |
| Historical 59 | changed | 119 / 67 | 2,955 | 283.668 ms | 52,848 KiB | not run |
| Sparse tall 5 | changed | 11 / 6 | 810 | 23.891 ms | 53,104 KiB | candidate, 27 expansions |
| Replica stress 255 | changed | 561 / 306 | 35,456 | 3,047.656 ms | 53,248 KiB | candidate, 24 expansions |

The baseline 59-facility action SHA-256 is
`cfbc9a001a9337c7e5519a506952f0ecb91bea84bab9efcedb91724eb9521749`;
the changed serialization SHA-256 is
`a99d790a6fef106af911a82cd763f140677e2499c67d74ab22698abd93abddec`.
All 2,261 baseline actions appear unchanged and in the same relative order in
the changed list. The old default stopped compound preflights after 512 calls
and silently returned a partial vocabulary. Removing that default stop exposed
694 further actions in the 59-facility case. The 255-facility case exceeds both
former group truncations; it produced all 561 translation and 306 compound
groups. Its 918 compound streams each retained their configured eight
candidates after 10,093 preflights. The prior 512-call run returned only 28,618
actions. A direct 17-facility pose passed both pose option construction and
movement planning. The group stress observation's largest *required* group was
only two members, so that direct test establishes capacity alignment but does
not prove the frequency of larger real closures.

The implementation builds a single body occupancy index for pose enumeration.
For large sparse canvases, it stores occupied coordinates rather than allocating
the full bounding box. Component actions construct their own index after pose
enumeration releases the first. Rectangle enumeration has configurable work and
temporary-buffer budgets, defaulting to 100 million inspected cells and 512 MiB
for the row, stack and rectangle buffers together. Exhaustion returns
`factory.vocabulary-resource-incomplete` with the required/budget amount; the
old 200,000-cell rejection is gone. Translation and compound groups have no
independent count truncation. Per-group scan and candidate counts still bound
enumeration. The global compound preflight budget defaults to the maximum
representable count, so the per-stream bounds determine normal work; when a
caller explicitly supplies a smaller global budget, exhaustion with pending
streams now returns typed incomplete instead of a successful partial list. A
one-preflight unit case and the 255-facility case with the historical 512-call
limit both returned that diagnostic.

Compound empty-space scanning now runs only when a catalog-legal rigid stream
exists. A two-facility sparse canvas above 100 million cells with zero compound
groups passes pose enumeration without that unused scan. The point vector and
occupancy sets use fallible growth, so allocation refusal is classified by the
same typed resource diagnostic.

The full 255-facility vocabulary takes about 3.0 seconds instead of 0.54
seconds for the old truncated vocabulary. This is the cost of enumerating the
configured eight candidates per stream, and it remains a first-turn cost to
consider before a long training run. The stress replica does not establish the
distribution of factory scale or topology shown to the model.
The 53,248 KiB process RSS peak includes the live occupancy index and all
temporary scan buffers; it is an upper bound on their resident peak together,
not a separate allocator-level attribution.

## Reproduction and limits

```sh
python3 docs/benchmarks/full-factory-pose-scale/generate-stress.py --kind tall --output target/pose-tall.json
python3 docs/benchmarks/full-factory-pose-scale/generate-stress.py --kind replicated --output target/pose-255.json
cargo test --release -p aic-pilot-native --no-run
python3 docs/benchmarks/full-factory-pose-scale/measure.py --source . --witness target/pose-255.json --output target/pose-255-measure.json --execute-first

witness="$PWD/artifacts/factory-edit-pilot/full-factory-pose-historical-input/witness.json"
git worktree add --detach target/pose-baseline 4502add4
git -C target/pose-baseline apply "$PWD/docs/benchmarks/full-factory-pose-scale/baseline-measure.patch.txt"
(cd target/pose-baseline && AIC_POSE_VOCABULARY_WITNESS="$witness" AIC_POSE_DUMP_ACTIONS="$OLDPWD/target/old-actions.json" cargo test --release -p aic-pilot-native measure_pose_vocabulary_witness -- --ignored --nocapture)
AIC_POSE_VOCABULARY_WITNESS="$witness" AIC_POSE_DUMP_ACTIONS="$PWD/target/new-actions.json" cargo test --release -p aic-pilot-native measure_pose_vocabulary_witness -- --ignored --nocapture
python3 docs/benchmarks/full-factory-pose-scale/compare-actions.py --baseline target/old-actions.json --current target/new-actions.json
```

For input admission, invoke `aic-witness-probe` with the generated witness and
the five normalized game catalog files. The ignored test checks full-factory
admission before describing actions. The measured route call uses work budget
5,000 and default reembedding limits. Its successful candidate result proves
the upstream vocabulary no longer blocks that call; it does not classify other
route stops.

## Verdict and review

Plan .23's vocabulary admission and candidate completeness contract passed for
the measured supported band. The first independent implementation review found
the silent global preflight stop and an incomplete temporary-buffer budget; the
second found an unnecessary scan with no rigid streams and infallible point
collections. Those were fixed. The third review returned `PASS_WITH_NOTES` with
no blocking finding. Its measurement-harness note was fixed by making the
ignored test fail when `describe` or a requested route fails, then rerunning the
workspace tests and clean-commit server measurement. The remaining note is
that a required connected closure above 16 members has not been observed in a
realistic witness; direct 17-member pose and movement execution passed, but
the cost of a large connected closure remains unmeasured.

Plan .19 owns later route-stop classification. Plan .08 owns actual training
distribution and broad-scale readiness; the 255-facility replica here is a
capacity stress input only.

Verification: `cargo fmt --all`, `cargo check --workspace`,
`cargo test --workspace`, tracked-file worktree check, and the relevant witness
probe CLI runs passed. The baseline action comparison script retained all
2,261 old actions in relative order.

## Cleanup manifest

After committing the implementation and verifying the source fixture,
generation script, baseline measurement patch, reproduction commands and
summary above, the regenerable server directory
`~/Srv/plan23-vocabulary-20260927/` was removed. It contained the working and
clean source exports, release targets, generated witnesses, raw logs and action
dumps. Its measured size before removal was 3,517,145,194 bytes. Server free
space changed from 159,088,320,512 to 162,933,178,368 bytes, and the directory
was confirmed absent. Local detached baseline worktree
`target/plan23-base`, generated `target/plan23-sparse`, action dumps, test log
and Python bytecode were also removed. The tracked scripts and this report
retain the compact reconstruction and conclusion; no continuation checkpoint
is required.
