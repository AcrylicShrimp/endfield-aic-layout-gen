//! Complete Gumbel case search over source-order root catalogs.

use std::{path::Path, sync::Arc, time::Instant};

use anyhow::Result;

use super::{
    collector::{
        CatalogCollectProgress, CatalogCollectStart, CatalogCollectStop, RootCatalogCollector,
    },
    publication::{CatalogBinding, RootCatalogDescriptor, StagedRootCatalog},
    search::{SparseGumbelSearch, SparseSearchStart, SparseSearchStep},
};
use crate::hierarchical_search::{
    Counters, Factory, GumbelAssignment, GumbelControls, MacroDiagnostic, RootSelection,
    SearchMemoryBudget, SearchResult, SearchTermination, Session, bytes_sha256,
    capture_tree_memory_stop, finish_result, root_scoring::RootNoiseMode,
};

pub(in crate::hierarchical_search) struct CatalogRunBinding {
    pub limits_sha256: String,
    pub effective_config_sha256: String,
    pub catalog_sha256: String,
    pub outer_checkpoint_sha256: String,
}

pub(in crate::hierarchical_search) struct CatalogRoot {
    pub state_sha256: String,
    pub state_revision: u64,
    pub descriptor: Arc<RootCatalogDescriptor>,
    pub pending_lower_tasks: usize,
    pub loaded_lower_pages: usize,
}

pub(in crate::hierarchical_search) struct CatalogCaseResult {
    pub result: SearchResult,
    pub roots: Vec<CatalogRoot>,
    pub stages: Vec<StagedRootCatalog>,
}

