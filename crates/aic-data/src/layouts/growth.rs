use std::collections::{BTreeMap, BTreeSet};

use serde::Serialize;

use crate::recipes::{FacilityInstanceWiringNode, FacilityInstanceWiringReport};

const STAGE: &str = "facility-growth-planning";

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct FacilityGrowthPlanReport {
    pub success: bool,
    pub components: Vec<FacilityGrowthComponent>,
    pub phases: Vec<FacilityGrowthPhase>,
    pub diagnostics: Vec<FacilityGrowthDiagnostic>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct FacilityGrowthComponent {
    pub id: String,
    pub facilities: Vec<String>,
    pub downstream_components: Vec<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct FacilityGrowthPhase {
    pub index: usize,
    pub components: Vec<String>,
    pub facilities: Vec<String>,
    pub ready_component_count: usize,
    pub selected_component_count: usize,
    pub deferred_component_count: usize,
    pub oversized_component_count: usize,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct FacilityGrowthDiagnostic {
    pub stage: &'static str,
    pub severity: &'static str,
    pub code: &'static str,
    pub path: String,
    pub entity: Option<String>,
    pub message: String,
}

impl FacilityGrowthDiagnostic {
    fn error(
        code: &'static str,
        path: impl Into<String>,
        entity: Option<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            stage: STAGE,
            severity: "error",
            code,
            path: path.into(),
            entity,
            message: message.into(),
        }
    }

    fn info(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            stage: STAGE,
            severity: "info",
            code,
            path: "/".to_string(),
            entity: None,
            message: message.into(),
        }
    }
}

pub fn plan_facility_growth(
    wiring: &FacilityInstanceWiringReport,
    max_new_facilities_per_phase: usize,
) -> FacilityGrowthPlanReport {
    if max_new_facilities_per_phase == 0 {
        return FacilityGrowthPlanReport::failure(FacilityGrowthDiagnostic::error(
            "max-new-facilities-per-phase-must-be-positive",
            "/max_new_facilities_per_phase",
            None,
            "maximum new facilities per phase must be positive",
        ));
    }
    let graph = match FacilityGraph::from_wiring(wiring) {
        Ok(graph) => graph,
        Err(diagnostic) => return FacilityGrowthPlanReport::failure(diagnostic),
    };
    if graph.facilities.is_empty() {
        return FacilityGrowthPlanReport {
            success: true,
            components: Vec::new(),
            phases: Vec::new(),
            diagnostics: vec![FacilityGrowthDiagnostic::info(
                "empty-facility-growth-plan",
                "wiring contains no facility instances, so no placement growth phases are required",
            )],
        };
    }

    let raw_components = strongly_connected_components(&graph.adjacency);
    let mut component_members = raw_components
        .into_iter()
        .map(|component| {
            let mut members = component
                .into_iter()
                .map(|node| graph.facilities[node].clone())
                .collect::<Vec<_>>();
            members.sort();
            members
        })
        .collect::<Vec<_>>();
    component_members.sort();

    let component_by_facility = component_members
        .iter()
        .enumerate()
        .flat_map(|(component, members)| {
            members
                .iter()
                .cloned()
                .map(move |facility| (facility, component))
        })
        .collect::<BTreeMap<_, _>>();
    let mut downstream = vec![BTreeSet::new(); component_members.len()];
    let mut has_self_edge = vec![false; component_members.len()];
    for (source, targets) in graph.adjacency.iter().enumerate() {
        let source_component = component_by_facility[&graph.facilities[source]];
        for target in targets {
            let target_component = component_by_facility[&graph.facilities[*target]];
            if source_component == target_component {
                if source == *target {
                    has_self_edge[source_component] = true;
                }
            } else {
                downstream[source_component].insert(target_component);
            }
        }
    }

    let component_ids = (0..component_members.len())
        .map(|index| format!("component:{index:04}"))
        .collect::<Vec<_>>();
    let depths = reverse_depths(&downstream);
    let components = component_members
        .iter()
        .enumerate()
        .map(|(index, facilities)| FacilityGrowthComponent {
            id: component_ids[index].clone(),
            facilities: facilities.clone(),
            downstream_components: downstream[index]
                .iter()
                .map(|target| component_ids[*target].clone())
                .collect(),
        })
        .collect::<Vec<_>>();

    let phases = bounded_ready_frontier_phases(
        &component_members,
        &component_ids,
        &downstream,
        &depths,
        max_new_facilities_per_phase,
    );
    let phase_count = phases.len();
    let cyclic_components = component_members
        .iter()
        .enumerate()
        .filter(|(index, members)| members.len() > 1 || has_self_edge[*index])
        .count();

    FacilityGrowthPlanReport {
        success: true,
        components,
        phases,
        diagnostics: vec![FacilityGrowthDiagnostic::info(
            "facility-growth-plan-built",
            format!(
                "planned {} facility instances as {} strongly connected components across {} output-first growth phases; {} components contain cycles",
                graph.facilities.len(),
                component_members.len(),
                phase_count,
                cyclic_components,
            ),
        )],
    }
}

impl FacilityGrowthPlanReport {
    fn failure(diagnostic: FacilityGrowthDiagnostic) -> Self {
        Self {
            success: false,
            components: Vec::new(),
            phases: Vec::new(),
            diagnostics: vec![diagnostic],
        }
    }
}

fn bounded_ready_frontier_phases(
    component_members: &[Vec<String>],
    component_ids: &[String],
    downstream: &[BTreeSet<usize>],
    reverse_depths: &[usize],
    max_new_facilities_per_phase: usize,
) -> Vec<FacilityGrowthPhase> {
    let mut included = BTreeSet::new();
    let mut phases = Vec::new();
    while included.len() < component_members.len() {
        let mut ready = (0..component_members.len())
            .filter(|component| {
                !included.contains(component)
                    && downstream[*component]
                        .iter()
                        .all(|target| included.contains(target))
            })
            .collect::<Vec<_>>();
        ready.sort_by(|left, right| {
            (reverse_depths[*left], component_ids[*left].as_str())
                .cmp(&(reverse_depths[*right], component_ids[*right].as_str()))
        });
        assert!(
            !ready.is_empty(),
            "condensed SCC graph must always expose a ready component"
        );
        let ready_component_count = ready.len();
        let mut selected = Vec::new();
        let mut selected_facilities = 0_usize;
        for component in ready {
            let component_facilities = component_members[component].len();
            if selected.is_empty() {
                selected.push(component);
                selected_facilities = component_facilities;
                if component_facilities > max_new_facilities_per_phase {
                    break;
                }
                continue;
            }
            let Some(next_total) = selected_facilities.checked_add(component_facilities) else {
                break;
            };
            if next_total > max_new_facilities_per_phase {
                break;
            }
            selected.push(component);
            selected_facilities = next_total;
        }
        let selected_component_count = selected.len();
        let oversized_component_count = selected
            .iter()
            .filter(|component| component_members[**component].len() > max_new_facilities_per_phase)
            .count();
        let components = selected
            .iter()
            .map(|component| component_ids[*component].clone())
            .collect();
        let facilities = selected
            .iter()
            .flat_map(|component| component_members[*component].iter().cloned())
            .collect();
        included.extend(selected);
        phases.push(FacilityGrowthPhase {
            index: phases.len(),
            components,
            facilities,
            ready_component_count,
            selected_component_count,
            deferred_component_count: ready_component_count - selected_component_count,
            oversized_component_count,
        });
    }
    phases
}

struct FacilityGraph {
    facilities: Vec<String>,
    adjacency: Vec<Vec<usize>>,
}

impl FacilityGraph {
    fn from_wiring(
        wiring: &FacilityInstanceWiringReport,
    ) -> Result<Self, FacilityGrowthDiagnostic> {
        if !wiring.success {
            return Err(FacilityGrowthDiagnostic::error(
                "upstream-instance-wiring-failed",
                "/",
                None,
                "facility growth planning requires successful facility instance wiring",
            ));
        }

        let mut node_kinds = BTreeMap::new();
        let mut facilities = Vec::new();
        for (index, node) in wiring.nodes.iter().enumerate() {
            let (id, is_facility) = match node {
                FacilityInstanceWiringNode::Facility { id, .. } => (id, true),
                FacilityInstanceWiringNode::External { id, .. }
                | FacilityInstanceWiringNode::Target { id, .. }
                | FacilityInstanceWiringNode::Surplus { id, .. } => (id, false),
            };
            if node_kinds.insert(id.clone(), is_facility).is_some() {
                return Err(FacilityGrowthDiagnostic::error(
                    "duplicate-wiring-node",
                    format!("/nodes/{index}/id"),
                    Some(id.clone()),
                    format!("wiring node '{id}' appears more than once"),
                ));
            }
            if is_facility {
                facilities.push(id.clone());
            }
        }
        facilities.sort();
        let facility_indexes = facilities
            .iter()
            .enumerate()
            .map(|(index, id)| (id.as_str(), index))
            .collect::<BTreeMap<_, _>>();
        let mut adjacency = vec![BTreeSet::new(); facilities.len()];
        for (index, edge) in wiring.edges.iter().enumerate() {
            let Some(source_is_facility) = node_kinds.get(&edge.source) else {
                return Err(missing_endpoint(index, "source", &edge.source));
            };
            let Some(target_is_facility) = node_kinds.get(&edge.target) else {
                return Err(missing_endpoint(index, "target", &edge.target));
            };
            if *source_is_facility && *target_is_facility {
                adjacency[facility_indexes[edge.source.as_str()]]
                    .insert(facility_indexes[edge.target.as_str()]);
            }
        }

        Ok(Self {
            facilities,
            adjacency: adjacency
                .into_iter()
                .map(|targets| targets.into_iter().collect())
                .collect(),
        })
    }
}

fn missing_endpoint(index: usize, endpoint: &str, id: &str) -> FacilityGrowthDiagnostic {
    FacilityGrowthDiagnostic::error(
        "missing-wiring-edge-endpoint",
        format!("/edges/{index}/{endpoint}"),
        Some(id.to_string()),
        format!("wiring edge {endpoint} '{id}' does not reference a known node"),
    )
}

fn strongly_connected_components(adjacency: &[Vec<usize>]) -> Vec<Vec<usize>> {
    let mut visited = vec![false; adjacency.len()];
    let mut finish_order = Vec::with_capacity(adjacency.len());
    for node in 0..adjacency.len() {
        visit_for_finish(node, adjacency, &mut visited, &mut finish_order);
    }

    let mut reverse = vec![Vec::new(); adjacency.len()];
    for (source, targets) in adjacency.iter().enumerate() {
        for target in targets {
            reverse[*target].push(source);
        }
    }
    let mut assigned = vec![false; adjacency.len()];
    let mut components = Vec::new();
    while let Some(node) = finish_order.pop() {
        if assigned[node] {
            continue;
        }
        let mut component = Vec::new();
        collect_component(node, &reverse, &mut assigned, &mut component);
        components.push(component);
    }
    components
}

fn visit_for_finish(
    node: usize,
    adjacency: &[Vec<usize>],
    visited: &mut [bool],
    finish_order: &mut Vec<usize>,
) {
    if visited[node] {
        return;
    }
    visited[node] = true;
    for target in &adjacency[node] {
        visit_for_finish(*target, adjacency, visited, finish_order);
    }
    finish_order.push(node);
}

fn collect_component(
    node: usize,
    reverse: &[Vec<usize>],
    assigned: &mut [bool],
    component: &mut Vec<usize>,
) {
    if assigned[node] {
        return;
    }
    assigned[node] = true;
    component.push(node);
    for source in &reverse[node] {
        collect_component(*source, reverse, assigned, component);
    }
}

fn reverse_depths(downstream: &[BTreeSet<usize>]) -> Vec<usize> {
    let mut upstream = vec![Vec::new(); downstream.len()];
    let mut remaining_downstream = Vec::with_capacity(downstream.len());
    let mut ready = BTreeSet::new();
    for (source, targets) in downstream.iter().enumerate() {
        remaining_downstream.push(targets.len());
        if targets.is_empty() {
            ready.insert(source);
        }
        for target in targets {
            upstream[*target].push(source);
        }
    }

    let mut depths = vec![0; downstream.len()];
    while let Some(component) = ready.pop_first() {
        for source in &upstream[component] {
            depths[*source] = depths[*source].max(depths[component] + 1);
            remaining_downstream[*source] -= 1;
            if remaining_downstream[*source] == 0 {
                ready.insert(*source);
            }
        }
    }
    depths
}

#[cfg(test)]
mod tests {
    use crate::recipes::{
        FacilityInstanceWiringDiagnostic, FacilityInstanceWiringEdge, FacilityInstanceWiringNode,
        FacilityInstanceWiringReport, Rate,
    };

    use super::plan_facility_growth;

    #[test]
    fn grows_a_chain_from_output_toward_inputs() {
        let report = plan_facility_growth(
            &wiring(&["a", "b", "c"], &[("a", "b"), ("b", "c"), ("c", "target")]),
            8,
        );

        assert!(report.success);
        assert_eq!(
            phase_facilities(&report),
            vec![vec!["c"], vec!["b"], vec!["a"]]
        );
    }

    #[test]
    fn keeps_a_cycle_in_one_atomic_component() {
        let report = plan_facility_growth(
            &wiring(
                &["a", "b", "c"],
                &[("a", "b"), ("b", "a"), ("b", "c"), ("c", "target")],
            ),
            8,
        );

        assert!(report.success);
        assert_eq!(phase_facilities(&report), vec![vec!["c"], vec!["a", "b"]]);
        assert_eq!(report.components[0].facilities, vec!["a", "b"]);
    }

    #[test]
    fn assigns_branches_by_longest_distance_to_an_output() {
        let report = plan_facility_growth(
            &wiring(
                &["a", "b", "c", "d"],
                &[
                    ("a", "c"),
                    ("b", "c"),
                    ("b", "d"),
                    ("c", "target"),
                    ("d", "target"),
                ],
            ),
            8,
        );

        assert!(report.success);
        assert_eq!(
            phase_facilities(&report),
            vec![vec!["c", "d"], vec!["a", "b"]]
        );
    }

    #[test]
    fn rejects_an_edge_with_an_unknown_endpoint() {
        let report = plan_facility_growth(&wiring(&["a"], &[("missing", "a")]), 8);

        assert!(!report.success);
        assert_eq!(report.diagnostics[0].code, "missing-wiring-edge-endpoint");
        assert_eq!(report.diagnostics[0].path, "/edges/0/source");
    }

    #[test]
    fn divides_a_wide_ready_frontier_into_bounded_deterministic_phases() {
        let graph = wiring(
            &["a", "b", "c", "d"],
            &[
                ("a", "target"),
                ("b", "target"),
                ("c", "target"),
                ("d", "target"),
            ],
        );
        let first = plan_facility_growth(&graph, 2);
        let second = plan_facility_growth(&graph, 2);

        assert_eq!(first, second);
        assert_eq!(
            phase_facilities(&first),
            vec![vec!["a", "b"], vec!["c", "d"]]
        );
        assert_eq!(first.phases[0].ready_component_count, 4);
        assert_eq!(first.phases[0].selected_component_count, 2);
        assert_eq!(first.phases[0].deferred_component_count, 2);
        assert_eq!(first.phases[0].oversized_component_count, 0);
        assert_eq!(first.phases[1].ready_component_count, 2);
        assert_eq!(first.phases[1].deferred_component_count, 0);
    }

    #[test]
    fn schedules_an_oversized_cycle_atomically_and_alone() {
        let report = plan_facility_growth(
            &wiring(
                &["a", "b", "c", "d"],
                &[
                    ("a", "b"),
                    ("b", "c"),
                    ("c", "a"),
                    ("c", "target"),
                    ("d", "target"),
                ],
            ),
            2,
        );

        assert_eq!(
            phase_facilities(&report),
            vec![vec!["a", "b", "c"], vec!["d"]]
        );
        assert_eq!(report.phases[0].selected_component_count, 1);
        assert_eq!(report.phases[0].oversized_component_count, 1);
        assert_eq!(report.phases[0].deferred_component_count, 1);
    }

    #[test]
    fn every_condensation_edge_points_from_a_later_phase_to_an_earlier_phase() {
        let report = plan_facility_growth(
            &wiring(
                &["a", "b", "c", "d"],
                &[("a", "c"), ("b", "d"), ("c", "target"), ("d", "target")],
            ),
            1,
        );
        let phase_by_facility = report
            .phases
            .iter()
            .flat_map(|phase| {
                phase
                    .facilities
                    .iter()
                    .map(move |facility| (facility.as_str(), phase.index))
            })
            .collect::<std::collections::BTreeMap<_, _>>();

        assert!(phase_by_facility["a"] > phase_by_facility["c"]);
        assert!(phase_by_facility["b"] > phase_by_facility["d"]);
    }

    #[test]
    fn rejects_a_zero_phase_facility_limit() {
        let report = plan_facility_growth(&wiring(&["a"], &[("a", "target")]), 0);

        assert!(!report.success);
        assert_eq!(
            report.diagnostics[0].code,
            "max-new-facilities-per-phase-must-be-positive"
        );
    }

    fn phase_facilities(report: &super::FacilityGrowthPlanReport) -> Vec<Vec<&str>> {
        report
            .phases
            .iter()
            .map(|phase| phase.facilities.iter().map(String::as_str).collect())
            .collect()
    }

    fn wiring(facilities: &[&str], edges: &[(&str, &str)]) -> FacilityInstanceWiringReport {
        let mut nodes = facilities
            .iter()
            .map(|id| facility_node(id))
            .collect::<Vec<_>>();
        nodes.push(FacilityInstanceWiringNode::Target {
            id: "target".to_string(),
            item: "item".to_string(),
        });
        FacilityInstanceWiringReport {
            schema_version: crate::recipes::FACILITY_INSTANCE_WIRING_SCHEMA_VERSION,
            success: true,
            nodes,
            edges: edges
                .iter()
                .map(|(source, target)| {
                    FacilityInstanceWiringEdge::original(
                        *source,
                        *target,
                        "production",
                        "item",
                        Rate {
                            numerator: 1,
                            denominator: 1,
                        },
                    )
                })
                .collect(),
            diagnostics: Vec::<FacilityInstanceWiringDiagnostic>::new(),
        }
    }

    fn facility_node(id: &str) -> FacilityInstanceWiringNode {
        FacilityInstanceWiringNode::Facility {
            id: id.to_string(),
            recipe: format!("recipe-{id}"),
            facility: "facility".to_string(),
            index: 0,
            runs_per_second: Rate {
                numerator: 1,
                denominator: 1,
            },
            work_seconds_per_second: Rate {
                numerator: 1,
                denominator: 1,
            },
            unused_capacity: Rate {
                numerator: 0,
                denominator: 1,
            },
        }
    }
}
