# Recipe layout dataset generator release evidence

Date: 2026-09-19

## Result

`aic-recipe-layout-gen` now generates complete recipe-derived factory layouts in
the bounded `recipe-packing-density-v3` domain. It preserves the selected recipe
semantics, exact material rates and external boundaries; samples a requested
packing profile; constructs shared capacity-aware belt and pipe networks with
catalog-backed crossings; independently validates the complete factory; and
publishes only validated layout witnesses.

The supported generator matrix is:

| Profile | Facility-body density | Maximum achieved facilities |
| --- | ---: | ---: |
| `compact` | `[0.12, 0.30)` | 8 |
| `balanced` | `[0.06, 0.12)` | 16 |
| `spacious` | `[0.02, 0.06)` | 32 |

These bounds describe measured generator support, not limits of the game. Both
the requested and achieved facility counts are checked. Exhausted bounded work
is reported as a failure and is never presented as global infeasibility.

## Integrated path

The command reuses the established recipe source-plan, contextual throughput,
facility expansion, instance wiring, fixed-factory constructor, independent
validator and HTML renderer. Catalogs and recipe indexes are loaded once per
process. A physical retry changes only placement and routing randomness; the
selected recipes, target rate, facility instances, wiring obligations and
external boundaries stay frozen.

For each placement, the fixed-factory constructor groups compatible flows into
capacity-sufficient commodity networks, assigns legal directional terminals,
routes belts and pipes, realizes catalog components and bridges, and validates
the final witness independently. Dense placements try the constrained-first
order first; lower-density placements try the seeded order first. Failed orders
and placements remain compact attempt summaries.

Batch mode accepts `--count`, `--workers` and a new `--output-dir`. One to four
persistent threads share immutable validated catalogs and indexes. Each slot uses
`initial_seed + index`, and its retry seeds derive from that slot seed. The
publication thread restores slot order, writes one report per slot, writes JSON
and HTML only for factory-validated witnesses, records the layout SHA-256, and
enforces a 16 MiB guard on individual reports and layouts.

Per-case reports use schema version 7, which adds the compact physical-attempt
history. Batch reports use schema version 1. A slot-level execution failure uses
the stable `recipe-layout-generation-error` code plus its message. Custom
research density bands leave `packing_policy_version` unset; only named profiles
claim the versioned v3 contract.

The default physical-attempt bound is 16. The CLI permits an explicit maximum of
128, while the supported release panel uses the default. There is no catalog
reload or output write inside a physical attempt.

## Final supported panel

The release-mode panel used the two target families
`item-proc-battery-5` and `item-xiranite-enr-powder`, seeds 100 through 103,
two persistent workers, 64 semantic starts, depth four, 200 A* calls per routing
order and the default 16 physical attempts. It covered every supported cell:

- compact: 8 facilities;
- balanced: 8 and 16 facilities; and
- spacious: 8, 16 and 32 facilities.

All **48 of 48** slots produced independently validated layouts. Forty-three
completed on the first physical attempt, 47 completed within eight attempts, and
the maximum was 11 attempts. The observed density range in every row stayed
inside its requested band.

| Profile | Complete | First attempt | Maximum attempts | Observed density | Batch wall time |
| --- | ---: | ---: | ---: | ---: | ---: |
| compact | 8 / 8 | 6 | 7 | 0.1250–0.2959 | 9.164 s |
| balanced | 16 / 16 | 15 | 2 | 0.0623–0.1196 | 2.536 s |
| spacious | 24 / 24 | 22 | 11 | 0.0256–0.0546 | 33.315 s |
| total | 48 / 48 | 43 | 11 | 0.0256–0.2959 | 45.015 s |

The twelve separately invoked batches produced 1.066 complete layouts/s in
aggregate, including semantic selection, placement, routing, independent
validation, JSON/HTML publication and batch summaries. Runtime is topology
dependent: the battery spacious-32 cell needed 31.271 seconds for four layouts,
while ten of the other eleven cells finished in less than two seconds. The panel
wrote 156 files totaling 73.6 MiB. These measurements support bounded production
use; they are not a promise that every recipe family or seed has the same yield.

