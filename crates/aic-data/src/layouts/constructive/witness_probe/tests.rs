use super::*;
use crate::logistics::LogisticsComponentKind;

struct Fixture {
    input: WitnessProbeInput,
    facilities: ValidatedFacilityCatalog,
    recipes: ValidatedRecipeBook,
    items: ValidatedItemCatalog,
    transports: ValidatedTransportCatalog,
    components: ValidatedLogisticsComponentCatalog,
}

impl Fixture {
    fn load() -> Self {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let data = root.join("data/game/normalized");
        Self {
            input: serde_json::from_slice(
                &std::fs::read(
                    root.join("docs/benchmarks/constructive-bound-interconnect/fixtures/crossing-witness.json"),
                )
                .unwrap(),
            )
            .unwrap(),
            facilities: ValidatedFacilityCatalog::try_from_catalog(
                crate::facilities::load_facility_catalog(data.join("facilities.json")).unwrap(),
            )
            .unwrap(),
            recipes: ValidatedRecipeBook::try_from_recipe_book(
                crate::recipes::load_recipe_book(data.join("recipes.json")).unwrap(),
            )
            .unwrap(),
            items: ValidatedItemCatalog::try_from_catalog(
                crate::logistics::load_item_catalog(data.join("items.json")).unwrap(),
            )
            .unwrap(),
            transports: ValidatedTransportCatalog::try_from_catalog(
                crate::logistics::load_transport_catalog(data.join("transports.json")).unwrap(),
            )
            .unwrap(),
            components: ValidatedLogisticsComponentCatalog::try_from_catalog(
                crate::logistics::load_logistics_component_catalog(
                    data.join("logistics-components.json"),
                )
                .unwrap(),
            )
            .unwrap(),
        }
    }
    fn probe(&self, input: &WitnessProbeInput) -> WitnessProbeReport {
        probe_constructive_witness(
            input,
            &self.facilities,
            &self.recipes,
            &self.items,
            &self.transports,
            &self.components,
        )
    }
}

#[test]
fn recipe_bound_fanin_fanout_crossing_and_external_connections_validate() {
    let f = Fixture::load();
    let r = f.probe(&f.input);
    assert!(r.covered_geometry_validated, "{:?}", r.diagnostics);
    assert!(r.recipe_coverage_complete);
    assert!(!r.factory_validated);
    assert_eq!(r.recipe_terminals.len(), 12);
    assert!(
        r.recipe_terminals
            .iter()
            .all(|t| t.required_rate == t.routed_rate)
    );
    assert_eq!(
        r.bounds.unwrap(),
        FacilityPlacementBounds {
            width: 35,
            height: 22
        }
    );
    let roundtrip: WitnessProbeInput =
        serde_json::from_slice(&serde_json::to_vec(&f.input).unwrap()).unwrap();
    assert!(f.probe(&roundtrip).recipe_coverage_complete);
}

#[test]
fn every_component_kind_is_necessary_for_the_retained_geometry() {
    let f = Fixture::load();
    for kind in [
        LogisticsComponentKind::Bridge,
        LogisticsComponentKind::Splitter,
        LogisticsComponentKind::Converger,
    ] {
        let mut input = f.input.clone();
        input.logistics_components.retain(|c| c.kind != kind);
        for n in &mut input.transport_networks {
            n.component_ids
                .retain(|id| input.logistics_components.iter().any(|c| &c.id == id));
        }
        assert!(!f.probe(&input).covered_geometry_validated, "{kind:?}");
    }
}

#[test]
fn removing_a_whole_network_cannot_hide_recipe_obligations() {
    let f = Fixture::load();
    let mut input = f.input.clone();
    let index = input
        .transport_networks
        .iter()
        .position(|n| n.component_ids.is_empty())
        .unwrap();
    input.transport_networks.remove(index);
    let r = f.probe(&input);
    assert!(r.covered_geometry_validated);
    assert!(!r.recipe_coverage_complete);
    assert!(r.recipe_terminals.iter().any(|t| !t.pending_rate.is_zero()));
    input.transport_networks.clear();
    input.logistics_components.clear();
    let r = f.probe(&input);
    assert!(r.covered_geometry_validated);
    assert!(!r.recipe_coverage_complete);
    assert_eq!(
        r.recipe_terminals
            .iter()
            .filter(|t| !t.pending_rate.is_zero())
            .count(),
        12
    );
}

#[test]
fn malformed_rate_geometry_and_identity_are_rejected_without_panics() {
    let f = Fixture::load();
    let mutations: Vec<Box<dyn Fn(&mut WitnessProbeInput)>> = vec![
        Box::new(|i| i.schema_version = 2),
        Box::new(|i| i.placements[0].x = i64::MAX),
        Box::new(|i| i.placements[0].recipe = "unknown".into()),
        Box::new(|i| {
            i.utilizations.clear();
        }),
        Box::new(|i| i.utilizations.values_mut().next().unwrap().denominator = 0),
        Box::new(|i| i.utilizations.values_mut().next().unwrap().numerator = 999),
        Box::new(|i| i.transport_networks[0].segments[0].rate.denominator = 0),
        Box::new(|i| i.transport_networks[0].segments[0].rate.numerator = 999),
        Box::new(|i| i.transport_networks[0].cells[0].x = i64::MAX),
        Box::new(|i| i.transport_networks[0].item = "unknown".into()),
        Box::new(|i| i.transport_networks[0].terminals[0].direction = FacilityPortDirection::Input),
        Box::new(|i| {
            if let TransportNetworkEndpoint::Facility { port, .. } =
                &mut i.transport_networks[0].terminals[0].endpoint
            {
                *port = "input-belt-0".into();
            }
        }),
        Box::new(|i| i.logistics_components[0].rotation = 45),
        Box::new(|i| i.logistics_components[0].position.x += 1),
    ];
    for (index, change) in mutations.iter().enumerate() {
        let mut input = f.input.clone();
        change(&mut input);
        let r = f.probe(&input);
        assert!(!r.covered_geometry_validated, "mutation {index}");
        assert!(!r.recipe_coverage_complete);
    }
}

#[test]
fn full_utilization_exposes_missing_parallel_capacity_in_the_single_trunk_geometry() {
    let f = Fixture::load();
    let mut input = f.input.clone();
    for u in input.utilizations.values_mut() {
        *u = u.checked_mul_i64(4).unwrap();
    }
    for n in &mut input.transport_networks {
        for s in &mut n.segments {
            s.rate = s.rate.checked_mul_i64(4).unwrap();
        }
        for t in &mut n.terminals {
            t.rate = t.rate.checked_mul_i64(4).unwrap();
        }
    }
    let r = f.probe(&input);
    assert!(!r.covered_geometry_validated);
    assert!(
        r.diagnostics[0].message.contains("segment rate"),
        "{:?}",
        r.diagnostics
    );
}
