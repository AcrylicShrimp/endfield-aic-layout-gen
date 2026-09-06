# Flexible ordinary terminal attachment

Status: bounded JavaScript research improvement over a complete validated factory.

Input adds endpointPolicy: retained | terminal-flexible (default retained). CLI uses
--endpoint-policy with the same values. Port identity, connection cell, placement,
commodity rates and external endpoint identity remain unchanged by this option.

Only an endpoint with exactly one logical terminal, exactly one incident chain and
no same-layer logistics component may release its old first/last segment direction.
All component, shared-terminal and branching anchors retain their old arms. A* body
and current-extent obstacles prevent entering the facility or leaving the boundary.
The chain remains a simple path. Other chains still reserve their constrained arms;
unconstrained endpoints reserve their occupied terminal cell but no arbitrary neighbor.

This matches the current semantic model: the facility arm points back into its body;
an ordinary transport cell has one input and one output with distinct arms, not a
straight-only requirement. See the mathematical model's facility-terminal semantics
and constructive/network/validation.rs flow-arm/topology checks. No game rule changes.

Output records endpoint policy and per-route first/last restrictions (null means
free). Compare retained versus flexible paths on the identical complete incumbent,
then combine with facility movement only if useful. Independent geometry and exact
graph coverage must pass before area/tiles/turns lexicographic promotion. A failure
or plateau is evidence only for this explicit neighborhood.

Test singleton terminal eligibility, shared/branch/component exclusion, frozen inputs,
preserved margins, actual changed terminal turns and whole-factory negative controls.
Inspect actual rendered output; shorter wires alone do not establish better placement.
