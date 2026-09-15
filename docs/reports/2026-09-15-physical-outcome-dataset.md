# Physical outcome dataset collection

The registered bounded collection completed all 96 configured procedural roots
on 2026-09-15. It admitted 64 training, 16 development and 16 confirmation
roots, collected 32 sequential physical edits from every root, and published
3,072 owned pre-action/outcome rows in 96 framed Postcard chunks. The run took
5.510 seconds and occupies 676,399,128 bytes. The retained dataset is local at
`artifacts/physical-outcomes-v1` and remains ignored because it is regenerable.

The source screen contains 48 roots at each of 8 and 16 blocks. All 96 source
hashes, structural identities and exact identities are distinct. Exact identity
resolution consumed two individualization work units for every root. The final
manifest reports every root as `collected` and no admission or collection
failure.

## Reproduction and validation

Build and collect from the repository root:

```sh
cargo build --release -p aic-cli --features outcomes
target/release/aic-cli learning collect-outcomes \
  --config docs/benchmarks/physical-outcomes/config.json \
  --catalog-root data/game/normalized \
  --output artifacts/physical-outcomes-v1
target/release/aic-cli learning validate-outcomes \
  --dataset artifacts/physical-outcomes-v1
```

The explicit full reload verified config, catalog, split, source and chunk
bindings; frame decoding; row order; selected roots and terminal-bound ports;
per-action and cumulative routing work; outcome-label masks; and sequential
continuation of unfinished drafts. It recovered 2,674 later pre-action rows from
an observed pending predecessor and reported `validated: true`.

The registered config SHA-256 is
`c1835d300d049e9d45049a324ffc2e6fed06638e89808afe8971fdbe0747179d` and
the catalog identity is
`6e483b5b903fe759008823f50dfe55d6e9a7b3d8d36978864fb292a76d5684de`.
The published compact metadata hashes are:

| File | SHA-256 |
| --- | --- |
| `manifest.json` | `11a49bcc051349e81b6a96be430b7ed7f1c3b6a76152a8f308c2a9fd9d1c4b3c` |
| `split-manifest.json` | `335febcb77895b6342122f8ef91e893e28ee46df867f43b59b65f3fe2da3a533` |
| `report.json` | `d9eb3ace304783a465f142b1b772af3ed259883831ed5210e9f9036f0d0632a4` |

## Coverage gate

Uniform family-first sampling produced 1,080 placement, 993 insert-band and 999
delete-band rows. Native outcomes were 2,775 `draft_accepted`, 145 `unchanged`,
131 `validated` and 21 `improved`. The report treats `validated` and `improved`
as observed complete outcomes and all other uncensored statuses as incomplete
for its binary coverage gate.

Three of the 18 size/split/family strata contain no complete outcome:

| Stratum | Incomplete | Complete |
| --- | ---: | ---: |
| 8 blocks / development / placement | 80 | 0 |
| 8 blocks / confirmation / placement | 97 | 0 |
| 16 blocks / confirmation / placement | 73 | 0 |

The dataset therefore passes collection, identity separation and reload gates,
but it does not support an accuracy claim across every registered stratum.
Training must report those three strata as single-class and must not use
confirmation labels for selection.
