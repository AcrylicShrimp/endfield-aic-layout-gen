use std::collections::{BTreeMap, BTreeSet};

use crate::layouts::growth::FacilityGrowthComponent;

use super::{EndpointInput, ModelInput, RoutingConflict};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct NeighborhoodPlan {
    pub rank: usize,
    pub free_facility_ids: BTreeSet<String>,
    pub fixed_facility_ids: BTreeSet<String>,
    pub invalidated_requirement_ids: BTreeSet<String>,
    pub escalation_causes: Vec<String>,
}

pub(super) struct NeighborhoodGraph {
    cumulative_facilities: BTreeSet<String>,
    adjacent_facilities: BTreeMap<String, BTreeSet<String>>,
    component_by_facility: BTreeMap<String, String>,
    facilities_by_component: BTreeMap<String, BTreeSet<String>>,
    facilities_by_requirement: BTreeMap<String, BTreeSet<String>>,
}

impl NeighborhoodGraph {
    pub(super) fn from_model(
        input: &ModelInput,
        growth_components: &[FacilityGrowthComponent],
        cumulative_facilities: &BTreeSet<String>,
    ) -> Self {
        let mut graph = Self {
            cumulative_facilities: cumulative_facilities.clone(),
            adjacent_facilities: cumulative_facilities
                .iter()
                .cloned()
                .map(|facility| (facility, BTreeSet::new()))
                .collect(),
            component_by_facility: BTreeMap::new(),
            facilities_by_component: BTreeMap::new(),
            facilities_by_requirement: BTreeMap::new(),
        };
        for component in growth_components {
            let members = component
                .facilities
                .iter()
                .filter(|facility| cumulative_facilities.contains(*facility))
                .cloned()
                .collect::<BTreeSet<_>>();
            if members.is_empty() {
                continue;
            }
            for facility in &members {
                graph
                    .component_by_facility
                    .insert(facility.clone(), component.id.clone());
            }
            graph
                .facilities_by_component
                .insert(component.id.clone(), members);
        }
        for edge in &input.edges {
            let facilities = endpoint_facilities(&edge.source, &edge.target);
            if facilities.len() == 2 {
                let left = facilities[0].clone();
                let right = facilities[1].clone();
                graph
                    .adjacent_facilities
                    .entry(left.clone())
                    .or_default()
                    .insert(right.clone());
                graph
                    .adjacent_facilities
                    .entry(right)
                    .or_default()
                    .insert(left);
            }
            graph.facilities_by_requirement.insert(
                edge.requirement_id.clone(),
                facilities.into_iter().collect(),
            );
        }
        graph
    }

    pub(super) fn plan(
        &self,
        rank: usize,
        introduced_facilities: &BTreeSet<String>,
        conflicts: &[RoutingConflict],
    ) -> NeighborhoodPlan {
        let mut causes = vec!["introduced-frontier".to_string()];
        let mut free = introduced_facilities.clone();
        expand_one_hop(&mut free, &self.adjacent_facilities);

        if rank >= 1 {
            causes.push("one-hop-repair".to_string());
            expand_components(
                &mut free,
                &self.component_by_facility,
                &self.facilities_by_component,
            );
            let conflict_facilities = self.conflict_facilities(conflicts);
            if !conflict_facilities.is_empty() {
                causes.extend(conflict_codes(conflicts));
                free.extend(conflict_facilities);
            }
        }
        if rank >= 2 {
            causes.push("bounded-two-hop-conflict-closure".to_string());
            let mut closure_seeds = introduced_facilities.clone();
            closure_seeds.extend(self.conflict_facilities(conflicts));
            expand_one_hop(&mut closure_seeds, &self.adjacent_facilities);
            expand_one_hop(&mut closure_seeds, &self.adjacent_facilities);
            free.extend(closure_seeds);
            expand_components(
                &mut free,
                &self.component_by_facility,
                &self.facilities_by_component,
            );
        }
        if rank >= 3 {
            causes.push("global-fallback".to_string());
            free = self.cumulative_facilities.clone();
        }

        free.retain(|facility| self.cumulative_facilities.contains(facility));
        let fixed = self
            .cumulative_facilities
            .difference(&free)
            .cloned()
            .collect::<BTreeSet<_>>();
        let invalidated_requirement_ids = self
            .facilities_by_requirement
            .iter()
            .filter(|(_, facilities)| facilities.iter().any(|facility| free.contains(facility)))
            .map(|(requirement_id, _)| requirement_id.clone())
            .collect();
        NeighborhoodPlan {
            rank,
            free_facility_ids: free,
            fixed_facility_ids: fixed,
            invalidated_requirement_ids,
            escalation_causes: causes,
        }
    }

