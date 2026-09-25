# Same-search model and horizon cross-evaluation

Status: six crossed evaluations complete. The observed final-model advantage
comes mainly from search horizon, not a demonstrated improvement in weights
trained by horizon-12 collections. The selected next contrast is to test
horizon-6-trained weights with horizon-12 search under new root seeds.

## Controlled question

For each of the three on-policy streams, evaluate both final outer models
under both search horizons on the same 64-case training-partition diagnostic,
lower model, case IDs and root seed 20260922. The diagonal H6/H6 and H12/H12
cells reuse the prior verified reports. Six new CLI searches fill the two
crossed cells per stream. Shadow checkpoint configs change only the search
contract; every model-weight hash matches its original final checkpoint.
The [protocol](2026-09-26-hierarchical-h12-cross-protocol.json) binds all
inputs and source code; the [progress](2026-09-26-hierarchical-h12-cross-progress.json)
retains every crossed 64-case success mask, report hash and runtime. The
runner source is commit `de23b039`.

## Results

Each cell gives `completed / lost starting successes`, where the latter
counts failures among the 26 cases solved by the common horizon-six starting
checkpoint.

| Stream | H6 model, H6 search | H6 model, H12 search | H12 model, H6 search | H12 model, H12 search |
| --- | ---: | ---: | ---: | ---: |
| s1 | 20 / 19 | 42 / 8 | 23 / 16 | 36 / 14 |
| s2 | 24 / 19 | 35 / 8 | 25 / 15 | 36 / 12 |
| s3 | 20 / 15 | 34 / 12 | 21 / 13 | 34 / 9 |

Holding the H6-trained weights fixed, extending search to H12 changes
completion by **+22, +11 and +14**, mean **+15.7/64**. Holding H12-trained
weights fixed, the same search change gives **+13, +11 and +13**, mean
**+12.3/64**. By contrast, under fixed H12 search, changing from H6-trained
to H12-trained weights gives **−6, +1 and 0**, mean **−1.7/64**. Under fixed H6
search, that model change gives **+3, +1 and +1**, mean **+1.7/64**. The
[summary](2026-09-26-hierarchical-h12-cross-summary.json) records the full
matrix and both conditional effects.

The H6-trained/H12-search combination completes 42, 35 and 34 cases versus
36, 36 and 34 for the H12-trained/H12-search combination. It loses 8, 8 and
12 starting successes versus 14, 12 and 9. Thus the mixed setup has mean
completion advantage 1.7/64 and loses 2.3 fewer starting successes on this
one seed, although individual streams disagree. Its much larger advantage
over H6-trained/H6-search is a search effect under identical trained weights.

These factor comparisons are conditional on the three retained training
streams, this diagnostic panel and one root seed; they do not prove that
horizon-12 training is harmful generally. In particular, H12 training changes
its own on-policy collections, so the model factor includes those data
changes. The earlier nine-pair diagonal seed test established robustness of
the combined H12/H12 setup but did not test the mixed setup. New-seed mixed
evaluations are therefore required before choosing between them. The six
new integrated searches averaged 202.8 seconds each, including report
publication and lineage validation.

## Reproduction and closeout

With the retained inputs and LibTorch environment in the recorded server
workspace:

```sh
python3 tools/factory-edit-pilot/hierarchical_horizon_cross.py prepare \
  --output artifacts/recovery-horizon-onpolicy-v2
python3 tools/factory-edit-pilot/hierarchical_horizon_cross.py run \
  --output artifacts/recovery-horizon-onpolicy-v2
python3 tools/factory-edit-pilot/hierarchical_horizon_cross.py summarize \
  --output artifacts/recovery-horizon-onpolicy-v2
```

The [cleanup manifest](2026-09-26-hierarchical-h12-cross-cleanup.json)
records removal of replayable crossed raw reports and logs after compact
evidence verification. Crossed shadow checkpoint configs and small weight
copies are retained temporarily for the next paired-seed continuation.
