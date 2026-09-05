# Corrected factory-port directions and frozen placement

## Result

Corrected the importer rather than the renderer. Factory outward normals now derive from source
yaw and input/output role; coordinates only validate the selected face. Removed coordinate-order
corner tie-breaking entirely. Unsupported rotations, inconsistent roles and off-face positions fail.
No source snapshot, recipe, facility dimensions, stable IDs or port positions changed. A structural
old/new comparison confirms exactly 20 input edges changed east -> south and nothing else in the
facility catalog. Regenerated manifest checksums follow the new catalog; schema remains version 3.

The independent source-class audit now finds **0 mismatches among 250 ports**, down from 20 across
20 facility/mode entries. The five planter inputs all normalize south and rotate west at 90 degrees.
Source-to-catalog regression tests protect this contract independently of the layout validator's
shared runtime catalog. Historical evidence remains in `../constructive-placement-only/report.md`
and its pre-fix artifacts; this correction is not a blanket revalidation of historical searches.

Heavy Xiranite regenerates all **59 facilities at 52 x 92**, with no assigned ports or physical routes.
Coordinates and rotations are identical to the pre-fix diagnostic: this policy chooses rows from
facility rectangles, not port endpoints. Thus fixing the data does not itself improve placement.
Thirty additions still shift rows, 18 logical requirements are non-forward, and five facilities have
mixed required interface directions. These remain explicit follow-up routing risks, not proofs of
unroutability. The two-facility seed cycle also regenerates successfully without routing.

`heavy.json` is the corrected, frozen stage-one input for later mechanical wiring. Do not relocate
facilities silently when probing its routing failures. The next experiment should assign ports and
route belts before pipes within the user's isolated diagnostic exception, while recording failed
requirements and obstructed approaches. Production construction and its pipe-first policy are unchanged.

## Timing

Twenty sequential repetitions, each with a release Rust graph CLI and a fresh Node v26.5.0 process
on darwin/arm64. No compilation/tests overlap the measured runs. Filesystem cache is warm, not a
cold-start benchmark. First run is included; raw samples and statistics excluding it are also retained.
The release CLI loads recipe data and builds contextual facility-instance wiring, writing JSON directly
to a file. The Node diagnostic then loads wiring/catalogs, places facilities, serializes its report,
renders HTML and writes both artifacts. Timing uses monotonic wall clocks; no routing is included.

| Stage | Median ms | P95 ms | Min–max ms |
| --- | ---: | ---: | ---: |
| Graph CLI, including process and wiring JSON output | 37.68 | 39.31 | 27.99–39.33 |
| Node input read/parse/hash | 5.01 | 5.19 | 4.79–5.68 |
| Pure placement | 23.55 | 24.40 | 23.19–24.60 |
| Provenance hashes | 0.68 | 0.79 | 0.59–0.81 |
| Report serialization | 2.26 | 2.32 | 1.51–2.46 |
| Report file write | 1.27 | 1.39 | 1.11–1.48 |
| HTML rendering | 1.09 | 1.20 | 0.80–1.21 |
| HTML file write | 2.40 | 2.61 | 2.05–2.81 |
| Node measured body, input through artifact writes | 36.56 | 37.35 | 34.69–37.46 |
| Whole Node CLI process | 164.93 | 170.10 | 151.03–179.72 |
| Whole graph + placement + artifact pipeline | **204.04** | **209.34** | **179.95–218.85** |

P95 is nearest-rank; medians are computed independently, not additive. The body excludes argument
parsing, module/process startup, timing-file write and process shutdown. The whole-process gap is
observed but not separately profiled; do not attribute all of it to placement or disk writes. Files
are written normally, without fsync; durability flush and browser load time are excluded. First run:
205.93 ms pipeline, 23.47 ms placement. Its report is 404,901 bytes; HTML is 292,745 bytes. All 20
placement reports are identical after excluding elapsed time and provenance file paths. This is a
placement-only Node diagnostic, **not** a speedup comparison against complete routed construction.

Reproduce from repository root (output directory must not exist):

```sh
node docs/benchmarks/constructive-placement-only/time.mjs NEW_DIRECTORY 20
```

`timing.json` contains every measured sample, source/binary hashes and summaries. Full temporary
artifacts are under `/tmp/aic-corrected-ports.YYsmX4/timing/`. The underlying placement CLI's optional
`--timing-output NEW_FILE` separates phase measurements from its compact stdout completion.

## Verification and publication

- `python3 -m unittest discover -s tools/tests`: six tests pass, including five source-port tests.
- `node --test docs/benchmarks/constructive-placement-only/model.test.mjs`: eight tests pass,
  including timing-output byte counts, finite timings, compact completion and output collision checks.
- `cargo check --workspace` and `cargo test --workspace`: pass; 509 tests, two existing research ignores.
- Release `facilities validate`: valid, no diagnostics. Repeated normalization reproduces every JSON
  catalog and manifest byte-for-byte.
- Structural comparison: only the expected 20 edge values changed; all 59 placement rectangles and
  rotations unchanged. Corrected Heavy Xiranite and small-cycle CLI calls succeed.
- Browser: 59 facilities, 518 port arrows, ports visible by default, zero routes. F37 shows five
  west-facing inputs; screenshot visually inspected. No renderer patch was necessary.
- Existing tailnet server now serves corrected `placement-only.html` and the distinct
  `placement-only-corrected.html`. Historical page retained as `placement-only-before-port-fix.html`.

This is a valid placement-geometry diagnostic, not a validated factory. Fixed-placement physical
routing, representative historical reruns and a broader game-data semantic audit remain follow-ups.

## F03 orientation review

User subsequently questioned F03's perpendicular belt faces. F03 is `mix-pool-1-mode-liquid`,
running `pool-liquid-xiranite-poly-1`. Its catalog has two belt inputs and two belt outputs, but its
actual graph requires no belts: pipe inputs `item-liquid-xiranite` and `item-liquid-sewage`, pipe
outputs `item-liquid-xiranite-poly` and surplus `item-liquid-xiranite-lowpoly`. `orient` ranks active
commodity/direction groups, not unused catalog ports. Thus the belt scores tie at zero and pipe
alignment selects rotation 0. Uniform orientation based on the physical belt faces even when unused
would be a different policy (90 degrees here), making the active pipe interfaces perpendicular to
global growth. This is not another source-direction defect; no orientation policy was changed by
this diagnostic check.
