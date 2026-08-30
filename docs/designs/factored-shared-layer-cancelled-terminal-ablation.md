# Factored Shared-Layer Cancelled-Terminal Ablation

## Status

Research-only confirmation of a previously proved dense-formulation dominance rule. Production
solver behavior remains unchanged in this checkpoint.

## Question

Does unrestricted grid topology for symbolically co-located, equal-and-opposite external/facility
terminal pairs account for the remaining factored shared-layer phase-zero search cost?

## Symbolic Precondition

The zero-arc case is allowed only when every selected logical requirement has exactly one external
endpoint and one facility endpoint. In the factored endpoint formulation both terminals reuse the
same facility placement-port geometry selector with opposite directions and equal flow units.
Their cell-wise contribution therefore cancels for every legal placement and port choice.

## Matrix

For both the hard Xiranite Powder network and the complete phase-zero graph, run:

1. the unchanged factored shared-layer baseline; and
2. a diagnostic case that fixes every grid arc in the selected symbolically cancelled networks to
   zero.

Each case receives an independent five-second release-mode budget. The zero-arc result is labelled
diagnostic-only even though the precondition supports an objective-preserving dominance proof.

## Decision

If zeroing arcs crosses the cliff, the first implementation action is to port the proved
cell-wise-cancellation dominance rule from the dense research result into the shared-layer exact
formulation. If it does not, continue decomposing terminal-presence and endpoint propagation before
changing production code.
