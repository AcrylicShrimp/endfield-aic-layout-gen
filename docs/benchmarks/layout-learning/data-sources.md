# Community blueprint data: availability audit

Inspected 2026-09-07. This is a bounded, read-only source/schema audit, not a bulk scrape or
permission to train on/repackage community content. Reproducible request URLs, HTTP status,
response hashes, field names and measured counts are in [source-results.json](../../../artifacts/benchmarks/layout-learning/source-results.json).
The experiment itself trains only on its own catalog-backed synthetic states.

## Outcome

Machine-readable **layout geometry exists**, including Chinese creators' designs. A large,
clean, independently validated and clearly reusable learning corpus has **not** been established.
The useful distinction is recipe tables versus blueprint metadata versus actual geometry.
Only the last provides direct placement/routing targets.

| Source | Directly observed | Practical assessment |
| --- | --- | --- |
| [IndustrialPlanner](https://github.com/hsyhhssyy/IndustrialPlanner) — Chinese project, [mainland site](https://endfield.amiyabot.com/) | Eight indexed JSON blueprints, all with entity position/rotation/configuration; 34–678 entities per document | Best immediately inspectable seed/reference format; not yet a large corpus |
| [EndfieldTools community factories](https://endfieldtools.dev/community-factories/) | Two of the first twelve all-time popular detail records contain `factory_data`; ten are screenshot-only | Real route/placement exports are obtainable for the simulated subset; total usable count and training permission remain unknown |
| [Talos Pioneers](https://talospioneers.com/en/blueprints) | API reports 1,669 entries; one list item and its detail contain codes, images, facilities/items, rates and optional dimensions, no entity geometry | Useful discovery/metadata index, not 1,669 ready-to-train layouts |
| [TapTap blueprint tool](https://www.taptap.cn/poster/NHxrgGW9gIsc) and [blueprint collection](https://www.taptap.cn/app/232326/strategy/entity-collection/359107) | Curated Chinese share-code/image workflow | Saves finding individual posts, but no coordinate export was verified |
| MSC Tencent spreadsheet, linked by the [creator's blueprint guide](https://www.bilibili.com/list/ml3944146783?bvid=BV1s76FBGEow&oid=115977756870355) | A consolidated [spreadsheet link](https://docs.qq.com/sheet/DUEh1aHpCVVd4ZUdh?tab=wovqgi) exists; direct content was not accessible in this audit | Promising creator-maintained collection, contents/size/export/license unverified; no access-control workaround attempted |

The twelve EndfieldTools entries are a popularity-selected sample, **not** a random sample.
Two out of twelve must not be extrapolated to the site's total geometry fraction or corpus size.
Likewise, failing to verify a public decoder/dataset in this search is not proof none exists.

## IndustrialPlanner: an actual geometry format

Pinned repository commit: `526cb81cc693b11310e88993c487f9354b5eeda0` (branch `v3`).
The [public index](https://github.com/hsyhhssyy/IndustrialPlanner/blob/526cb81cc693b11310e88993c487f9354b5eeda0/public/blueprints/index.json)
lists tutorial, capsule, Xiranite, battery, underground-pipe and power-cycling examples.
All eight retrieved documents use schema 5. The document has `entities`, `entityOrder`,
`slotLinks`, `baseId` and version metadata; entity records include `definitionId`, `position`,
`rotation` and `config`. See its
[blueprint contract](https://github.com/hsyhhssyy/IndustrialPlanner/blob/526cb81cc693b11310e88993c487f9354b5eeda0/src/domain/document/blueprint-document.ts)
and [world-entity contract](https://github.com/hsyhhssyy/IndustrialPlanner/blob/526cb81cc693b11310e88993c487f9354b5eeda0/src/domain/document/world-document.ts).

Measured entity counts, including logistics and other devices, are 34, 227, 148, 338, 137,
678, 188 and 162. They are **not facility counts**. For example, the
[Wuling battery example](https://github.com/hsyhhssyy/IndustrialPlanner/blob/526cb81cc693b11310e88993c487f9354b5eeda0/public/blueprints/wuling-battery-line.json)
contains 338 entities, 13 slot links and a Bilibili creator reference. The
[seven-oven Xiranite example](https://github.com/hsyhhssyy/IndustrialPlanner/blob/526cb81cc693b11310e88993c487f9354b5eeda0/public/blueprints/utimate-xiranite.json)
contains 678 entities. Ordinary adjacent transport connections are partly implicit in geometry;
`slotLinks` alone is not a complete production graph.

Six of the eight documents enable `ignoreStock` on some entities; some also provide initial
inventory, port priorities or underground-pipe configuration. No definition ID literally named
as a cheat/infinite device appears in this sample, but that string check is not a semantic
validity test. These files must be revalidated against our catalog, external-boundary and
transport rules before becoming training targets. Simulator validity is not our factory proof.

The current README says MIT, but the inspected commit has no root `LICENSE` file and GitHub
metadata did not supply a recognized license. In addition, several designs credit third-party
creators. Treat repository-code licensing and rights to creator/game content separately; obtain
clear permission for the intended dataset/model use. No upstream implementation was copied.

## EndfieldTools: query detail, not only the list

The shipped UI uses public GET requests with credentials omitted. The list endpoint is:

`https://endfieldtools.dev/api/cache/v1/factories/search?limit=12&orderBy=popular`

The detail endpoint takes the public record ID:

`https://endfieldtools.dev/api/cache/v1/factories/detail?id=<public-id>`

The two inspected simulation records are `26x27-all-parts-and-bottles` and
`double-hc-battery-full-grid`. Their detail responses contain respectively 62/97 building
records and 52/91 conveyor-line records. Buildings include coordinates, rotations, definition
IDs and optional recipe/output selections. Lines contain cell segments, layers and endpoint
building/port references. The other ten samples explicitly return `plan_type: screenshot_only`
and null geometry. This distinction was verified on the live detail responses, not guessed
from screenshots or frontend field names.

Public client assets used to trace the read contract:
[factory reader](https://endfieldtools.dev/_next/static/chunks/3957-4dbb2c4886fc7d2d.js),
[GET gateway](https://endfieldtools.dev/_next/static/chunks/7558-4171e73d3f0ad851.js).
Those build-specific URLs may change; response hashes and the source audit retain provenance.
No login, write/RPC action, view-counter increment, rate-limit bypass or private data access
was used. The existence of this endpoint does not establish permission for bulk training or
redistribution. A site/creator-approved export is the sensible next acquisition path.

## Talos Pioneers: useful metadata with explicit restrictions

`GET https://api.talospioneers.com/api/v1/blueprints?per_page=1` reports total 1,669 at inspection.
The one item and its detail match the public
[frontend blueprint DTO](https://github.com/Talos-Pioneers/ui/blob/cedec4e72c707051492cafe38cb06638eefaff72/app/models/blueprint.ts):
code, description, gallery, facility quantities, input/output quantities and optional width/height,
without per-entity positions, rotations or routes. A share-code string must not be mistaken for
a locally decodable geometry file; no verified public conversion path was found in this audit.

The [API terms](https://talospioneers.com/en/api-terms), dated February 26, permit personal
noncommercial access and tools adding functionality, require attribution, and restrict
monetization, mirroring and bulk redistribution. They reserve contributor/content ownership.
The frontend's AGPL license does not supersede those content terms. Model training and any
subsequent dataset/model distribution should be clarified with the operators and contributors;
this audit does not claim such permission.

## What this changes for the learning plan

The data-source problem is more promising than manually transcribing every forum post: both
structured community geometry and curated Chinese collections exist. Still, eight examples
and two sampled simulated layouts do not justify forecasting human-level training quality.

Start with permissioned native geometry, retain original creator/source/version and verify it.
Then derive bounded connected repair tasks and physically validated perturbations. Split by
original design/creator/topology before augmentation; related edits belong to the same split.
Popularity is a discovery signal, not a verified optimum label. Preserve failures, simulator
assumptions and throughput discrepancies rather than silently fixing them into success labels.

Other searched sources such as general Endfield game-table repositories, character databases
and automatic-layout projects supply useful catalogs or code, but do not by themselves provide
a corpus of strong human layouts. No scraper, importer or external-data training integration
is included in this milestone.
