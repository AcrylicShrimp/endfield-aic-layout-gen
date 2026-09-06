# Medium battery: user reference versus existing generators

Date: 2026-09-06. Status: measured comparison complete; no optimizer changes.

## Contract and result

Compare the same runtime source plan, six batteries per minute, eight recipe instances,
and full utilization. Score only all-eight-facility states with every internal recipe
margin routed and exactly the six original external recipe margins still pending.
Power equipment is display-only. Do not add its footprint to the generator or remove
space from the user's bounding box. No screenshot geometry, pose hint or port assignment
is passed into a generator. No generator is tuned or optimized after seeing the reference.

The diagnostic `compare.mjs` reads frozen output reports and creates fresh independent
validation inputs; it does not call a constructor, router or optimizer. Map user labels
to wiring instances by recipe and instance index. Check exact inventory, recipe identity,
utilization, absence of actual external terminals in comparable states, and exact rational
pending external margins. Independent geometry validation must pass before scoring.
An incomplete growth state gets no comparable score. External-inclusive output is reported
separately. Any failed assertion or validator process returns `reference-comparison-invalid`
at stage `reference-comparison` with severity `error` and a specific message.

| Internal-only layout | Facilities | Used bounds | Area | Belt cells | Segment-only turns |
| --- | ---: | --- | ---: | ---: | ---: |
| User-confirmed reconstruction | 8 | 10x15 | 150 | 32 | 4 |
| Default production constructor | 8 | 27x36 | 972 | 140 | 13 |
| Existing experimental region constructor | 8 | 15x17 | 255 | 38 | 8 |

The user's layout is smaller on both area and belt cells than either tested complete
internal layout. The experimental result uses 1.70 times the area and 1.1875 times the
belt cells. The default constructor uses 6.48 times the area and 4.375 times the belt cells.
These are one-case observations, not global quality ratios, optimality gaps or an argument
for a new algorithm rule. The segment-only turn measure is the existing `witnessScore`
applied uniformly; do not compare its four user turns with the seven visual bends that
include facility-port arms in the reconstruction report.

## Completion and directionality controls

- Default `layouts construct-production` completes external wiring too. Its **full**
  footprint is 35x52, area 1,820, 223 belt cells and 20 segment-only turns. This is not
  a like-for-like score against the screenshot. Use the last internal-growth node for
  the table. Fresh validation of the full witness confirms no pending recipe margin.
- The existing `--downstream-closed-growth --flow-direction east
  --connection-first-placement` configuration stops at five of eight facilities after
  256 failed placement attempts. The retained state is valid, but three internal
  requirements remain unresolved. Its smaller partial footprint is not a quality win.
- The existing experimental `constructive-recipe-breadth/run.mjs` snapshot constructs
  all eight facilities and internal connections, then fails an originium-ore external
  input with `closure-port-template-exhausted`. The recorded failed edge has 216 port
  pairs, 219 accumulated search calls and 355 expanded states. These limits/search
  results are not global infeasibility. Use the retained internal region for the table;
  do not claim the experiment completed the external-inclusive factory.
- The experimental runner is still uncommitted work from the paused multi-recipe slice,
  not the production API and not a newly implemented improvement. Its report preserves
  that run's complete recursive source hashes. This comparison does not promote it.

## Visual and independent verification

After rebuilding current release `aic-cli` and `aic-witness-probe`, repeat both production
configurations and the unchanged experimental request. Default full geometry is exactly
identical to the pre-rebuild run. The production default is visibly sparse with long
detours and five bridge components; the experimental internal region is smaller, has no
logistics components, and retains the packager's west-facing input bank. The user reference
uses a north-facing packager and nests the five input lanes without bridges. These are
observations, not proof that packager rotation alone explains the quality difference.

Fresh Rust validation passes all five frozen witnesses (user, default internal,
experimental internal, directional partial, default full). All three comparable witnesses
contain the same eight recipes/utilizations and exactly the original five raw-input and
one product-output pending margins, with no internal input omitted. The user witness is
only identity-renamed; no coordinates, paths or quantities change.

Three native comparison pages pass six actual-browser checks each (1400/736/360 pixels,
light/dark, persistent focus/zoom/pan/Fit). All three full images were personally inspected.
The final exact-rational comparison rerun preserves all five witness hashes, so the
browser-inspected geometry matches the final report. Invalid CLI options reject explicitly.
Workspace check/tests and the 279-test Node suite passed during the reconstruction work
unit; the additional comparison CLI was executed successfully after its final code edit.

## Evidence and reproduction

Input request: `medium-battery-request.json`. Keep its explicit ore-to-ingot selection;
do not switch to recycled powder or import intermediate products.

Prepare wiring with `aic-cli recipes contextual-instance-wiring --file
data/game/normalized/recipes.json --request
docs/benchmarks/user-layout-reconstruction/medium-battery-request.json`.
Run the normal `layouts construct-production` command with that source plan, the five
runtime catalogs, and target instance `facility-instance:recipe-occurrence:/target:0`.
For the directionality control add the three flags above; all budgets otherwise remain
defaults. Feed the wiring output to the existing experimental region runner with the same
catalogs and witness validator. Do not pass the reference geometry into any of these runs.

Current evidence paths:

- `target/user-medium-battery-default-current.json` and corresponding HTML.
- `target/user-medium-battery-east-current.json` and corresponding HTML.
- `target/user-medium-battery-experimental-current/report.json` and its retained region.
- `target/user-medium-battery-comparison-verified/report.json`, containing input/witness
  hashes, five independently validated snapshots and comparable/noncomparable scopes.
- `target/user-medium-battery-comparison/*-browser/` for three browser reports and images.

Current release CLI SHA-256:
`32cd341f0a0a053711c25edee8cbcd0b7da4eee22d391041645b878d84721a12`.
Final comparison report SHA-256:
`313cb3d2d72cffa87e06ac2b885b7d31941a1f234bb1574436effe51f81e2973`.

```sh
node docs/benchmarks/user-layout-reconstruction/compare.mjs \
  --reference target/user-medium-battery-01/detail.json \
  --production target/user-medium-battery-default-current.json \
  --directional target/user-medium-battery-east-current.json \
  --experimental target/user-medium-battery-experimental-current/report.json \
  --wiring target/user-medium-battery-wiring.json \
  --recipes data/game/normalized/recipes.json \
  --facilities data/game/normalized/facilities.json \
  --items data/game/normalized/items.json \
  --transports data/game/normalized/transports.json \
  --components data/game/normalized/logistics-components.json \
  --validator target/release/aic-witness-probe \
  --output-directory target/user-medium-battery-comparison-replay
```

## Capacity observation

Under this request's recipes, each furnace, component processor and grinder produces
one item per two seconds at full utilization. The belt catalog also permits one item
per two seconds. Hence each of these full-rate outputs consumes one entire belt lane.
The packager needs two component lanes and three originium-powder lanes. A converger
cannot compress the three 30/min grinder outputs into one 30/min trunk while preserving
the 90/min total; at most two batteries per minute could be supplied by that trunk.
Other recipes or lower utilization can have different lane requirements. Do not encode
one-output-lane-per-facility as a universal game or planner rule.

Next: collect additional user-designed recipe cases, reconstruct and confirm each, and
repeat the same scope-matched baseline comparison before discussing optimizer changes.
