# Heavy Xiranite Bottom-Up Rung 0 Growth Report

## Scope

This experiment solves only facility geometry for cumulative Heavy Xiranite production-graph
phases. The solver decides facility coordinates and allowed rotations and proves pairwise footprint
non-overlap. It contains no port, terminal, belt, pipe, flow, logistics-component, objective, hint,
or transferred learned state.

Every case used the release build, a `50 x 50` request ceiling, cold feasibility search, and a
five-second search budget. The ceiling is a diagnostic request bound, not a required footprint or a
game limit. A witness can use any smaller region inside it.

## Growth Results

| Phase | Facilities | Outcome | Build | First witness | Variables | Constraints | Decisions | Conflicts | Propagations |
|---:|---:|---|---:|---:|---:|---:|---:|---:|---:|
| 3 | 4 | feasible | 1 ms | 1 ms | 412 | 556 | 110 | 11 | 6,567 |
| 10 | 11 | feasible | 9 ms | 5 ms | 3,597 | 4,609 | 128 | 7 | 39,581 |
| 20 | 21 | feasible | 37 ms | 177 ms | 13,587 | 17,199 | 2,937 | 526 | 1,044,528 |
| 30 | 33 | feasible | 103 ms | 1,349 ms | 34,023 | 42,867 | 12,297 | 1,926 | 6,900,732 |
| 34 | 47 | feasible | 221 ms | 3,713 ms | 69,513 | 87,373 | 27,407 | 4,476 | 15,943,878 |
| 35 | 48 | feasible | 247 ms | 4,931 ms | 72,528 | 91,152 | 29,644 | 5,825 | 18,108,883 |
| 36 | 49 | unknown | 242 ms | none in 5 s | 75,607 | 95,011 | 29,140 | 5,343 | 21,550,946 |
| 37 | 50 | unknown | 273 ms | none in 5 s | 78,750 | 98,950 | 29,130 | 5,263 | 17,361,163 |
| 39 | 56 | unknown | 358 ms | none in 5 s | 98,952 | 124,264 | 27,906 | 5,126 | 14,793,909 |
| 40 | 59 | unknown | 393 ms | none in 5 s | 109,917 | 138,001 | 25,645 | 3,688 | 18,783,847 |

`unknown` means that the resource-limited solve found no witness and proved no infeasibility. It is
not an infeasibility result.

## Boundary Repetition

The three boundary phases were run four times in fresh release processes.

| Phase | Facilities | Outcomes | First-witness times |
|---:|---:|---|---|
| 34 | 47 | 4/4 feasible | 3,713; 3,778; 3,851; 3,885 ms |
| 35 | 48 | 4/4 feasible | 4,931; 4,616; 4,610; 4,554 ms |
| 36 | 49 | 4/4 unknown | none within 5,000 ms |

All successful repetitions of a phase had identical decision and conflict counts. Phase 36 varied
slightly because the wall-clock cutoff interrupted the same deterministic search at different
instruction boundaries. The measured five-second first-witness cliff is therefore robust between
48 and 49 facilities for this solver, encoding, ceiling, hardware, and budget.

## First Structural Finding

Rung 0 already contains a quadratic rotation-expanded non-overlap block. At 49 facilities:

- every facility has four allowed rotations;
- every facility pair has `4 x 4 = 16` rotation pairs;
- every rotation pair creates four reified separation relations;
- `C(49, 2) x 16 x 4 = 75,264` reified separation relations;
- the complete model contains 75,607 variables and 95,011 constraints before ports or routing.

The next diagnosis starts above formulation. In a geometry-only rung, rotations that produce the
same footprint are observationally equivalent because ports are absent. A square facility's four
rotations all describe the same occupied rectangle; a rectangular facility has at most two distinct
geometry orientations. The baseline nevertheless asks Pumpkin to distinguish all four rotation
states and expands non-overlap across them.

The next controlled change will project full rotations onto distinct footprint-orientation classes
only in Rung 0. This is not a heuristic restriction: it preserves every facility-geometry witness
and removes only distinctions that the Rung 0 contract cannot observe. Full directional rotations
return in Rung 1 when facility ports make them semantically different.

## Artifacts

- `heavy-xiranite-bottom-up-rung0-growth/`: raw JSON and self-contained HTML for every growth and
  repetition case.
- `heavy-xiranite-bottom-up-rung0-full/`: full 59-facility case.
- `heavy-xiranite-bottom-up-rung0-phase3/`: initial four-facility 16 x 16 case.
