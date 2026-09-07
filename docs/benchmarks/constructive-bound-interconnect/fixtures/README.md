# Recipe-bound crossing regression input

`crossing-witness.json` is the independently selected 50 KB input to Rust witness-probe
tests, not a stored run report. It exercises recipe-bound fan-in/fan-out, crossing,
external obligations and negative mutations of those rules. Keeping one concrete
witness avoids making the validator's tests depend on the JavaScript generator.

It contains only the witness DTO: no validation results, timing, rendered output or
candidate history. It was extracted unchanged from the former `witness.json` at
`e31b97a`. Runtime catalog values remain external and are loaded by every test run.
The generator's complete per-run outputs are not committed.
