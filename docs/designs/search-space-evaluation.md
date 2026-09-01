# Search-Space Evaluation

## Purpose

Solver variable counts do not describe the same thing as the number of game-semantic candidate
layouts, and neither number describes the work that Pumpkin actually performs. AIC solver research
therefore records three separate layers instead of reporting one ambiguous "search-space size."

## AIC Search Profile

The canonical profile is:

```text
<L_sem^UB, L_model, N_var, N_constraint,
 D, B, C, L, P, T_build, T_search, T_first, outcome>
```

The fields have the following meanings:

- `L_sem^UB = log2(U_sem)` is the semantic state-space upper bound in equivalent binary choices.
  `U_sem` is defined separately for each experiment contract. It must count only decisions that
  are observable under that contract and must state which hard constraints have not yet been
  applied. For Rung 0, it is the Cartesian product of each facility's legal origin and distinct
  occupied-rectangle orientation choices before pairwise non-overlap.
- `L_model = sum(log2(|D_i|))` is the Cartesian domain volume of every declared solver variable.
  It includes auxiliary and channel variables, even when constraints functionally determine them.
  It is a formulation-size upper bound, not a count of independent legal assignments.
- `N_var` and `N_constraint` are the declared variable and posted-constraint counts.
- `D`, `B`, `C`, `L`, and `P` are observed branch decisions, backtracks, conflicts, learned clauses,
  and solver propagations.
- `T_build`, `T_search`, and `T_first` are model-construction time, total search time, and time to the
  first validated witness. A timeout without a witness is reported as a lower bound, never as
  infeasibility, and retains its `T_search` consumption and outcome.

Base-two logarithms are primary because one bit means one equivalent binary choice. Reports may
also show `log10(|Omega|) = L * log10(2)` as decimal orders of magnitude for readability. Large
spaces are written as `a x 10^b` or `2^L`, never as raw integers.

## Interpreting Reductions

For two exact formulations with identical semantics:

```text
Delta L_sem^UB = L_sem^UB(old) - L_sem^UB(new)
Delta L_model = L_model(old) - L_model(new)
```

- `Delta L_sem^UB = 0` is expected for a pure reformulation. A positive value is sound only when the
  removed states are proven semantic equivalences or invalid assignments.
- `2^(Delta L)` is the volume ratio. For example, `Delta L = 112` means a reduction of
  `2^112`, approximately `5.19 x 10^33`.
- A smaller `L_model` is useful evidence but does not by itself prove faster search. Actual `D`,
  `C`, `P`, and `T_first` remain authoritative.

Semantic upper bounds from different ladder rungs are comparable only when their `Omega_sem`
definitions are shown. Adding ports or routing adds new observable decisions, so the profile must
report component contributions rather than silently reusing the Rung 0 definition.

## Rung 0 Definition

For facility `i`, canvas `W x H`, and distinct fitting occupied rectangles `G_i`, the independent
candidate count is:

```text
A_i = sum over (w,h) in G_i of (W - w + 1)(H - h + 1)
|Omega_sem| <= U_sem = product over i of A_i
L_sem^UB = sum over i of log2(A_i)
```

The inequality is strict whenever pairwise non-overlap removes candidate tuples. The directional
rotation comparison uses the same formula but sums every allowed rotation separately, including
rotations with identical occupied rectangles. Their quotient measures exactly how much
contract-invisible rotational duplication was removed.
