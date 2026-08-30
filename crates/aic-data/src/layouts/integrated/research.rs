use std::time::Duration;

use serde::Serialize;

use crate::facilities::ValidatedFacilityCatalog;
use crate::layouts::FacilityPlacementRequest;
use crate::logistics::{
    ValidatedItemCatalog, ValidatedLogisticsComponentCatalog, ValidatedTransportCatalog,
};
use crate::recipes::FacilityInstanceWiringReport;

use super::{IntegratedLayoutReport, exact, harness, prepare_exact_model};

pub const EXACT_ABLATION_MATRIX_SCHEMA_VERSION: u32 = 1;
pub const SHARED_LAYER_COMPARISON_SCHEMA_VERSION: u32 = 1;
pub const FACTORED_ENDPOINT_COMPARISON_SCHEMA_VERSION: u32 = 1;
pub const FACTORED_NETWORK_DECOMPOSITION_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct FactoredNetworkSubsetCaseReport {
    pub id: String,
    pub network_indices: Vec<usize>,
    pub selected_networks: Vec<String>,
    pub search_budget_ms: u64,
    pub layout: IntegratedLayoutReport,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct FactoredNetworkDecompositionReport {
    pub schema_version: u32,
    pub search_budget_ms_per_case: u64,
    pub cases: Vec<FactoredNetworkSubsetCaseReport>,
}

#[allow(clippy::too_many_arguments)]
pub fn decompose_first_integrated_layout_phase_factored_networks(
    instance_wiring: &FacilityInstanceWiringReport,
    facilities: &ValidatedFacilityCatalog,
    items: &ValidatedItemCatalog,
    transports: &ValidatedTransportCatalog,
    logistics_components: &ValidatedLogisticsComponentCatalog,
    request: &FacilityPlacementRequest,
    search_budget: Duration,
) -> Result<FactoredNetworkDecompositionReport, IntegratedLayoutReport> {
    let first_phase_wiring = harness::first_iterative_scc_wiring(instance_wiring)?;
    let input = prepare_exact_model(
        &first_phase_wiring,
        facilities,
        items,
        transports,
        logistics_components,
        request,
    )?;
    let network_count = input.networks.len();
    let mut selections = (0..network_count)
        .map(|index| vec![index])
        .collect::<Vec<_>>();
    for first in 0..network_count {
        for second in (first + 1)..network_count {
            selections.push(vec![first, second]);
        }
    }
    if network_count > 2 {
        selections.push((0..network_count).collect());
    }

    let mut cases = Vec::with_capacity(selections.len());
    for indices in selections {
        let (case_input, selected_networks) = input
            .clone()
            .select_network_indices(&indices)
            .map_err(IntegratedLayoutReport::invalid)?;
        let id = match indices.as_slice() {
            [index] => format!("single-{index}"),
            [first, second] => format!("pair-{first}-{second}"),
            _ => "full".to_string(),
        };
        let layout = exact::shared_layer::solve_factored_endpoints(
            case_input,
            logistics_components,
            Some(search_budget),
        );
        cases.push(FactoredNetworkSubsetCaseReport {
            id,
            network_indices: indices,
            selected_networks,
            search_budget_ms: millis(search_budget),
            layout,
        });
    }

    Ok(FactoredNetworkDecompositionReport {
        schema_version: FACTORED_NETWORK_DECOMPOSITION_SCHEMA_VERSION,
        search_budget_ms_per_case: millis(search_budget),
        cases,
    })
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct FactoredEndpointComparisonReport {
    pub schema_version: u32,
    pub search_budget_ms_per_formulation: u64,
    pub flattened: IntegratedLayoutReport,
    pub factored: IntegratedLayoutReport,
}

#[allow(clippy::too_many_arguments)]
pub fn compare_first_integrated_layout_phase_factored_endpoints(
    instance_wiring: &FacilityInstanceWiringReport,
    facilities: &ValidatedFacilityCatalog,
    items: &ValidatedItemCatalog,
    transports: &ValidatedTransportCatalog,
    logistics_components: &ValidatedLogisticsComponentCatalog,
    request: &FacilityPlacementRequest,
    search_budget: Duration,
) -> Result<FactoredEndpointComparisonReport, IntegratedLayoutReport> {
    let first_phase_wiring = harness::first_iterative_scc_wiring(instance_wiring)?;
    let input = prepare_exact_model(
        &first_phase_wiring,
        facilities,
        items,
        transports,
        logistics_components,
        request,
    )?;
    let flattened =
        exact::shared_layer::solve(input.clone(), logistics_components, Some(search_budget));
    let factored = exact::shared_layer::solve_factored_endpoints(
        input,
        logistics_components,
        Some(search_budget),
    );
    Ok(FactoredEndpointComparisonReport {
        schema_version: FACTORED_ENDPOINT_COMPARISON_SCHEMA_VERSION,
        search_budget_ms_per_formulation: millis(search_budget),
        flattened,
        factored,
    })
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct SharedLayerComparisonReport {
    pub schema_version: u32,
    pub search_budget_ms_per_formulation: u64,
    pub dense: IntegratedLayoutReport,
    pub shared_layer: IntegratedLayoutReport,
}

#[allow(clippy::too_many_arguments)]
pub fn compare_first_integrated_layout_phase_shared_layer(
    instance_wiring: &FacilityInstanceWiringReport,
    facilities: &ValidatedFacilityCatalog,
    items: &ValidatedItemCatalog,
    transports: &ValidatedTransportCatalog,
    logistics_components: &ValidatedLogisticsComponentCatalog,
    request: &FacilityPlacementRequest,
    search_budget: Duration,
) -> Result<SharedLayerComparisonReport, IntegratedLayoutReport> {
    let first_phase_wiring = harness::first_iterative_scc_wiring(instance_wiring)?;
    let input = prepare_exact_model(
        &first_phase_wiring,
        facilities,
        items,
        transports,
        logistics_components,
        request,
    )?;
    let dense = exact::solve_with_prior_solution(
        input.clone(),
        logistics_components,
        Some(search_budget),
        None,
    );
    let shared_layer = exact::shared_layer::solve(input, logistics_components, Some(search_budget));
    Ok(SharedLayerComparisonReport {
        schema_version: SHARED_LAYER_COMPARISON_SCHEMA_VERSION,
        search_budget_ms_per_formulation: millis(search_budget),
        dense,
        shared_layer,
    })
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum ExactAblationFixation {
    None,
    Placements,
    PlacementsAndTerminals,
    NetworkRoute {
        network_id: String,
    },
    ZeroNetworkArcs {
        network_ids: Vec<String>,
    },
    ReferenceWithZeroNetworkArcs {
        placements: bool,
        terminals: bool,
        network_ids: Vec<String>,
    },
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ExactAblationCaseReport {
    pub id: String,
    pub search_budget_ms: u64,
    pub selected_networks: Vec<String>,
    pub fixation: ExactAblationFixation,
    pub diagnostic_only: bool,
    pub layout: IntegratedLayoutReport,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ExactAblationMatrixReport {
    pub schema_version: u32,
    pub selected_pair: Vec<String>,
    pub case_budget_ms: u64,
    pub reference_budget_ms: u64,
    pub reference_case_id: Option<String>,
    pub cases: Vec<ExactAblationCaseReport>,
}

#[allow(clippy::too_many_arguments)]
pub fn decompose_first_integrated_layout_phase_pair(
    instance_wiring: &FacilityInstanceWiringReport,
    facilities: &ValidatedFacilityCatalog,
    items: &ValidatedItemCatalog,
    transports: &ValidatedTransportCatalog,
    logistics_components: &ValidatedLogisticsComponentCatalog,
    request: &FacilityPlacementRequest,
    network_indices: [usize; 2],
    case_budget: Duration,
    reference_budget: Duration,
) -> Result<ExactAblationMatrixReport, IntegratedLayoutReport> {
    let first_phase_wiring = harness::first_iterative_scc_wiring(instance_wiring)?;
    let input = prepare_exact_model(
        &first_phase_wiring,
        facilities,
        items,
        transports,
        logistics_components,
        request,
    )?;
    let (pair_input, selected_pair) = input
        .clone()
        .select_network_indices(&network_indices)
        .map_err(IntegratedLayoutReport::invalid)?;
    let (first_input, first_network) = input
        .clone()
        .select_network_indices(&[network_indices[0]])
        .map_err(IntegratedLayoutReport::invalid)?;
    let (second_input, second_network) = input
        .select_network_indices(&[network_indices[1]])
        .map_err(IntegratedLayoutReport::invalid)?;

    let mut cases = Vec::new();
    let baseline = run_case(
        "pair-free",
        pair_input.clone(),
        &selected_pair,
        ExactAblationFixation::None,
        None,
        case_budget,
        logistics_components,
    );
    cases.push(baseline.clone());
    let first_single = run_case(
        "single-first-free",
        first_input,
        &first_network,
        ExactAblationFixation::None,
        None,
        case_budget,
        logistics_components,
    );
    cases.push(first_single.clone());
    let second_single = run_case(
        "single-second-free",
        second_input,
        &second_network,
        ExactAblationFixation::None,
        None,
        case_budget,
        logistics_components,
    );
    cases.push(second_single.clone());

    let zero_first = run_case(
        "pair-zero-first-network-arcs",
        pair_input.clone(),
        &selected_pair,
        ExactAblationFixation::ZeroNetworkArcs {
            network_ids: vec![selected_pair[0].clone()],
        },
        None,
        case_budget,
        logistics_components,
    );
    cases.push(zero_first);
    let zero_second = run_case(
        "pair-zero-second-network-arcs",
        pair_input.clone(),
        &selected_pair,
        ExactAblationFixation::ZeroNetworkArcs {
            network_ids: vec![selected_pair[1].clone()],
        },
        None,
        case_budget,
        logistics_components,
    );
    cases.push(zero_second);
    let zero_both = run_case(
        "pair-zero-both-network-arcs",
        pair_input.clone(),
        &selected_pair,
        ExactAblationFixation::ZeroNetworkArcs {
            network_ids: selected_pair.clone(),
        },
        None,
        case_budget,
        logistics_components,
    );
    cases.push(zero_both.clone());

    let mut reference_case_id = baseline.layout.success.then(|| baseline.id.clone());
    let mut reference_layout = baseline.layout.success.then(|| baseline.layout.clone());

    if reference_layout.is_none() && zero_both.layout.success {
        reference_case_id = Some(zero_both.id.clone());
        reference_layout = Some(zero_both.layout.clone());
    }

    if reference_layout.is_none() {
        let extended = run_case(
            "pair-free-reference-budget",
            pair_input.clone(),
            &selected_pair,
            ExactAblationFixation::None,
            None,
            reference_budget,
            logistics_components,
        );
        if extended.layout.success {
            reference_case_id = Some(extended.id.clone());
            reference_layout = Some(extended.layout.clone());
        }
        cases.push(extended);
    }

    if reference_layout.is_none() {
        let placement_source = first_single
            .layout
            .success
            .then_some(&first_single.layout)
            .or_else(|| {
                second_single
                    .layout
                    .success
                    .then_some(&second_single.layout)
            });
        if let Some(placement_source) = placement_source {
            let extended = run_case(
                "pair-placement-reference-budget",
                pair_input.clone(),
                &selected_pair,
                ExactAblationFixation::Placements,
                Some(placement_source),
                reference_budget,
                logistics_components,
            );
            if extended.layout.success {
                reference_case_id = Some(extended.id.clone());
                reference_layout = Some(extended.layout.clone());
            }
            cases.push(extended);
        }
    }

    if let Some(reference) = reference_layout.as_ref() {
        for (id, placements, terminals) in [
            ("reference-check-placement-zero-arcs", true, false),
            ("reference-check-terminals-zero-arcs", false, true),
            ("reference-check-all-zero-arcs", true, true),
        ] {
            cases.push(run_case(
                id,
                pair_input.clone(),
                &selected_pair,
                ExactAblationFixation::ReferenceWithZeroNetworkArcs {
                    placements,
                    terminals,
                    network_ids: selected_pair.clone(),
                },
                Some(reference),
                case_budget,
                logistics_components,
            ));
        }
        for (id, fixation) in [
            ("pair-fixed-placements", ExactAblationFixation::Placements),
            (
                "pair-fixed-placements-terminals",
                ExactAblationFixation::PlacementsAndTerminals,
            ),
            (
                "pair-fixed-first-network-route",
                ExactAblationFixation::NetworkRoute {
                    network_id: selected_pair[0].clone(),
                },
            ),
            (
                "pair-fixed-second-network-route",
                ExactAblationFixation::NetworkRoute {
                    network_id: selected_pair[1].clone(),
                },
            ),
        ] {
            cases.push(run_case(
                id,
                pair_input.clone(),
                &selected_pair,
                fixation,
                Some(reference),
                case_budget,
                logistics_components,
            ));
        }
    }

    Ok(ExactAblationMatrixReport {
        schema_version: EXACT_ABLATION_MATRIX_SCHEMA_VERSION,
        selected_pair,
        case_budget_ms: millis(case_budget),
        reference_budget_ms: millis(reference_budget),
        reference_case_id,
        cases,
    })
}

fn run_case(
    id: &str,
    input: super::ModelInput,
    selected_networks: &[String],
    fixation: ExactAblationFixation,
    reference: Option<&IntegratedLayoutReport>,
    budget: Duration,
    logistics_components: &ValidatedLogisticsComponentCatalog,
) -> ExactAblationCaseReport {
    let layout = exact::solve_with_research_fixation(
        input,
        logistics_components,
        Some(budget),
        reference,
        &fixation,
    );
    ExactAblationCaseReport {
        id: id.to_string(),
        search_budget_ms: millis(budget),
        selected_networks: selected_networks.to_vec(),
        fixation,
        diagnostic_only: true,
        layout,
    }
}

fn millis(duration: Duration) -> u64 {
    duration.as_millis().try_into().unwrap_or(u64::MAX)
}
