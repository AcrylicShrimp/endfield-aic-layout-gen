# Layered adjacent-swap geometry

Input: a nonempty bijection of concrete transport lane IDs with runtime catalog items, rates and
bridge definitions. Keep the existing stable same-item ordering and chosen adjacent-swap sequence.
Output: fully reserved swap envelopes, catalog bridge cells, unit directed lane paths, original
endpoint order, independent cell-check certificate and explicit stage schedule.

Swaps on disjoint adjacent position pairs commute. Preserve the original ordering of every pair of
swaps sharing a wire position. Assign each swap to one plus the latest stage touching either of its
positions. This longest-path schedule minimizes stage count for this fixed precedence graph, not
over all permutation decompositions or all legal geometries. Disjoint pairs in one stage have
nonoverlapping six-row envelopes at the existing three-cell pitch. Lower all swaps in a stage from
the same pre-stage order, then update that order together. Assert the final target permutation.

Keep the swap count, component arms/capacities, resource guards and independent cell checker intact.
No implicit crossings or new game rules are introduced. Empty permutations retain one straight
stage. Replace serial placement internally; no serial compatibility mode is required. Full factory
acceptance still requires independent Rust validation and exact original graph coverage. Retain the
previous complete factory until a complete candidate improves area, then transport tiles and turns.