    fn conflict_facilities(&self, conflicts: &[RoutingConflict]) -> BTreeSet<String> {
        let mut facilities = BTreeSet::new();
        for conflict in conflicts {
            facilities.extend(conflict.related_facility_ids.iter().cloned());
            for component in &conflict.related_scc_ids {
                if let Some(members) = self.facilities_by_component.get(component) {
                    facilities.extend(members.iter().cloned());
                }
            }
            for requirement in conflict
                .failed_requirement_ids
                .iter()
                .chain(&conflict.blocking_requirement_ids)
            {
                if let Some(endpoints) = self.facilities_by_requirement.get(requirement) {
                    facilities.extend(endpoints.iter().cloned());
                }
            }
        }
        facilities
    }
}

fn endpoint_facilities(source: &EndpointInput, target: &EndpointInput) -> Vec<String> {
    [source, target]
        .into_iter()
        .filter_map(|endpoint| match endpoint {
            EndpointInput::Facility { instance, .. } => Some(instance.clone()),
            EndpointInput::External { .. } => None,
        })
        .collect()
}

fn expand_one_hop(
    facilities: &mut BTreeSet<String>,
    adjacency: &BTreeMap<String, BTreeSet<String>>,
) {
    let neighbors = facilities
        .iter()
        .filter_map(|facility| adjacency.get(facility))
        .flatten()
        .cloned()
        .collect::<Vec<_>>();
    facilities.extend(neighbors);
}

fn expand_components(
    facilities: &mut BTreeSet<String>,
    component_by_facility: &BTreeMap<String, String>,
    facilities_by_component: &BTreeMap<String, BTreeSet<String>>,
) {
    let components = facilities
        .iter()
        .filter_map(|facility| component_by_facility.get(facility))
        .cloned()
        .collect::<BTreeSet<_>>();
    for component in components {
        if let Some(members) = facilities_by_component.get(&component) {
            facilities.extend(members.iter().cloned());
        }
    }
}

fn conflict_codes(conflicts: &[RoutingConflict]) -> Vec<String> {
    conflicts
        .iter()
        .map(|conflict| format!("routing-conflict:{}", conflict.code))
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn neighborhoods_expand_without_permanently_fixing_the_frontier() {
        let graph = test_graph();
        let introduced = BTreeSet::from(["d".to_string()]);

        let frontier = graph.plan(0, &introduced, &[]);
        assert_eq!(frontier.free_facility_ids, strings(&["c", "d"]));
        assert_eq!(frontier.fixed_facility_ids, strings(&["a", "b"]));

        let closure = graph.plan(2, &introduced, &[]);
        assert_eq!(closure.free_facility_ids, strings(&["b", "c", "d"]));
        assert_eq!(closure.fixed_facility_ids, strings(&["a"]));

        let global = graph.plan(3, &introduced, &[]);
        assert_eq!(global.free_facility_ids, strings(&["a", "b", "c", "d"]));
        assert!(global.fixed_facility_ids.is_empty());
    }

    #[test]
    fn routing_conflicts_add_blocker_endpoints_to_repair_neighborhoods() {
        let graph = test_graph();
        let conflict = RoutingConflict {
            code: "blocked".to_string(),
            failed_requirement_ids: Vec::new(),
            related_facility_ids: Vec::new(),
            related_scc_ids: Vec::new(),
            blocked_cells: Vec::new(),
            blocking_requirement_ids: vec!["r-ab".to_string()],
            blocking_component_ids: Vec::new(),
            message: "blocked".to_string(),
        };

        let plan = graph.plan(1, &BTreeSet::from(["d".to_string()]), &[conflict]);

        assert_eq!(plan.free_facility_ids, strings(&["a", "b", "c", "d"]));
        assert!(
            plan.escalation_causes
                .contains(&"routing-conflict:blocked".to_string())
        );
    }

    fn test_graph() -> NeighborhoodGraph {
        NeighborhoodGraph {
            cumulative_facilities: strings(&["a", "b", "c", "d"]),
            adjacent_facilities: BTreeMap::from([
                ("a".to_string(), strings(&["b"])),
                ("b".to_string(), strings(&["a", "c"])),
                ("c".to_string(), strings(&["b", "d"])),
                ("d".to_string(), strings(&["c"])),
            ]),
            component_by_facility: BTreeMap::new(),
            facilities_by_component: BTreeMap::new(),
            facilities_by_requirement: BTreeMap::from([
                ("r-ab".to_string(), strings(&["a", "b"])),
                ("r-bc".to_string(), strings(&["b", "c"])),
                ("r-cd".to_string(), strings(&["c", "d"])),
            ]),
        }
    }

    fn strings(values: &[&str]) -> BTreeSet<String> {
        values.iter().map(|value| (*value).to_string()).collect()
    }
}
