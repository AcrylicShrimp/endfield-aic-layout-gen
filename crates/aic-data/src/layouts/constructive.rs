use serde::Serialize;

use crate::layouts::{
    FacilityPlacement, FacilityPlacementBounds, IntegratedLayoutDiagnostic, IntegratedLayoutReport,
    IntegratedLayoutStatus, PlacedFacilityPort, TransportNetwork, TransportNetworkEndpoint,
    TransportNetworkSegment, TransportNetworkTerminal, WorldGridPosition,
    render_integrated_layout_html_with_localization,
};
use crate::localization::ValidatedLocalizationCatalog;
use crate::logistics::TransportKind;
use crate::recipes::Rate;

mod first_pipe_frontier;
mod routing;

pub use first_pipe_frontier::construct_first_pipe_frontier;

pub fn render_constructive_frontier_html(
    report: &ConstructiveFrontierReport,
    localization: Option<&ValidatedLocalizationCatalog>,
) -> Result<String, IntegratedLayoutDiagnostic> {
    let transport_networks = match (
        report.requirement.as_ref(),
        report.item.as_ref(),
        report.rate,
        report.source_port.as_ref(),
        report.target_port.as_ref(),
    ) {
        (Some(requirement), Some(item), Some(rate), Some(source), Some(target)) => {
            vec![TransportNetwork {
                id: format!("constructive:{requirement}"),
                requirement_ids: vec![requirement.clone()],
                item: item.clone(),
                transport: TransportKind::Pipe,
                cells: report.pipe_cells.clone(),
                segments: report
                    .pipe_cells
                    .windows(2)
                    .map(|cells| TransportNetworkSegment {
                        from: cells[0].clone(),
                        to: cells[1].clone(),
                        rate,
                    })
                    .collect(),
                terminals: vec![
                    TransportNetworkTerminal {
                        id: format!("{requirement}:source"),
                        node: source.instance.clone(),
                        direction: source.direction,
                        endpoint: TransportNetworkEndpoint::Facility {
                            instance: source.instance.clone(),
                            port: source.port.clone(),
                        },
                        position: source.connection.clone(),
                        rate,
                    },
                    TransportNetworkTerminal {
                        id: format!("{requirement}:target"),
                        node: target.instance.clone(),
                        direction: target.direction,
                        endpoint: TransportNetworkEndpoint::Facility {
                            instance: target.instance.clone(),
                            port: target.port.clone(),
                        },
                        position: target.connection.clone(),
                        rate,
                    },
                ],
                component_ids: Vec::new(),
            }]
        }
        _ => Vec::new(),
    };
    let integrated = IntegratedLayoutReport {
        schema_version: crate::layouts::INTEGRATED_LAYOUT_SCHEMA_VERSION,
        success: report.success,
        status: if report.success {
            IntegratedLayoutStatus::Feasible
        } else {
            IntegratedLayoutStatus::Unknown
        },
        bounds: report.bounds.clone(),
        placements: report.placements.clone(),
        logistics_components: Vec::new(),
        transport_networks,
        phases: Vec::new(),
        exact: None,
        diagnostics: report
            .diagnostics
            .iter()
            .map(|diagnostic| IntegratedLayoutDiagnostic {
                stage: diagnostic.stage,
                severity: diagnostic.severity,
                code: diagnostic.code,
                path: diagnostic.path.clone(),
                entity: diagnostic.entity.clone(),
                message: diagnostic.message.clone(),
            })
            .collect(),
    };
    render_integrated_layout_html_with_localization(&integrated, localization)
}

const STAGE: &str = "constructive-frontier";
pub const CONSTRUCTIVE_FRONTIER_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum ConstructiveFrontierStatus {
    Constructed,
    InvalidInput,
    NoEligibleFrontier,
    Exhausted,
}

#[derive(Debug, Clone, Default, Serialize, PartialEq, Eq)]
pub struct ConstructiveFrontierStatistics {
    pub seed_placements_considered: u64,
    pub supplier_placements_considered: u64,
    pub overlapping_placements_rejected: u64,
    pub port_pairs_considered: u64,
    pub blocked_port_pairs_rejected: u64,
    pub astar_searches: u64,
    pub astar_failures: u64,
    pub accepted_path_tiles: usize,
    pub accepted_path_turns: usize,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ConstructiveFrontierReport {
    pub schema_version: u32,
    pub success: bool,
    pub status: ConstructiveFrontierStatus,
    pub requirement: Option<String>,
    pub item: Option<String>,
    pub rate: Option<Rate>,
    pub bounds: Option<FacilityPlacementBounds>,
    pub placements: Vec<FacilityPlacement>,
    pub source_port: Option<PlacedFacilityPort>,
    pub target_port: Option<PlacedFacilityPort>,
    pub pipe_cells: Vec<WorldGridPosition>,
    pub statistics: ConstructiveFrontierStatistics,
    pub diagnostics: Vec<ConstructiveFrontierDiagnostic>,
}

impl ConstructiveFrontierReport {
    pub(super) fn failure(
        status: ConstructiveFrontierStatus,
        diagnostic: ConstructiveFrontierDiagnostic,
    ) -> Self {
        Self {
            schema_version: CONSTRUCTIVE_FRONTIER_SCHEMA_VERSION,
            success: false,
            status,
            requirement: None,
            item: None,
            rate: None,
            bounds: None,
            placements: Vec::new(),
            source_port: None,
            target_port: None,
            pipe_cells: Vec::new(),
            statistics: ConstructiveFrontierStatistics::default(),
            diagnostics: vec![diagnostic],
        }
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ConstructiveFrontierDiagnostic {
    pub stage: &'static str,
    pub severity: &'static str,
    pub code: &'static str,
    pub path: String,
    pub entity: Option<String>,
    pub message: String,
}

impl ConstructiveFrontierDiagnostic {
    pub(super) fn error(
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

    pub(super) fn info(code: &'static str, message: impl Into<String>) -> Self {
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
