# Persistent independent witness validation

Status: implemented and verified, 2026-09-07. This is diagnostic transport only; the
Rust geometry, capacity, connectivity and exact recipe-margin checks are unchanged.

`aic-witness-probe --stream` takes the same five explicit catalog flags as file mode.
It loads and validates those catalogs once per execution. Each stdin line is:

```json
{"schema_version":1,"id":7,"witness":{"schema_version":1,"placements":[],"utilizations":{},"transport_networks":[],"logistics_components":[]}}
```

The example is a well-formed request for an invalid empty witness, not a valid factory.
Each stdout line contains `schema_version`, the echoed unsigned `id`, the unchanged
independent `report`, and catalog path/SHA-256 provenance. Flush after each response.
There is no solver, router, shared mutable witness or success shortcut in the stream.
The probe still never claims original-request factory completeness by itself.

The frame limit is 16 MiB. Unknown envelope fields, invalid JSON/UTF-8, unsupported
schema or oversized input terminate the process with the stable `witness-probe-failed`
diagnostic on stderr. Invalid geometry is a normal report, not a protocol error; the
stream continues. Clean EOF exits successfully. The client owns per-request timeout,
process termination and correlation-ID/provenance checks.

File mode retains `--witness` and `--report-output`, fresh-file output, exit 0 for
accepted geometry and exit 2 for rejected geometry. Those two flags conflict with
`--stream`; there is no ambiguous simultaneous file/stream mode. Both transports call
the same `probe_constructive_witness` function with the same validated catalog objects.

Verification: two Rust protocol tests cover frame separation, EOF, schema, unknown
fields, malformed and oversized lines. A manual 25-case file/stream audit compares
23 generated complete witnesses from both generator generations and two invalid
mutations. Every report field except elapsed time and transport-specific provenance
matches. Evidence: `target/layout-rl-probe-audit-01/report.json`.

The shared re-embedding route-account extension separately adds an optional aggregate
expansion cap and per-path observer. Its tests show unchanged uncapped results,
failure charging, cumulative-cap enforcement and immutable inputs. This is not a
new heuristic, propagator, placement bound or routing-semantic change.

`cargo fmt --all`, `cargo check --workspace` and `cargo test --workspace` pass
(526 tests passed, two pre-existing ignored). The accepted constructive-planner
design remains intact: validation is neutral, exact research is not invoked,
and production entry points have not changed.
