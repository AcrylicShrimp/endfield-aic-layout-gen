use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::sync::atomic::{AtomicUsize, Ordering as AtomicOrdering};

use crate::facilities::{FacilityDefinition, FacilityPortDirection, ValidatedFacilityCatalog};
use crate::layouts::{
    FacilityPlacement, FacilityPlacementBounds, FacilityPlacementReport, FacilityPlacementRequest,
    FacilityPlacementStatus, PlacedFacilityPort, SUPPORTED_FACILITY_PLACEMENT_SCHEMA_VERSION,
    TransportNetwork, TransportNetworkEndpoint, TransportNetworkSegment, TransportNetworkTerminal,
    WorldGridPosition, project_facility_ports,
};
use crate::logistics::{TransportKind, ValidatedItemCatalog};
use crate::recipes::{
    FacilityInstanceWiringEdge, FacilityInstanceWiringNode, FacilityInstanceWiringReport,
};

use super::first_pipe_frontier::{
    FacilityInstance, bounds_for, candidate_ports, occupied_cells, placement_candidates,
    rectangles_overlap, validate_inputs,
};
use super::routing::{count_turns, route_shortest_path};
use super::{
    CONSTRUCTIVE_FRONTIER_GROWTH_SCHEMA_VERSION, ConstructionCandidateScore,
    ConstructiveFrontierDiagnostic, ConstructiveFrontierGrowthPhase,
    ConstructiveFrontierGrowthReport, ConstructiveFrontierGrowthStatistics,
    ConstructiveFrontierGrowthStatus, ConstructiveFrontierStatistics,
};

#[derive(Clone)]
struct GrowthEdge<'a> {
    edge: &'a FacilityInstanceWiringEdge,
    source: FacilityInstance,
    target: FacilityInstance,
    transport: TransportKind,
}

#[derive(Clone, Default)]
struct LayoutState {
    placements: Vec<FacilityPlacement>,
    transport_networks: Vec<TransportNetwork>,
    used_ports: BTreeSet<(String, String)>,
}

