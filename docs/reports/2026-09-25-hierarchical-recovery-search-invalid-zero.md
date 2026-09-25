# Recovery search screen: rejected exact-zero scale

Status: stopped before the full search matrix. The initial proposal to set
Gumbel `value_scale` to exactly zero violated the existing CLI configuration
contract. The valid near-zero scale `0.001` replaced it in the revised
[protocol](../designs/2026-09-25-hierarchical-recovery-research.md) before
its first search. No search or learner setting was adopted from this attempt.

The intended screen used the prior causal study's three 16-case panels,
generation-87 model checkpoint, 128 simulations per move, and three root
seeds. The initial source code identity was commit `0ec3b599`, with release
binary SHA-256
`e594b5425074a03731524d8c91991126232d840ef44a86e8b0630e1f16800fdf`.
The prepared protocol SHA-256 was
`21d4888f87b52c77bcfc01bf203af610d069fa083664af8474bb33d8f41be1c1`.
There was no uncommitted training or search patch.

The first two valid S1/seed-20260922 cells completed 6/16 cases in 56.37 s
for `base` and 5/16 in 60.47 s for `q02`. Their report SHA-256 values were
`6bbf87af319620f3bd37ec2a01cb328897730474b32b7e0f52b05e2f776baa66`
and `e846acd64730d67ff2bfc1c7846081a2d33b9e5a94fb469da41cb3db83e83345`.
The `q0` cell made no output report; the CLI emitted diagnostic code
`aic-learning.hierarchical-config` with message `invalid Gumbel root controls`
for its search configuration. The progress SHA-256 at the stop was
`2d045e59488d892eb3058c4b1809f088ae12ec51a3823a9796b13815a69a9`.
These two valid cells are too small to judge either hypothesis.

The revised v2 root regenerated and byte-compared all three source panels and
the two valid configurations, then copied the two reports with matching
SHA-256 and wall times. The revised screen uses `q001` for scale `0.001`.
Reproduce the stopped attempt using the historical source commit:

```bash
cd ~/Srv/aic-target-causal-20260925
bash tools/factory-edit-pilot/prepare_hierarchical_target_quality.sh \
  target/release/aic-cli artifacts/recovery-search
python3 tools/factory-edit-pilot/hierarchical_recovery_search.py prepare \
  --output artifacts/recovery-search --experiment-commit 0ec3b599
python3 tools/factory-edit-pilot/hierarchical_recovery_search.py run \
  --output artifacts/recovery-search
```

The exact-zero control failure is a configuration validation result, not
evidence about policy performance. The cleanup manifest is recorded alongside
the revised screen closeout after verifying the migrated report hashes.
