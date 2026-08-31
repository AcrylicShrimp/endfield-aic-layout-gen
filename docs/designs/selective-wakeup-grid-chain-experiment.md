# Selective-Wakeup Grid Chain Experiment

## Purpose

Retain the exact unique-support-chain inference and its search-tree reduction while avoiding
propagator executions caused by domain changes that cannot create a new unique support.

This is a diagnostic-only scheduling reformulation. It does not change the inference, feasible
set, objective, or solver decisions.

## Wake contract

A chain can gain a new forced predecessor only when one of these events occurs:

- a selected demand becomes required (`selected` lower bound rises);
- a possible route arc is excluded (`selected` upper bound falls);
- a route arm loses an item code (any integer-domain change is observed); or
- a possible local supply is excluded (`selected` upper bound falls).

Selecting a route arc by raising its lower bound cannot remove an alternative and therefore does
not need to wake this propagator. Assigning unrelated demand options to false also cannot create a
required chain.

Item variables retain broad integer-domain events because the required material code may be an
interior domain value rather than a current bound.

## Invariants

- The propagation algorithm and explanations are identical to the broad-wakeup chain case.
- The initial from-scratch propagation still runs.
- Every event that can remove a possible predecessor or supply is subscribed.
- Backtracking restores domains through Pumpkin's normal propagator scheduling.
- No placement, port, terminal, route, or material choice is fixed or removed heuristically.
- The normal joint solver path remains unchanged.

## Decision rule

Accept the scheduling variant only if:

- it reproduces the broad-wakeup chain's deterministic decisions, backtracks, conflicts, and
  learned clauses;
- it passes independent witness validation and Pumpkin reason checks; and
- repeated release runs improve elapsed time over both the broad-wakeup chain and terminal-only
  cases.

