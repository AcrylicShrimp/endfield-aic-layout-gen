# Shared Transport Layer Experiment

## Status

Accepted experiment contract. This document defines a research formulation and does not replace
the production dense joint solver.

## Question

Can one exact physical belt grid and one exact physical pipe grid replace the current
commodity-network-per-grid representation without removing any legal placement, port assignment,
route, splitter, converger, or bridge decision?

## Scope

The first executable comparison uses cumulative SCC phase zero of the heavy xiranite minimum-rate
workload. Facility placement candidates and endpoint assignment variables remain unchanged so the
experiment isolates routing-state representation.

The dense solver remains the reference implementation. The shared-layer solver is available only
through a research command until equivalence and performance have been measured.

## Shared-Layer State

Each transport kind owns one physical grid. A physical directed grid arc has one selected variable
and one positive flow variable, regardless of the number of commodities. Each cell side has:

- an incoming-arm variable;
- an outgoing-arm variable; and
- an item identifier whose zero value means that the arm is absent.

Selected physical arcs connect equal non-zero item identifiers on opposite cell arms. Selected
facility or external terminal options assign their network item to the corresponding arm. A
non-bridge cell may carry only one item across all active arms. A bridge has independent horizontal
and vertical item identities and may therefore cross two different commodities.

Flow units retain the owning commodity network's exact scale. Conditional capacity constraints use
the selected item identifier. Scalar conservation is posted once per physical cell because every
non-bridge arm at that cell carries the same item. A selected bridge additionally enforces separate
horizontal and vertical conservation.

## Exact Semantics

The shared-layer model must preserve:

- joint facility placement, rotation, port assignment, and routing decisions;
- directional facility ports and their adjacent connection cells;
- belt and pipe as independent physical layers;
- one physical occupant per cell within each layer;
- same-item trunks, splits, and convergences;
- rotated splitter and converger topology and capacity;
- bridge crossings with one straight channel per axis;
- route circulation, matching the current production model;
- the lexicographic objective: used area, physical transport tiles, route turns, maximum side, and
  logistics component count.

No coordinate window, route corridor, path ordering, fixed placement, fixed port, or other
heuristic restriction is permitted.

## Objective Mapping

A shared-layer cell is physically occupied when any route arm is active. One occupied belt cell or
pipe cell contributes one physical transport tile. Facility and occupied transport cells jointly
define used geometry. Route turns use the same grid-arc definition as the dense reference model.
Selected splitter, converger, and bridge cells contribute one logistics component each.

## Validation

The experiment must provide:

1. small synthetic cases whose dense and shared formulations have the same feasibility and
   lexicographic objective;
2. independent witness validation through the existing integrated-layout validator;
3. dense-versus-shared model construction, variable, domain-volume, constraint, incidence,
   first-incumbent, search, objective, and termination metrics; and
4. automatic JSON and HTML artifacts even when either formulation returns no incumbent.

## Failure Modes

The experiment reports structured failure when:

- layer capacity units cannot safely represent a commodity network;
- a selected physical arc or terminal is not assigned a non-zero item;
- bridge or branch topology cannot be represented exactly;
- the solver reaches its resource limit without an incumbent; or
- the extracted witness fails the existing validator.

An experimental failure does not invoke the dense solver as a fallback and does not change the
production solve path.