struct Candidate {
    state: LayoutState,
    source_port: PlacedFacilityPort,
    target_port: PlacedFacilityPort,
    score: ConstructionCandidateScore,
    order: CandidateOrder,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct CandidateOrder {
    target_state: usize,
    source: usize,
    source_port: usize,
    target_port: usize,
}

#[derive(Default)]
struct WorkerOutcome {
    best: Option<Candidate>,
    statistics: WorkerStatistics,
    workers: usize,
}

#[derive(Debug, Clone, Copy, Default)]
struct WorkerStatistics {
    placement_candidates: u64,
    overlaps: u64,
    port_pairs: u64,
    blocked_port_pairs: u64,
    future_port_dead_ends: u64,
    astar_searches: u64,
    astar_failures: u64,
    valid_candidates: u64,
    placement_area_bound_pruned: u64,
    endpoint_area_bound_pruned: u64,
    route_cache_hits: u64,
}

pub fn construct_frontier_growth(
    wiring: &FacilityInstanceWiringReport,
    facilities: &ValidatedFacilityCatalog,
    items: &ValidatedItemCatalog,
    belt_frontier_depth: usize,
) -> ConstructiveFrontierGrowthReport {
    if let Some(diagnostic) = validate_inputs(wiring) {
        return failure(
            ConstructiveFrontierGrowthStatus::InvalidInput,
            Vec::new(),
            ConstructiveFrontierGrowthStatistics::default(),
            diagnostic,
            belt_frontier_depth,
        );
    }
    let growth = match select_initial_frontier_growth(wiring, items, belt_frontier_depth) {
        Ok(growth) if !growth.is_empty() => growth,
        Ok(_) => {
            return failure(
                ConstructiveFrontierGrowthStatus::NoEligibleFrontier,
                Vec::new(),
                ConstructiveFrontierGrowthStatistics::default(),
                ConstructiveFrontierDiagnostic::error(
                    "no-eligible-frontier-growth",
                    "/edges",
                    None,
                    "instance wiring contains no eligible facility-to-facility transport frontier",
                ),
                belt_frontier_depth,
            );
        }
        Err(diagnostic) => {
            return failure(
                ConstructiveFrontierGrowthStatus::InvalidInput,
                Vec::new(),
                ConstructiveFrontierGrowthStatistics::default(),
                diagnostic,
                belt_frontier_depth,
            );
        }
    };
    let request = match growth_canvas(&growth, facilities) {
        Ok(request) => request,
        Err(diagnostic) => {
            return failure(
                ConstructiveFrontierGrowthStatus::InvalidInput,
                Vec::new(),
                ConstructiveFrontierGrowthStatistics::default(),
                diagnostic,
                belt_frontier_depth,
            );
        }
    };
    let mut aggregate = ConstructiveFrontierGrowthStatistics {
        selected_requirements: growth.len(),
        ..ConstructiveFrontierGrowthStatistics::default()
    };
    let mut phases = Vec::with_capacity(growth.len());
    let mut state = LayoutState::default();

    for (index, growth_edge) in growth.iter().enumerate() {
        let target_states = if state.placements.is_empty() {
            let Some(definition) = facilities.facility(&growth_edge.target.facility) else {
                return failure(
                    ConstructiveFrontierGrowthStatus::InvalidInput,
                    phases,
                    aggregate,
                    missing_facility(&growth_edge.target),
                    belt_frontier_depth,
                );
            };
            seed_candidates(&growth_edge.target, definition, &request)
                .into_iter()
                .map(|placement| LayoutState {
                    placements: vec![placement],
                    ..LayoutState::default()
                })
                .collect::<Vec<_>>()
        } else {
            vec![state.clone()]
        };

        let Some(source_definition) = facilities.facility(&growth_edge.source.facility) else {
            return failure(
                ConstructiveFrontierGrowthStatus::InvalidInput,
                phases,
                aggregate,
                missing_facility(&growth_edge.source),
                belt_frontier_depth,
            );
        };
        let mut phase_statistics = ConstructiveFrontierStatistics::default();
        phase_statistics.seed_placements_considered = if index == 0 {
            target_states.len() as u64
        } else {
            0
        };
        let mut best: Option<Candidate> = None;
        let best_area = AtomicUsize::new(usize::MAX);
        for (target_state_index, target_state) in target_states.into_iter().enumerate() {
            let Some(target) = target_state
                .placements
                .iter()
                .find(|placement| placement.instance == growth_edge.target.id)
            else {
                continue;
            };
            let source_candidates = placement_candidates(
                &growth_edge.source,
                source_definition,
                &request,
                Some(target),
            );
            let outcome = evaluate_source_candidates_parallel(
                target_state_index,
                &target_state,
                &source_candidates,
                growth_edge,
                &growth[index + 1..],
                facilities,
                &request,
                &best_area,
            );
            apply_worker_statistics(&mut phase_statistics, &mut aggregate, outcome.statistics);
            aggregate.parallel_workers_peak = aggregate.parallel_workers_peak.max(outcome.workers);
            if let Some(candidate) = outcome.best
                && best
                    .as_ref()
                    .is_none_or(|current| candidate_is_better(&candidate, current))
            {
                best = Some(candidate);
            }
        }

        let Some(candidate) = best else {
            aggregate.completed_requirements = phases.len();
            return failure(
                ConstructiveFrontierGrowthStatus::Exhausted,
                phases,
                aggregate,
                ConstructiveFrontierDiagnostic::error(
                    "frontier-growth-exhausted",
                    format!("/phases/{index}"),
                    Some(growth_edge.edge.id.clone()),
                    format!(
                        "constructive frontier growth exhausted local placement, port, and route candidates for requirement '{}'",
                        growth_edge.edge.id
                    ),
                ),
                belt_frontier_depth,
            );
        };
        phase_statistics.accepted_path_tiles = candidate
            .state
            .transport_networks
            .last()
            .map_or(0, |network| network.cells.len());
        phase_statistics.accepted_path_turns = candidate
            .state
            .transport_networks
            .last()
            .map_or(0, |network| count_turns(&network.cells));
        state = candidate.state;
        let (placements, networks, source_port, target_port, bounds) =
            canonical_snapshot(&state, candidate.source_port, candidate.target_port);
        phases.push(ConstructiveFrontierGrowthPhase {
            index,
            requirement: growth_edge.edge.id.clone(),
            item: growth_edge.edge.item.clone(),
            rate: growth_edge.edge.rate,
            introduced_facility: growth_edge.source.id.clone(),
            bounds,
            placements,
            transport_networks: networks,
            source_port,
            target_port,
            score: candidate.score,
            statistics: phase_statistics,
        });
        aggregate.completed_requirements = phases.len();
    }

    let final_phase = phases
        .last()
        .expect("a non-empty selected chain completes at least one phase");
    ConstructiveFrontierGrowthReport {
        schema_version: CONSTRUCTIVE_FRONTIER_GROWTH_SCHEMA_VERSION,
        requested_belt_frontier_depth: belt_frontier_depth,
        success: true,
        status: ConstructiveFrontierGrowthStatus::Constructed,
        bounds: Some(final_phase.bounds.clone()),
        placements: final_phase.placements.clone(),
        transport_networks: final_phase.transport_networks.clone(),
        phases,
        statistics: aggregate,
        diagnostics: vec![ConstructiveFrontierDiagnostic::info(
            "frontier-growth-constructed",
            "constructed an initial pipe chain and its immediate belt suppliers as validated routed frontier transactions",
        )],
    }
}

#[allow(clippy::too_many_arguments)]
fn evaluate_source_candidates_parallel(
    target_state_index: usize,
    target_state: &LayoutState,
    source_candidates: &[FacilityPlacement],
    growth_edge: &GrowthEdge<'_>,
    remaining: &[GrowthEdge<'_>],
    facilities: &ValidatedFacilityCatalog,
    request: &FacilityPlacementRequest,
    best_area: &AtomicUsize,
) -> WorkerOutcome {
    if source_candidates.is_empty() {
        return WorkerOutcome::default();
    }
    let workers = std::thread::available_parallelism()
        .map_or(1, std::num::NonZeroUsize::get)
        .min(source_candidates.len());
    let chunk_size = source_candidates.len().div_ceil(workers);
    let outcomes = std::thread::scope(|scope| {
        let handles = source_candidates
            .chunks(chunk_size)
            .enumerate()
            .map(|(chunk_index, chunk)| {
                let source_offset = chunk_index * chunk_size;
                scope.spawn(move || {
                    let mut outcome = WorkerOutcome {
                        workers: 1,
                        ..WorkerOutcome::default()
                    };
                    for (offset, source) in chunk.iter().enumerate() {
                        let candidate_outcome = evaluate_source_candidate(
                            CandidateOrder {
                                target_state: target_state_index,
                                source: source_offset + offset,
                                source_port: 0,
                                target_port: 0,
                            },
                            target_state,
                            source,
                            growth_edge,
                            remaining,
                            facilities,
                            request,
                            best_area,
                        );
                        merge_worker_outcome(&mut outcome, candidate_outcome);
                    }
                    outcome
                })
            })
            .collect::<Vec<_>>();
        handles
            .into_iter()
            .map(|handle| handle.join().expect("constructive route worker panicked"))
            .collect::<Vec<_>>()
    });

    let mut combined = WorkerOutcome {
        workers: outcomes.len(),
        ..WorkerOutcome::default()
    };
    for outcome in outcomes {
        merge_worker_outcome(&mut combined, outcome);
    }
    combined.workers = workers;
    combined
}

#[allow(clippy::too_many_arguments)]
fn evaluate_source_candidate(
    order: CandidateOrder,
    target_state: &LayoutState,
    source: &FacilityPlacement,
    growth_edge: &GrowthEdge<'_>,
    remaining: &[GrowthEdge<'_>],
    facilities: &ValidatedFacilityCatalog,
    request: &FacilityPlacementRequest,
    best_area: &AtomicUsize,
) -> WorkerOutcome {
    let mut outcome = WorkerOutcome::default();
    outcome.statistics.placement_candidates = 1;
    if target_state
        .placements
        .iter()
        .any(|placed| rectangles_overlap(source, placed))
    {
        outcome.statistics.overlaps = 1;
        return outcome;
    }
    let mut placements = target_state.placements.clone();
    placements.push(source.clone());
    if used_area_lower_bound(&placements, &target_state.transport_networks, &[])
        > best_area.load(AtomicOrdering::Relaxed)
    {
        outcome.statistics.placement_area_bound_pruned = 1;
        return outcome;
    }
    let Some((source_ports, target_ports)) = candidate_ports(
        &placements,
        facilities,
        request,
        &growth_edge.source.id,
        &growth_edge.target.id,
        growth_edge.transport,
    ) else {
        return outcome;
    };
    let mut blocked = occupied_cells(&placements);
    blocked.extend(transport_cells_for_kind(
        &target_state.transport_networks,
        growth_edge.transport,
    ));
    let mut route_cache = HashMap::<(i64, i64, i64, i64), Option<Vec<WorldGridPosition>>>::new();
    for (source_port_index, source_port) in source_ports.into_iter().enumerate() {
        if target_state
            .used_ports
            .contains(&(source_port.instance.clone(), source_port.port.clone()))
        {
            continue;
        }
        for (target_port_index, target_port) in target_ports.iter().enumerate() {
            if target_state
                .used_ports
                .contains(&(target_port.instance.clone(), target_port.port.clone()))
            {
                continue;
            }
            outcome.statistics.port_pairs += 1;
            if blocked.contains(&(source_port.connection.x, source_port.connection.y))
                || blocked.contains(&(target_port.connection.x, target_port.connection.y))
            {
                outcome.statistics.blocked_port_pairs += 1;
                continue;
            }
            if used_area_lower_bound(
                &placements,
                &target_state.transport_networks,
                &[&source_port.connection, &target_port.connection],
            ) > best_area.load(AtomicOrdering::Relaxed)
            {
                outcome.statistics.endpoint_area_bound_pruned += 1;
                continue;
            }
            let route_key = (
                source_port.connection.x,
                source_port.connection.y,
                target_port.connection.x,
                target_port.connection.y,
            );
            let path = if let Some(cached) = route_cache.get(&route_key) {
                outcome.statistics.route_cache_hits += 1;
                cached.clone()
            } else {
                outcome.statistics.astar_searches += 1;
                let routed = route_shortest_path(
                    request.max_width,
                    request.max_height,
                    &blocked,
                    &source_port.connection,
                    &target_port.connection,
                );
                if routed.is_none() {
                    outcome.statistics.astar_failures += 1;
                }
                route_cache.insert(route_key, routed.clone());
                routed
            };
            let Some(path) = path else {
                continue;
            };
            let network = network_for(
                growth_edge.edge,
                growth_edge.transport,
                &source_port,
                target_port,
                path,
            );
            let mut candidate_state = target_state.clone();
            candidate_state.placements = placements.clone();
            candidate_state.transport_networks.push(network);
            candidate_state
                .used_ports
                .insert((source_port.instance.clone(), source_port.port.clone()));
            candidate_state
                .used_ports
                .insert((target_port.instance.clone(), target_port.port.clone()));
            if !validate_state(&candidate_state) {
                continue;
            }
            let Some(blocked_future_port_options) =
                future_port_loss(&candidate_state, remaining, facilities, request)
            else {
                outcome.statistics.future_port_dead_ends += 1;
                continue;
            };
            let score = score(&candidate_state, blocked_future_port_options);
            outcome.statistics.valid_candidates += 1;
            best_area.fetch_min(score.used_bounding_box_area, AtomicOrdering::Relaxed);
            let candidate = Candidate {
                state: candidate_state,
                source_port: source_port.clone(),
                target_port: target_port.clone(),
                score,
                order: CandidateOrder {
                    source_port: source_port_index,
                    target_port: target_port_index,
                    ..order
                },
            };
            if outcome
                .best
                .as_ref()
                .is_none_or(|current| candidate_is_better(&candidate, current))
            {
                outcome.best = Some(candidate);
            }
        }
    }
    outcome
}

fn candidate_is_better(candidate: &Candidate, current: &Candidate) -> bool {
    (candidate.score, candidate.order) < (current.score, current.order)
}

fn merge_worker_outcome(combined: &mut WorkerOutcome, outcome: WorkerOutcome) {
    if let Some(candidate) = outcome.best
        && combined
            .best
            .as_ref()
            .is_none_or(|current| candidate_is_better(&candidate, current))
    {
        combined.best = Some(candidate);
    }
    let target = &mut combined.statistics;
    let source = outcome.statistics;
    target.placement_candidates += source.placement_candidates;
    target.overlaps += source.overlaps;
    target.port_pairs += source.port_pairs;
    target.blocked_port_pairs += source.blocked_port_pairs;
    target.future_port_dead_ends += source.future_port_dead_ends;
    target.astar_searches += source.astar_searches;
    target.astar_failures += source.astar_failures;
    target.valid_candidates += source.valid_candidates;
    target.placement_area_bound_pruned += source.placement_area_bound_pruned;
    target.endpoint_area_bound_pruned += source.endpoint_area_bound_pruned;
    target.route_cache_hits += source.route_cache_hits;
}

fn apply_worker_statistics(
    phase: &mut ConstructiveFrontierStatistics,
    aggregate: &mut ConstructiveFrontierGrowthStatistics,
    statistics: WorkerStatistics,
) {
    phase.supplier_placements_considered += statistics.placement_candidates;
    phase.overlapping_placements_rejected += statistics.overlaps;
    phase.port_pairs_considered += statistics.port_pairs;
    phase.blocked_port_pairs_rejected += statistics.blocked_port_pairs;
    phase.astar_searches += statistics.astar_searches;
    phase.astar_failures += statistics.astar_failures;
    aggregate.placement_candidates_considered += statistics.placement_candidates;
    aggregate.overlapping_placements_rejected += statistics.overlaps;
    aggregate.port_pairs_considered += statistics.port_pairs;
    aggregate.blocked_port_pairs_rejected += statistics.blocked_port_pairs;
    aggregate.future_port_dead_ends_rejected += statistics.future_port_dead_ends;
    aggregate.astar_searches += statistics.astar_searches;
    aggregate.astar_failures += statistics.astar_failures;
    aggregate.valid_candidates_scored += statistics.valid_candidates;
    aggregate.placement_area_bound_pruned += statistics.placement_area_bound_pruned;
    aggregate.endpoint_area_bound_pruned += statistics.endpoint_area_bound_pruned;
    aggregate.route_cache_hits += statistics.route_cache_hits;
}

fn failure(
    status: ConstructiveFrontierGrowthStatus,
    phases: Vec<ConstructiveFrontierGrowthPhase>,
    statistics: ConstructiveFrontierGrowthStatistics,
    diagnostic: ConstructiveFrontierDiagnostic,
    requested_belt_frontier_depth: usize,
) -> ConstructiveFrontierGrowthReport {
    let final_phase = phases.last();
    ConstructiveFrontierGrowthReport {
        schema_version: CONSTRUCTIVE_FRONTIER_GROWTH_SCHEMA_VERSION,
        requested_belt_frontier_depth,
        success: false,
        status,
        bounds: final_phase.map(|phase| phase.bounds.clone()),
        placements: final_phase.map_or_else(Vec::new, |phase| phase.placements.clone()),
        transport_networks: final_phase
            .map_or_else(Vec::new, |phase| phase.transport_networks.clone()),
        phases,
        statistics,
        diagnostics: vec![diagnostic],
    }
}

fn select_longest_linear_pipe_chain<'a>(
    wiring: &'a FacilityInstanceWiringReport,
    items: &ValidatedItemCatalog,
) -> Result<Vec<GrowthEdge<'a>>, ConstructiveFrontierDiagnostic> {
    let instances = wiring
        .nodes
        .iter()
        .filter_map(|node| match node {
            FacilityInstanceWiringNode::Facility {
                id,
                recipe,
                facility,
                ..
            } => Some((
                id.as_str(),
                FacilityInstance {
                    id: id.clone(),
                    recipe: recipe.clone(),
                    facility: facility.clone(),
                },
            )),
            _ => None,
        })
        .collect::<BTreeMap<_, _>>();
    let mut eligible = Vec::new();
    for edge in &wiring.edges {
        let Some(item) = items.item(&edge.item) else {
            return Err(ConstructiveFrontierDiagnostic::error(
                "missing-item-definition",
                "/edges",
                Some(edge.item.clone()),
                format!("wiring edge references missing item '{}'", edge.item),
            ));
        };
        if item.transport != TransportKind::Pipe {
            continue;
        }
        let (Some(source), Some(target)) = (
            instances.get(edge.source.as_str()),
            instances.get(edge.target.as_str()),
        ) else {
            continue;
        };
        eligible.push(GrowthEdge {
            edge,
            source: source.clone(),
            target: target.clone(),
            transport: TransportKind::Pipe,
        });
    }
    eligible.sort_by(|left, right| left.edge.id.cmp(&right.edge.id));
    let mut incoming = BTreeMap::<&str, Vec<usize>>::new();
    let mut sources = BTreeSet::new();
    for (index, edge) in eligible.iter().enumerate() {
        incoming
            .entry(edge.target.id.as_str())
            .or_default()
            .push(index);
        sources.insert(edge.source.id.as_str());
    }
    let mut sinks = eligible
        .iter()
        .map(|edge| edge.target.id.as_str())
        .filter(|target| !sources.contains(target))
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    sinks.sort();
    let mut best = Vec::new();
    for sink in sinks {
        let mut current = sink;
        let mut path = Vec::new();
        let mut seen = BTreeSet::new();
        loop {
            let Some(edges) = incoming.get(current) else {
                break;
            };
            if edges.len() != 1 || !seen.insert(edges[0]) {
                break;
            }
            let edge = &eligible[edges[0]];
            path.push(edge.clone());
            current = edge.source.id.as_str();
        }
        if path.len() > best.len() {
            best = path;
        }
    }
    Ok(best)
}

fn select_initial_frontier_growth<'a>(
    wiring: &'a FacilityInstanceWiringReport,
    items: &ValidatedItemCatalog,
    belt_frontier_depth: usize,
) -> Result<Vec<GrowthEdge<'a>>, ConstructiveFrontierDiagnostic> {
    let mut growth = select_longest_linear_pipe_chain(wiring, items)?;
    let instances = wiring
        .nodes
        .iter()
        .filter_map(|node| match node {
            FacilityInstanceWiringNode::Facility {
                id,
                recipe,
                facility,
                ..
            } => Some((
                id.as_str(),
                FacilityInstance {
                    id: id.clone(),
                    recipe: recipe.clone(),
                    facility: facility.clone(),
                },
            )),
            _ => None,
        })
        .collect::<BTreeMap<_, _>>();
    let mut selected_instances = growth
        .iter()
        .flat_map(|edge| [edge.source.id.clone(), edge.target.id.clone()])
        .collect::<BTreeSet<_>>();
    for _ in 0..belt_frontier_depth {
        let mut belt_frontiers = Vec::new();
        for edge in &wiring.edges {
            let Some(item) = items.item(&edge.item) else {
                return Err(ConstructiveFrontierDiagnostic::error(
                    "missing-item-definition",
                    "/edges",
                    Some(edge.item.clone()),
                    format!("wiring edge references missing item '{}'", edge.item),
                ));
            };
            if item.transport != TransportKind::Belt
                || !selected_instances.contains(&edge.target)
                || selected_instances.contains(&edge.source)
            {
                continue;
            }
            let (Some(source), Some(target)) = (
                instances.get(edge.source.as_str()),
                instances.get(edge.target.as_str()),
            ) else {
                continue;
            };
            belt_frontiers.push(GrowthEdge {
                edge,
                source: source.clone(),
                target: target.clone(),
                transport: TransportKind::Belt,
            });
        }
        belt_frontiers.sort_by(|left, right| left.edge.id.cmp(&right.edge.id));
        if belt_frontiers.is_empty() {
            break;
        }
        for frontier in belt_frontiers {
            if selected_instances.insert(frontier.source.id.clone()) {
                growth.push(frontier);
            }
        }
    }
    Ok(growth)
}

