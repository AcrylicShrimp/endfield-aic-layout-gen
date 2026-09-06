# Fair bounded coalescence exploration

Input is a complete validated witness, its normalized production graph, runtime
catalogs, and explicit candidate/pose budgets. Output is the existing schema-3
simultaneous translation portfolio with per-link and per-pair coverage diagnostics.
This changes proposal scheduling only; game semantics, movement, routing, validation
and lexicographic acceptance remain unchanged.

The former pair-major traversal spends its shared pose budget inside early pairs.
Replace it with round-robin sampling of pair pose iterators. Within each pair,
interleave legal relative-contact directions and diagonally traverse sorted X/Y
translation domains so both axes receive attention. Every yielded full translation,
including a body collision or out-of-bounds pose, consumes the common pose budget.
Preparing the finite domains and the later movement screen have separately bounded
work; they must be included in measured generation time.

Keep at most 64 body-feasible raw poses per pair, ranked by actual body envelope,
common translation distance and canonical move signature. Screen those through the
existing movement engine, retaining at most eight per pair. Final shortlist ranks
first by per-terminal-link proposal rank, then envelope and endpoint distance. A
large overlapping group family must not consume the entire final shortlist either.
All truncations are heuristic, not exact pruning or infeasibility evidence.

Reports must distinguish available links, sampled links, sampled pairs, pose-budget
termination, geometry screening and final selected links. Each sampled link records
its stable source/target terminal IDs, labels, pose count and shortlist count.
Deterministic repetition and input immutability are required. Unsupported bridge
release and movement diagnostics keep their existing meaning.

Compare the frozen prior pair-major report and current-source fair run from the same
incumbent, at the same 250,000-pose and 512-candidate budgets. Also retain independent
cone/group-exchange controls. Validate every promotion against the original graph,
inspect its native full and close-up rendering, and do not conflate wiring improvement
with resolution of the central area problem. No Pumpkin or Rust cutover in this slice.
