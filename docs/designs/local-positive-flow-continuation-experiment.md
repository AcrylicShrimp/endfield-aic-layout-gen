# Local Positive-Flow Continuation Experiment

## Purpose

Measure a stronger exact grid inference before allowing it to prune solver domains. The current
unique-support chain starts at selected demand terminals and walks backward. The proposed local
rule also recognizes positive material flow already entering or leaving any cell and asks whether
flow conservation leaves exactly one continuation.

The first slice is passive. It observes opportunities and estimated local explanation sizes but
does not post a predicate or conflict.

## Live-domain definitions

For material `m` and cell `g`:

```text
possible_in(m, g)  = incoming arcs whose selected and incident item domains still contain m
possible_out(m, g) = outgoing arcs whose selected and incident item domains still contain m

possible_supply(m, g) = supply terminal options at g whose selected domain still contains 1
possible_demand(m, g) = demand terminal options at g whose selected domain still contains 1
```

A positive incoming witness is either:

- a supply terminal at `g` with `selected = 1`; or
- an incoming arc at `g` with `selected = 1` and both incident item variables fixed to `m`.

A positive outgoing witness is the dual selected demand or fixed-material selected outgoing arc.
Arc selection is a positive-flow witness because the exact model enforces `selected <= flow` and
all flow variables are non-negative integers.

## Candidate exact rules

Forward continuation:

```text
positive incoming witness
and no possible local demand for m
and exactly one possible outgoing m arc
=> that outgoing arc is selected and carries m
```

Backward continuation is the dual:

```text
positive outgoing witness
and no possible local supply for m
and exactly one possible incoming m arc
=> that incoming arc is selected and carries m
```

If the corresponding possible-arc set is empty, the positive witness is inconsistent with exact
cell conservation. Multiple possible continuations produce no inference.

The rule does not require global terminal connectivity and does not prohibit circulation. The
passive MVP skips a cell whenever that cell's same-layer bridge can still be selected. This is a
conservative undercount: it prevents the analyzer from combining the two independent bridge axes
before an axis-aware proof and representation exist.

## Passive metrics

The analyzer records:

- executions and material passes;
- cells with positive incoming and outgoing witnesses;
- forward and backward continuation cells after opposing terminal options are excluded;
- repeated zero-support and unique-support observations in each direction;
- unresolved arc/item predicates on unique supports;
- distinct unique-support arcs; and
- skipped material/cell observations where a bridge remains possible; and
- estimated maximum local explanation predicates.

The explanation estimate contains one positive witness, every excluded opposing terminal option,
every excluded alternative arc predicate needed by the existing local-support reason builder, and
the bridge-selection exclusion when the cell has a same-layer bridge variable.
It is diagnostic only and cannot justify propagation in this slice.

## Acceptance criteria

- The analyzer posts no domain changes. A time-limited run may report only partial, repeated
  observations and analyzer overhead; it is not required to complete the same search tree as the
  watched-demand case.
- Controlled fixtures identify supply-rooted forward opportunities, demand-rooted backward
  opportunities, arc-rooted continuation, branch stopping, zero support, and circulation without
  changing domains.
- Phase 2 reports whether the rule finds residual opportunities after the accepted watched-demand
  chain, and separates forward opportunities from backward opportunities already covered by the
  current rule.
- Independent reviewers confirm the proof boundary before any active propagator is implemented.