Earlier boundary probes showed why the scale limits are graduated. With eight
physical attempts, compact 16 completed only 2/4 battery slots and balanced 32
completed 3/4 while taking 53 seconds. Spacious 32 remained attainable; the final
slot-seeded retry correction exposed one battery seed that required attempt 11.
Increasing the bounded default to 16 completed that case without changing its
semantic problem or density profile.

## Determinism and failure preservation

A separate balanced-8 battery run used seeds 100 through 103 with one worker and
then two workers. Seed 100 required two physical attempts. All four slot rows had
the same attempt count, achieved density and layout SHA-256 under both worker
counts. The one-worker and two-worker runs completed 4/4 in 1.919 and 1.921
seconds respectively. This exercises the retry path rather than comparing only
first-attempt successes.

A constructor exhaustion is a normal row with `factory_validated=false`; it does
not receive a layout or HTML file. A per-slot generation error is also retained
in its original row. The batch command does not resample or replace either case.

## Render inspection

Three current-policy witnesses were rendered to HTML and rasterized for manual
inspection:

| Representative | Density | Canvas | Networks | Transport cells | Components / bridges |
| --- | ---: | ---: | ---: | ---: | ---: |
| compact xiranite, 8 facilities | 0.1310 | 66 × 56 | 7 | 331 | 5 / 5 |
| balanced battery, 16 facilities | 0.0713 | 83 × 76 | 10 | 929 | 43 / 43 |
| spacious battery, 32 facilities | 0.0405 | 137 × 154 | 19 | 3,492 | 123 / 121 |

The compact render has distinct machine clusters joined by shared belt and pipe
trunks. The balanced render has a dense central bus structure, readable external
arms and legal visible crossings. The spacious render deliberately separates the
32 facilities and uses long shared orthogonal trunks; every facility, terminal
and direction marker remained visible. No body collision, disconnected terminal,
cropped geometry or renderer artifact was found. The visual density and extent
ordering is compact, then balanced, then spacious as intended.

## Comparison with public layouts

The [Talos Pioneers compact Xiranite blueprint](https://talospioneers.com/en/blueprints/01kfzdktgj623xgz6d16rbnjg2)
is a 23 × 27, self-sustaining, tileable example. The
[EndfieldTools community factory gallery](https://endfieldtools.dev/community-factories/)
shows player blueprints and a planner, while the
[Icy Veins 1.1 blueprint roundup](https://www.icy-veins.com/arknights-endfield/news/best-1-1-aic-factory-blueprints-for-arknights-endfield/)
collects community layouts that use tight rows and buses, sometimes split work
between an outpost and the main base or deliberately retain free room.

Those public examples are visual and operational references rather than an
equivalent benchmark. They usually optimize one hand-selected production loop
and may rely on storage, depot or base boundaries. The generated witnesses expose
every exact external input, target output and surplus output as a perimeter
terminal and do not minimize area after validity. This explains much of their
larger extent. The comparison still found the initial placement-only bands too
sparse; it motivated the denser v3 bands and the graduated profile limits used in
the final panel.

Generated compact layouts remain more open than the most aggressively hand-built
public examples. That is acceptable for this dataset contract: compact means the
measured `[0.12, 0.30)` body-density band with complete exact obligations, not a
claim of minimum area or human blueprint optimality.

## Reproduction

Build the official command and run one supported batch with:

```sh
cargo build --release -p aic-cli --bin aic-recipe-layout-gen
target/release/aic-recipe-layout-gen \
  --recipes data/game/normalized/recipes.json \
  --catalog-dir data/game/normalized \
  --target-item item-proc-battery-5 \
  --facilities 16 --packing-profile balanced \
  --seed 100 --attempts 64 --max-steps 64 --max-depth 4 \
  --construct-factory --factory-route-searches 200 \
  --count 4 --workers 2 \
  --output-dir target/recipe-balanced-16 \
  --report-output target/recipe-balanced-16-batch.json
```

The final panel outputs and rasterized QA files are regenerable and remain under
ignored `target/`. This report retains the compact findings and exact settings;
the repository does not commit the 73.6 MiB generated corpus.