#[allow(clippy::too_many_arguments)]
pub(in crate::hierarchical_search) fn search_case(
    mut working: Session,
    factory: &Factory<'_>,
    config: &crate::hierarchical_search::Config,
    case_id: &str,
    report_path: &Path,
    retain_roots: bool,
    binding: &CatalogRunBinding,
    budget: &SearchMemoryBudget,
) -> Result<CatalogCaseResult> {
    let RootSelection::GumbelSequentialHalving {
        candidates,
        anchor_candidates,
        anchor_policy_mass,
        seed,
        value_scale,
        max_visit_init,
        gumbel_assignment,
    } = config.root_selection
    else {
        unreachable!("catalog search is only for Gumbel roots")
    };
    let controls = GumbelControls {
        candidates,
        anchor_candidates,
        anchor_policy_mass,
        seed,
        value_scale,
        max_visit_init,
        gumbel_assignment,
    };
    let mode = match gumbel_assignment {
        GumbelAssignment::Rank => RootNoiseMode::Rank,
        GumbelAssignment::ActionIdentity => RootNoiseMode::ActionIdentity,
    };
    let started = Instant::now();
    let mut counters = Counters::default();
    let mut trace = Vec::new();
    let mut roots = Vec::new();
    let mut stages = Vec::new();
    let mut simulations = 0usize;
    let mut termination = SearchTermination::MoveBudgetExhausted;
    let mut no_macro_diagnostic = None;
    for move_index in 0..config.max_moves {
        if working.metrics().is_some() {
            termination = SearchTermination::FullyValidated;
            break;
        }
        let root_stops_before = counters.request_stops_total;
        let revision = working.revision();
        let source = Arc::new(working.clone());
        let prefix = root_prefix(report_path, case_id, move_index);
        let collector = match RootCatalogCollector::new(
            source.as_ref(),
            Arc::clone(&source),
            factory,
            factory.outer.as_ref(),
            factory.device,
            &prefix,
            mode,
            seed,
            case_id,
            candidates,
            anchor_candidates,
            anchor_policy_mass,
            budget,
        )? {
            CatalogCollectStart::Ready(collector) => collector,
            CatalogCollectStart::MemoryStop(stop) => {
                capture_tree_memory_stop(&mut counters, revision, budget, stop.clone());
                no_macro_diagnostic = Some(resource_diagnostic(format!("{stop:?}")));
                termination = SearchTermination::ResourceStopped;
                break;
            }
            CatalogCollectStart::DiskStop(stop) => {
                no_macro_diagnostic = Some(resource_diagnostic(format!("{stop:?}")));
                termination = SearchTermination::ResourceStopped;
                break;
            }
        };
        let mut collector = collector;
        match collector.advance(factory, &mut counters, budget)? {
            CatalogCollectProgress::Ready => {}
            CatalogCollectProgress::NoActions(diagnostic) => {
                no_macro_diagnostic = Some(diagnostic);
                termination = SearchTermination::NoMacroActions;
                break;
            }
            CatalogCollectProgress::Stopped(stop) => {
                if let CatalogCollectStop::Memory(memory) = &stop {
                    capture_tree_memory_stop(&mut counters, revision, budget, memory.clone());
                }
                no_macro_diagnostic = Some(resource_diagnostic(format!("{stop:?}")));
                termination = SearchTermination::ResourceStopped;
                break;
            }
        }
        let mut catalog = collector.finish();
        let sparse = match catalog.sparse_edges(budget)? {
            Ok(sparse) => sparse,
            Err(stop) => {
                capture_tree_memory_stop(&mut counters, revision, budget, stop.clone());
                no_macro_diagnostic = Some(resource_diagnostic(format!("{stop:?}")));
                termination = SearchTermination::ResourceStopped;
                break;
            }
        };
        let mut sparse = Some(sparse);
        let search = match SparseGumbelSearch::new(&catalog, &mut sparse, budget) {
            SparseSearchStart::Ready(search) => search,
            SparseSearchStart::MemoryStop(stop) => {
                capture_tree_memory_stop(&mut counters, revision, budget, stop.clone());
                no_macro_diagnostic = Some(resource_diagnostic(format!("{stop:?}")));
                termination = SearchTermination::ResourceStopped;
                break;
            }
        };
        let mut search = search;
        loop {
            match search.step(
                &mut catalog,
                factory,
                config,
                controls,
                &mut counters,
                budget,
            )? {
                SparseSearchStep::Running => {}
                SparseSearchStep::Complete => break,
                SparseSearchStep::MemoryStop(stop) => {
                    simulations += search.completed_simulations();
                    no_macro_diagnostic = Some(resource_diagnostic(format!("{stop:?}")));
                    termination = SearchTermination::ResourceStopped;
                    break;
                }
            }
        }
        if termination == SearchTermination::ResourceStopped {
            break;
        }
        let simulation = search.finish(&catalog);
        simulations += simulation.simulations;
        let Some(selected) = simulation.selected.clone() else {
            if counters.request_stops_total > root_stops_before {
                termination = SearchTermination::ResourceStopped;
                no_macro_diagnostic = Some(resource_diagnostic(
                    "Gumbel shortlist has no live action after a resource stop".into(),
                ));
            } else {
                termination = SearchTermination::NoMacroActions;
                no_macro_diagnostic = Some(MacroDiagnostic {
                    code: "hierarchical.no-macro-actions".into(),
                    severity: "warning".into(),
                    stage: "hierarchical-search".into(),
                    message: "Gumbel shortlist has no live selected action".into(),
                    entity: "root-catalog".into(),
                });
            }
            break;
        };
        if retain_roots {
            let binding = CatalogBinding {
                source_sha256: catalog.state_identity.clone(),
                source_revision: revision,
                limits_sha256: binding.limits_sha256.clone(),
                effective_config_sha256: binding.effective_config_sha256.clone(),
                catalog_sha256: binding.catalog_sha256.clone(),
                outer_checkpoint_sha256: binding.outer_checkpoint_sha256.clone(),
            };
            let mut catalog = Some(catalog);
            let mut simulation = Some(simulation);
            let stage = match StagedRootCatalog::new(
                &mut catalog,
                &mut simulation,
                &binding,
                report_path,
                case_id,
                value_scale,
                max_visit_init,
                anchor_policy_mass,
                budget,
            )? {
                Ok(stage) => stage,
                Err(stop) => {
                    no_macro_diagnostic = Some(resource_diagnostic(format!("{stop}")));
                    termination = SearchTermination::ResourceStopped;
                    break;
                }
            };
            roots.push(CatalogRoot {
                state_sha256: stage.descriptor.binding.source_sha256.clone(),
                state_revision: revision,
                descriptor: Arc::clone(&stage.descriptor),
                pending_lower_tasks: stage.descriptor.pending_lower_tasks,
                loaded_lower_pages: stage.descriptor.loaded_lower_pages,
            });
            stages.push(stage);
        }
        working = selected.apply(&working, &config.candidate, config.repair_max_removed)?;
        trace.push(selected.summary(revision));
    }
    if working.metrics().is_some() {
        termination = SearchTermination::FullyValidated;
    }
    let result = finish_result(
        "gumbel-mcts",
        working,
        trace,
        Vec::new(),
        simulations,
        counters,
        termination,
        no_macro_diagnostic,
        started,
    );
    Ok(CatalogCaseResult {
        result,
        roots,
        stages,
    })
}

pub(in crate::hierarchical_search) fn root_prefix(
    report_path: &Path,
    case_id: &str,
    move_index: usize,
) -> std::path::PathBuf {
    let parent = report_path.parent().unwrap_or_else(|| Path::new("."));
    let report_name = report_path
        .file_name()
        .unwrap_or_default()
        .to_string_lossy();
    parent.join(format!(
        ".{report_name}.case-{}-move-{move_index:04}",
        bytes_sha256(case_id.as_bytes())
    ))
}

fn resource_diagnostic(message: String) -> MacroDiagnostic {
    MacroDiagnostic {
        code: "hierarchical.resource-limit".into(),
        severity: "warning".into(),
        stage: "hierarchical-search".into(),
        message,
        entity: "root-catalog".into(),
    }
}
