# Preserve admitted route headings at facility ports

Restoring the unchanged historical 32-factory cohort exposed an admission error:
blueprint direction checks replaced the first or last original route segment
with the facility boundary normal. A legal turn in the outside connection cell
then failed `blueprint.route-direction`, including on a no-op original layout.

Preserve the admitted segment heading and rotate it with the facility's rotation
delta. Zero-length original edges retain their boundary-facing check. This agrees
with the independent Python witness audit's existing orientation rule. Exact
endpoints, identities, rates, directed topology and full physical validation stay
required. Production construction and global exact research are unaffected; this
is a correction to the approved blueprint-edit experiment, not a new pruning rule.
The constructive design remains applicable without modification.

The smallest original cohort witness is deliberately retained in
`crates/aic-pilot-native/tests/fixtures/blueprint-port-turn.json`; its neighboring
README documents provenance and why the full 6,347-byte fixture is necessary.
The regression checks original admission, bent approaches, quarter-turn heading
updates and a no-op commit exporting exactly the original validated witness.

Validation: `cargo fmt --all`, `cargo check --workspace`,
`cargo test --workspace` (564 passed, five ignored), release native build,
and CLI admission of all 32 byte-preserved historical factories passed.
