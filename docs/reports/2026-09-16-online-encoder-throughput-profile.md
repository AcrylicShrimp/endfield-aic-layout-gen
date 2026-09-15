# Online encoder throughput profile

## Result

The registered 16-worker CUDA configuration sustains approximately 51,000 to
54,000 training samples per minute after startup. A 100-update run including
one fresh development evaluation sustained 48,064 training samples per minute.

The clear optimization target is procedural root generation. The runner
generates the 16 roots for an update serially before opening the persistent
physical-editor worker pool. Generation consumed about 297 ms of a 659 ms warm
update, while GPU training consumed about 21 ms. The GPU was idle for most of
the run. Increasing the model minibatch from four to eight or sixteen roots did
not materially improve throughput.

## Measurement

All runs used the NVIDIA GeForce RTX 5090 host, release mode, `cuda:0`, the
registered online configuration, and fresh output directories. A sample is one
charged native physical-editor action row. Profiling was bounded and did not
write generated roots or rows to disk.

| Run | Training samples | Trainer elapsed | Samples/min |
| --- | ---: | ---: | ---: |
| 40 updates, no development evaluation | 20,480 | 22.98 s | 53,463 |
| 100 updates, one 16-root development evaluation | 51,200 | 63.91 s | 48,064 |

The 100-update run also generated and evaluated 512 development samples. Its
peak process-tree RSS was 2.17 GB.

An external sampler collected 283 observations at approximately 4 Hz during the
100-update run:

| Signal | Mean | P50 | P90 | Maximum |
| --- | ---: | ---: | ---: | ---: |
| GPU utilization | 2.35% | 0% | 10% | 15% |
| GPU memory | 1,320 MiB | 1,340 MiB | 1,340 MiB | 1,340 MiB |
| GPU power | 85.4 W | 86.9 W | 88.2 W | 90.6 W |
| Process-tree CPU | 559% | 580% | 616% | 645% |
| Process-tree RSS | 2.00 GiB | 2.03 GiB | 2.03 GiB | 2.15 GiB |

The host has 24 logical CPUs, so the run used about 5.6 CPU cores on average
despite configuring 16 editor workers.

## Phase profile

A temporary `Instant` profile covered ten updates and was removed after the
measurement. Excluding the first update's CUDA initialization, the mean phase
times were:

| Phase | Mean per update | Share of warm update |
| --- | ---: | ---: |
| Generate and admit 16 roots | 296.7 ms | 45.0% |
| Open native sessions | 5.7 ms | 0.9% |
| Collect 512 editor outcomes | 209.6 ms | 31.8% |
| Pack four-root graph minibatches | 126.0 ms | 19.1% |
| Forward, backward and Adam update | 20.9 ms | 3.2% |
| Total | 659.5 ms | 100% |

Packing varied with generated graph structure; the final three observed updates
averaged 25.0 ms. The generation and editor costs remained dominant. The first
update paid a separate 629 ms CUDA initialization cost, which is negligible in
a 12-hour run.

## Controlled comparisons

Worker scaling used 40 updates per configuration:

| Workers and roots/update | Samples/min | Gain from previous |
| ---: | ---: | ---: |
| 4 | 32,435 | — |
| 8 | 42,389 | 30.7% |
| 16 | 53,463 | 26.1% |

More workers still help, but efficiency falls because root generation remains
serial and the editor advances streams through synchronized exchanges.

Minibatch comparisons used the same first 20 training updates and 16 workers:

| Roots/minibatch | Samples/min | Peak RSS |
| ---: | ---: | ---: |
| 4 | 51,090 | 2.17 GB |
| 8 | 51,772 | 2.18 GB |
| 16 | 51,497 | 2.22 GB |

The differences are within run variation. Enlarging the minibatch is not a
useful first optimization.

## Diagnosis

The unreasonable bottleneck is the serial loop in
`crates/aic-learning/src/outcomes/live.rs` that constructs and admits every root
before the worker pool begins editor work. It is the largest measured phase,
uses one core, and directly explains the low aggregate CPU and GPU utilization.

The next performance change should parallelize this bounded 16-root generation
and admission step while retaining deterministic per-root seed lanes and the
existing persistent editor pool. Perfect scaling is not expected, but removing
most of the measured 297 ms serial section gives a theoretical ceiling near
1.7 times current throughput. A bounded implementation should target a more
conservative 1.3 to 1.6 times improvement and verify it with the same 40-update
measurement.

GPU utilization is low because the workload is data-production bound; increasing
model size or minibatch size would consume more GPU without addressing the
measured wall-time bottleneck. Pipelining collection with training can be
considered after generation is parallel, but the current 21 ms training phase
limits its standalone benefit.