fn growth_canvas(
    growth: &[GrowthEdge<'_>],
    facilities: &ValidatedFacilityCatalog,
) -> Result<FacilityPlacementRequest, ConstructiveFrontierDiagnostic> {
    let mut instances = BTreeMap::new();
    for edge in growth {
        instances.insert(edge.source.id.as_str(), &edge.source);
        instances.insert(edge.target.id.as_str(), &edge.target);
    }
    let mut extent = 8;
    for instance in instances.values() {
        let Some(definition) = facilities.facility(&instance.facility) else {
            return Err(missing_facility(instance));
        };
        extent += definition.footprint.width.max(definition.footprint.height) + 2;
    }
    Ok(FacilityPlacementRequest {
        schema_version: SUPPORTED_FACILITY_PLACEMENT_SCHEMA_VERSION,
        max_width: extent,
        max_height: extent,
    })
}

fn missing_facility(instance: &FacilityInstance) -> ConstructiveFrontierDiagnostic {
    ConstructiveFrontierDiagnostic::error(
        "missing-facility-definition",
        "/nodes",
        Some(instance.id.clone()),
        format!(
            "facility instance '{}' references missing facility '{}'",
            instance.id, instance.facility
        ),
    )
}

fn seed_candidates(
    instance: &FacilityInstance,
    definition: &FacilityDefinition,
    request: &FacilityPlacementRequest,
) -> Vec<FacilityPlacement> {
    let mut candidates = Vec::new();
    let mut seen = BTreeSet::new();
    for &rotation in &definition.allowed_rotations {
        let (width, height) = match rotation {
            0 | 180 => (definition.footprint.width, definition.footprint.height),
            90 | 270 => (definition.footprint.height, definition.footprint.width),
            _ => continue,
        };
        if !seen.insert((width, height, rotation)) {
            continue;
        }
        candidates.push(FacilityPlacement {
            instance: instance.id.clone(),
            recipe: instance.recipe.clone(),
            facility: instance.facility.clone(),
            x: (request.max_width - width) / 2,
            y: (request.max_height - height) / 2,
            width,
            height,
            rotation,
        });
    }
    candidates
}

fn network_for(
    edge: &FacilityInstanceWiringEdge,
    transport: TransportKind,
    source: &PlacedFacilityPort,
    target: &PlacedFacilityPort,
    cells: Vec<WorldGridPosition>,
) -> TransportNetwork {
    TransportNetwork {
        id: format!("constructive:{}", edge.id),
        requirement_ids: vec![edge.id.clone()],
        item: edge.item.clone(),
        transport,
        segments: cells
            .windows(2)
            .map(|pair| TransportNetworkSegment {
                from: pair[0].clone(),
                to: pair[1].clone(),
                rate: edge.rate,
            })
            .collect(),
        cells,
        terminals: vec![
            TransportNetworkTerminal {
                id: format!("{}:source", edge.id),
                node: source.instance.clone(),
                direction: FacilityPortDirection::Output,
                endpoint: TransportNetworkEndpoint::Facility {
                    instance: source.instance.clone(),
                    port: source.port.clone(),
                },
                position: source.connection.clone(),
                rate: edge.rate,
            },
            TransportNetworkTerminal {
                id: format!("{}:target", edge.id),
                node: target.instance.clone(),
                direction: FacilityPortDirection::Input,
                endpoint: TransportNetworkEndpoint::Facility {
                    instance: target.instance.clone(),
                    port: target.port.clone(),
                },
                position: target.connection.clone(),
                rate: edge.rate,
            },
        ],
        component_ids: Vec::new(),
    }
}

fn future_port_loss(
    state: &LayoutState,
    remaining: &[GrowthEdge<'_>],
    facilities: &ValidatedFacilityCatalog,
    request: &FacilityPlacementRequest,
) -> Option<usize> {
    if remaining.is_empty() {
        return Some(0);
    }
    let report = FacilityPlacementReport {
        success: true,
        status: FacilityPlacementStatus::Feasible,
        bounds: Some(bounds_for(&state.placements, &[])),
        placements: state.placements.clone(),
        diagnostics: Vec::new(),
    };
    let projection = project_facility_ports(&report, facilities, request);
    if !projection.success {
        return None;
    }
    let occupied_facilities = occupied_cells(&state.placements);
    let mut loss = 0;
    for edge in remaining {
        let occupied_transport =
            transport_cells_for_kind(&state.transport_networks, edge.transport);
        for (instance, direction) in [
            (&edge.source.id, FacilityPortDirection::Output),
            (&edge.target.id, FacilityPortDirection::Input),
        ] {
            if !state
                .placements
                .iter()
                .any(|placement| placement.instance == *instance)
            {
                continue;
            }
            let candidates = projection
                .ports
                .iter()
                .filter(|port| {
                    port.instance == *instance
                        && port.direction == direction
                        && port.transport == edge.transport
                        && !state
                            .used_ports
                            .contains(&(port.instance.clone(), port.port.clone()))
                })
                .collect::<Vec<_>>();
            let viable = candidates
                .iter()
                .filter(|port| {
                    !occupied_facilities.contains(&(port.connection.x, port.connection.y))
                        && !occupied_transport.contains(&(port.connection.x, port.connection.y))
                })
                .count();
            if viable == 0 {
                return None;
            }
            loss += candidates.len() - viable;
        }
    }
    Some(loss)
}

fn score(state: &LayoutState, blocked_future_port_options: usize) -> ConstructionCandidateScore {
    let transport_tiles = state
        .transport_networks
        .iter()
        .flat_map(|network| {
            network
                .cells
                .iter()
                .map(move |cell| (transport_layer_key(network.transport), cell.x, cell.y))
        })
        .collect::<HashSet<_>>()
        .len();
    ConstructionCandidateScore {
        used_bounding_box_area: used_area_lower_bound(
            &state.placements,
            &state.transport_networks,
            &[],
        ),
        blocked_future_port_options,
        transport_tiles,
        route_turns: state
            .transport_networks
            .iter()
            .map(|network| count_turns(&network.cells))
            .sum(),
    }
}

fn used_area_lower_bound(
    placements: &[FacilityPlacement],
    networks: &[TransportNetwork],
    extra_points: &[&WorldGridPosition],
) -> usize {
    let minimum_x = placements
        .iter()
        .map(|placement| placement.x)
        .chain(
            networks
                .iter()
                .flat_map(|network| network.cells.iter().map(|cell| cell.x)),
        )
        .chain(extra_points.iter().map(|point| point.x))
        .min()
        .unwrap_or(0);
    let minimum_y = placements
        .iter()
        .map(|placement| placement.y)
        .chain(
            networks
                .iter()
                .flat_map(|network| network.cells.iter().map(|cell| cell.y)),
        )
        .chain(extra_points.iter().map(|point| point.y))
        .min()
        .unwrap_or(0);
    let maximum_x = placements
        .iter()
        .map(|placement| placement.x + placement.width)
        .chain(
            networks
                .iter()
                .flat_map(|network| network.cells.iter().map(|cell| cell.x + 1)),
        )
        .chain(extra_points.iter().map(|point| point.x + 1))
        .max()
        .unwrap_or(0);
    let maximum_y = placements
        .iter()
        .map(|placement| placement.y + placement.height)
        .chain(
            networks
                .iter()
                .flat_map(|network| network.cells.iter().map(|cell| cell.y + 1)),
        )
        .chain(extra_points.iter().map(|point| point.y + 1))
        .max()
        .unwrap_or(0);
    usize::try_from((maximum_x - minimum_x).saturating_mul(maximum_y - minimum_y))
        .unwrap_or(usize::MAX)
}

fn validate_state(state: &LayoutState) -> bool {
    for (index, left) in state.placements.iter().enumerate() {
        if state.placements[index + 1..]
            .iter()
            .any(|right| rectangles_overlap(left, right))
        {
            return false;
        }
    }
    let facilities = occupied_cells(&state.placements);
    let mut transport = HashSet::new();
    for network in &state.transport_networks {
        if network.cells.is_empty()
            || network
                .cells
                .iter()
                .any(|cell| facilities.contains(&(cell.x, cell.y)))
            || network
                .cells
                .windows(2)
                .any(|pair| pair[0].x.abs_diff(pair[1].x) + pair[0].y.abs_diff(pair[1].y) != 1)
            || network.cells.iter().any(|cell| {
                !transport.insert((transport_layer_key(network.transport), cell.x, cell.y))
            })
            || network.terminals.first().map(|terminal| &terminal.position) != network.cells.first()
            || network.terminals.last().map(|terminal| &terminal.position) != network.cells.last()
        {
            return false;
        }
    }
    true
}

fn transport_cells_for_kind(
    networks: &[TransportNetwork],
    transport: TransportKind,
) -> HashSet<(i64, i64)> {
    networks
        .iter()
        .filter(|network| network.transport == transport)
        .flat_map(|network| network.cells.iter().map(|cell| (cell.x, cell.y)))
        .collect()
}

fn transport_layer_key(transport: TransportKind) -> u8 {
    match transport {
        TransportKind::Belt => 0,
        TransportKind::Pipe => 1,
    }
}

fn canonical_snapshot(
    state: &LayoutState,
    mut source_port: PlacedFacilityPort,
    mut target_port: PlacedFacilityPort,
) -> (
    Vec<FacilityPlacement>,
    Vec<TransportNetwork>,
    PlacedFacilityPort,
    PlacedFacilityPort,
    FacilityPlacementBounds,
) {
    let mut placements = state.placements.clone();
    let mut networks = state.transport_networks.clone();
    let minimum_x = placements
        .iter()
        .map(|placement| placement.x)
        .chain(
            networks
                .iter()
                .flat_map(|network| network.cells.iter().map(|cell| cell.x)),
        )
        .min()
        .unwrap_or(0);
    let minimum_y = placements
        .iter()
        .map(|placement| placement.y)
        .chain(
            networks
                .iter()
                .flat_map(|network| network.cells.iter().map(|cell| cell.y)),
        )
        .min()
        .unwrap_or(0);
    for placement in &mut placements {
        placement.x -= minimum_x;
        placement.y -= minimum_y;
    }
    for network in &mut networks {
        for cell in &mut network.cells {
            cell.x -= minimum_x;
            cell.y -= minimum_y;
        }
        for segment in &mut network.segments {
            segment.from.x -= minimum_x;
            segment.from.y -= minimum_y;
            segment.to.x -= minimum_x;
            segment.to.y -= minimum_y;
        }
        for terminal in &mut network.terminals {
            terminal.position.x -= minimum_x;
            terminal.position.y -= minimum_y;
        }
    }
    for port in [&mut source_port, &mut target_port] {
        port.position.x -= minimum_x;
        port.position.y -= minimum_y;
        port.connection.x -= minimum_x;
        port.connection.y -= minimum_y;
    }
    let cells = networks
        .iter()
        .flat_map(|network| network.cells.iter().cloned())
        .collect::<Vec<_>>();
    let bounds = bounds_for(&placements, &cells);
    (placements, networks, source_port, target_port, bounds)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::facilities::{
        FacilityCatalog, FacilityFootprint, FacilityPortDefinition, FacilityPortEdge,
        FacilityPortPosition, SUPPORTED_FACILITY_CATALOG_SCHEMA_VERSION,
    };
    use crate::logistics::{ItemCatalog, ItemDefinition, SUPPORTED_ITEM_CATALOG_SCHEMA_VERSION};
    use crate::recipes::{
        FACILITY_INSTANCE_WIRING_SCHEMA_VERSION, FacilityInstanceWiringProjection, Rate,
    };

    fn definition(id: &str) -> FacilityDefinition {
        FacilityDefinition {
            id: id.to_string(),
            footprint: FacilityFootprint {
                width: 2,
                height: 2,
            },
            allowed_rotations: vec![0],
            ports: vec![
                FacilityPortDefinition {
                    id: "input".to_string(),
                    direction: FacilityPortDirection::Input,
                    transport: TransportKind::Pipe,
                    position: FacilityPortPosition { x: 0, y: 0 },
                    edge: FacilityPortEdge::West,
                },
                FacilityPortDefinition {
                    id: "output".to_string(),
                    direction: FacilityPortDirection::Output,
                    transport: TransportKind::Pipe,
                    position: FacilityPortPosition { x: 1, y: 0 },
                    edge: FacilityPortEdge::East,
                },
                FacilityPortDefinition {
                    id: "belt-input".to_string(),
                    direction: FacilityPortDirection::Input,
                    transport: TransportKind::Belt,
                    position: FacilityPortPosition { x: 0, y: 1 },
                    edge: FacilityPortEdge::South,
                },
                FacilityPortDefinition {
                    id: "belt-output".to_string(),
                    direction: FacilityPortDirection::Output,
                    transport: TransportKind::Belt,
                    position: FacilityPortPosition { x: 1, y: 1 },
                    edge: FacilityPortEdge::South,
                },
            ],
        }
    }

    fn node(id: &str) -> FacilityInstanceWiringNode {
        let one = Rate {
            numerator: 1,
            denominator: 1,
        };
        FacilityInstanceWiringNode::Facility {
            id: id.to_string(),
            recipe: format!("{id}-recipe"),
            facility: id.to_string(),
            index: 0,
            runs_per_second: one,
            work_seconds_per_second: one,
            unused_capacity: Rate::zero(),
        }
    }

    #[test]
    fn grows_a_three_facility_pipe_chain_in_two_routed_phases() {
        let rate = Rate {
            numerator: 1,
            denominator: 1,
        };
        let wiring = FacilityInstanceWiringReport {
            schema_version: FACILITY_INSTANCE_WIRING_SCHEMA_VERSION,
            success: true,
            nodes: vec![node("upstream"), node("middle"), node("target")],
            edges: vec![
                FacilityInstanceWiringEdge {
                    id: "upstream-middle".to_string(),
                    source: "upstream".to_string(),
                    target: "middle".to_string(),
                    kind: "intermediate".to_string(),
                    item: "fluid-a".to_string(),
                    rate,
                    projection: FacilityInstanceWiringProjection::Original,
                },
                FacilityInstanceWiringEdge {
                    id: "middle-target".to_string(),
                    source: "middle".to_string(),
                    target: "target".to_string(),
                    kind: "intermediate".to_string(),
                    item: "fluid-b".to_string(),
                    rate,
                    projection: FacilityInstanceWiringProjection::Original,
                },
            ],
            diagnostics: Vec::new(),
        };
        let facilities = ValidatedFacilityCatalog::try_from_catalog(FacilityCatalog {
            schema_version: SUPPORTED_FACILITY_CATALOG_SCHEMA_VERSION,
            facilities: vec![
                definition("upstream"),
                definition("middle"),
                definition("target"),
            ],
        })
        .expect("facility catalog validates");
        let items = ValidatedItemCatalog::try_from_catalog(ItemCatalog {
            schema_version: SUPPORTED_ITEM_CATALOG_SCHEMA_VERSION,
            items: vec![
                ItemDefinition {
                    id: "fluid-a".to_string(),
                    transport: TransportKind::Pipe,
                },
                ItemDefinition {
                    id: "fluid-b".to_string(),
                    transport: TransportKind::Pipe,
                },
            ],
        })
        .expect("item catalog validates");

        let report = construct_frontier_growth(&wiring, &facilities, &items, 1);
        assert!(report.success, "{:?}", report.diagnostics);
        assert_eq!(report.phases.len(), 2);
        assert_eq!(report.phases[0].placements.len(), 2);
        assert_eq!(report.phases[1].placements.len(), 3);
        assert_eq!(report.phases[1].transport_networks.len(), 2);
        assert_eq!(report.statistics.completed_requirements, 2);
        let html = crate::layouts::render_constructive_frontier_growth_html(&report, None)
            .expect("constructive frontier growth history should render");
        assert!(html.contains("data-phase-label=\"Growth 1/2\""));
        assert!(html.contains("data-phase-label=\"Growth 2/2\""));
        assert!(html.contains("CONSTRUCTED"));
        assert!(validate_state(&LayoutState {
            placements: report.placements,
            transport_networks: report.transport_networks,
            used_ports: BTreeSet::new(),
        }));
    }

    #[test]
    fn extends_the_pipe_chain_with_an_immediate_belt_supplier() {
        let rate = Rate {
            numerator: 1,
            denominator: 1,
        };
        let wiring = FacilityInstanceWiringReport {
            schema_version: FACILITY_INSTANCE_WIRING_SCHEMA_VERSION,
            success: true,
            nodes: vec![
                node("upstream"),
                node("middle"),
                node("target"),
                node("belt-source"),
                node("belt-source-upstream"),
            ],
            edges: vec![
                FacilityInstanceWiringEdge {
                    id: "upstream-middle".to_string(),
                    source: "upstream".to_string(),
                    target: "middle".to_string(),
                    kind: "intermediate".to_string(),
                    item: "fluid-a".to_string(),
                    rate,
                    projection: FacilityInstanceWiringProjection::Original,
                },
                FacilityInstanceWiringEdge {
                    id: "middle-target".to_string(),
                    source: "middle".to_string(),
                    target: "target".to_string(),
                    kind: "intermediate".to_string(),
                    item: "fluid-b".to_string(),
                    rate,
                    projection: FacilityInstanceWiringProjection::Original,
                },
                FacilityInstanceWiringEdge {
                    id: "belt-target".to_string(),
                    source: "belt-source".to_string(),
                    target: "target".to_string(),
                    kind: "intermediate".to_string(),
                    item: "solid".to_string(),
                    rate,
                    projection: FacilityInstanceWiringProjection::Original,
                },
                FacilityInstanceWiringEdge {
                    id: "belt-source-upstream".to_string(),
                    source: "belt-source-upstream".to_string(),
                    target: "belt-source".to_string(),
                    kind: "intermediate".to_string(),
                    item: "solid".to_string(),
                    rate,
                    projection: FacilityInstanceWiringProjection::Original,
                },
            ],
            diagnostics: Vec::new(),
        };
        let facilities = ValidatedFacilityCatalog::try_from_catalog(FacilityCatalog {
            schema_version: SUPPORTED_FACILITY_CATALOG_SCHEMA_VERSION,
            facilities: vec![
                definition("upstream"),
                definition("middle"),
                definition("target"),
                definition("belt-source"),
            ],
        })
        .expect("facility catalog validates");
        let items = ValidatedItemCatalog::try_from_catalog(ItemCatalog {
            schema_version: SUPPORTED_ITEM_CATALOG_SCHEMA_VERSION,
            items: vec![
                ItemDefinition {
                    id: "fluid-a".to_string(),
                    transport: TransportKind::Pipe,
                },
                ItemDefinition {
                    id: "fluid-b".to_string(),
                    transport: TransportKind::Pipe,
                },
                ItemDefinition {
                    id: "solid".to_string(),
                    transport: TransportKind::Belt,
                },
            ],
        })
        .expect("item catalog validates");

        let report = construct_frontier_growth(&wiring, &facilities, &items, 1);
        assert!(report.success, "{:?}", report.diagnostics);
        assert_eq!(report.phases.len(), 3);
        assert_eq!(report.placements.len(), 4);
        assert_eq!(report.transport_networks.len(), 3);
        assert_eq!(report.transport_networks[2].transport, TransportKind::Belt);
        assert_eq!(report.statistics.completed_requirements, 3);

        let two_ring_growth = select_initial_frontier_growth(&wiring, &items, 2)
            .expect("two belt rings should be selected");
        assert_eq!(two_ring_growth.len(), 4);
        assert_eq!(two_ring_growth[3].source.id, "belt-source-upstream");
    }

    #[test]
    fn candidate_score_ignores_translation_inside_the_local_canvas() {
        let state = |offset| LayoutState {
            placements: vec![FacilityPlacement {
                instance: "facility".to_string(),
                recipe: "recipe".to_string(),
                facility: "machine".to_string(),
                x: offset,
                y: offset,
                width: 2,
                height: 2,
                rotation: 0,
            }],
            transport_networks: vec![TransportNetwork {
                id: "network".to_string(),
                requirement_ids: vec!["requirement".to_string()],
                item: "fluid".to_string(),
                transport: TransportKind::Pipe,
                cells: vec![WorldGridPosition {
                    x: offset + 2,
                    y: offset,
                }],
                segments: Vec::new(),
                terminals: Vec::new(),
                component_ids: Vec::new(),
            }],
            used_ports: BTreeSet::new(),
        };

        assert_eq!(score(&state(1), 0), score(&state(100), 0));
    }
}
