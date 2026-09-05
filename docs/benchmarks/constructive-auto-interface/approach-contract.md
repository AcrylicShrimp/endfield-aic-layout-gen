# Shared bend-column approach contract

Input: lanes with unique IDs and strictly increasing source and target rows, on two common
integer x faces. Output: the same ID-to-orthogonal-polyline map, without endpoint changes,
crossings, shared cells, extra components or item/rate reinterpretation. Inputs remain untouched.
All moving lanes use one interior vertical segment and eastbound horizontal segments. Straight
lanes need no bend. Invalid ordering or insufficient face separation returns the stable
`interface-approach-order` geometry diagnostic, not a factory infeasibility claim.

For two upward lanes in top-to-bottom order, overlapping closed vertical row intervals require
the upper lane's column to precede the lower lane's column. Reversing or equating them makes
the lower vertical segment intersect the upper source horizontal segment. This strict order is
also sufficient to avoid all pairwise intersections. Downward lanes have the reversed column
order, by reflection. Opposite-direction intervals cannot overlap when both endpoint orders
are strict. Straight lanes lie outside every other lane's vertical interval. Endpoint-touching
intervals count as overlapping because sharing even one cell is forbidden.

Process upward lanes top-to-bottom and downward lanes bottom-to-top. Assign a moving lane to
one plus the maximum column of preceding lanes with overlapping intervals, or column one
when none conflict. This is a longest-path ranking of necessary strict inequalities and attains
the minimum number of interior columns for this monotone single-vertical-segment family.
It does not prove minimum area over arbitrary routes or factory layouts. Face separation must
exceed this depth; existing callers may reserve more, with event-free strip compaction afterward.

Verification must enumerate unit cells independently and brute-force smaller column assignments
on small endpoint sets, including touching intervals, mixed directions and straight lanes.
Only independently validated complete-factory improvements replace the retained incumbent.
