use std::{
    io::Write,
    path::{Path, PathBuf},
};

use aic_data::facilities::{ValidatedFacilityCatalog, load_facility_catalog};
use aic_data::layouts::{FacilityPlacementRequest, analyze_integrated_layout_search_space};
use aic_data::logistics::{
    ValidatedItemCatalog, ValidatedLogisticsComponentCatalog, ValidatedTransportCatalog,
    load_item_catalog, load_logistics_component_catalog, load_transport_catalog,
};
use aic_data::recipes::{
    RecipeSourcePlanRequest, ValidatedRecipeBook, build_contextual_facility_instance_wiring,
    calculate_contextual_facility_requirements, load_recipe_book,
};
use aic_data::research::{
    AnalysisInputFileIdentity, AnalysisInputRole, BenchmarkRequestBounds, BenchmarkWorkloadInputs,
    FormulationIdentity, SEARCH_SPACE_ANALYSIS_SCHEMA_VERSION, SearchSpaceAnalysisReport,
    ValidatedBenchmarkWorkloadManifest, WorkloadIdentity, load_benchmark_workload_manifest,
    validate_benchmark_workload_manifest,
};
use anyhow::{Context, Result, ensure};
use clap::{Subcommand, ValueEnum};
use sha2::{Digest, Sha256};

mod cumulative_growth;
mod external_connectors;
mod factored_networks;
mod first_phase;
mod pair_cliff;
mod physical_occupancy;
mod requirement_cliff;
mod shared_layer;

#[derive(Debug, Clone, Copy, ValueEnum)]
pub(super) enum PortDomainClassificationArg {
    FaithfulBaseline,
    DiagnosticOnly,
}

#[derive(Debug, Subcommand)]
pub(crate) enum ResearchCommand {
    /// Validate a benchmark workload identity without building a solver model.
    ValidateWorkload {
        /// Benchmark workload manifest JSON file to validate.
        #[arg(long, short, value_name = "FILE")]
        file: PathBuf,
    },
    /// Estimate search-space growth without constructing a Pumpkin model.
    AnalyzeWorkload {
        /// Benchmark workload manifest JSON file to analyze.
        #[arg(long, value_name = "FILE")]
        workload: PathBuf,

        /// Root used to resolve portable input paths in the workload manifest.
        #[arg(long, value_name = "DIR", default_value = ".")]
        workspace_root: PathBuf,

        /// Hard maximum layout bounds for this experiment only.
        #[arg(long, value_name = "FILE")]
        placement_request: PathBuf,

        /// Optional JSON artifact path. The full report is always written to stdout.
        #[arg(long, value_name = "FILE")]
        output: Option<PathBuf>,
    },
    /// Solve only cumulative SCC phase 0 for a controlled exact-model experiment.
    SolveFirstPhase {
        /// Benchmark workload manifest JSON file to solve.
        #[arg(long, value_name = "FILE")]
        workload: PathBuf,

        /// Root used to resolve portable input paths in the workload manifest.
        #[arg(long, value_name = "DIR", default_value = ".")]
        workspace_root: PathBuf,

        /// Hard maximum layout bounds for this experiment only.
        #[arg(long, value_name = "FILE")]
        placement_request: PathBuf,

        /// Exact solver wall-clock budget in milliseconds.
        #[arg(long, value_name = "MILLISECONDS")]
        time_limit_ms: u64,

        /// JSON artifact path.
        #[arg(long, value_name = "FILE")]
        output: PathBuf,

        /// Optional standalone HTML result, including structured failure evidence.
        #[arg(long, value_name = "FILE")]
        visualization_output: Option<PathBuf>,
    },
    /// Solve cumulative SCC phases through one target with the v2 exact formulation.
    SolveCumulativeSccGrowth {
        /// Benchmark workload manifest JSON file to solve.
        #[arg(long, value_name = "FILE")]
        workload: PathBuf,

        /// Root used to resolve portable input paths in the workload manifest.
        #[arg(long, value_name = "DIR", default_value = ".")]
        workspace_root: PathBuf,

        /// Hard maximum layout bounds for this experiment only.
        #[arg(long, value_name = "FILE")]
        placement_request: PathBuf,

        /// Zero-based cumulative SCC target phase.
        #[arg(long, value_name = "INDEX")]
        target_phase: usize,

        /// Exact solver wall-clock budget independently given to every phase.
        #[arg(long, value_name = "MILLISECONDS")]
        phase_time_limit_ms: u64,

        /// JSON artifact path.
        #[arg(long, value_name = "FILE")]
        output: PathBuf,

        /// Standalone HTML history or structured failure evidence.
        #[arg(long, value_name = "FILE")]
        visualization_output: PathBuf,
    },
    /// Solve one clean subset of phase-zero external connector requirements.
    SolveFirstPhaseExternalSubset {
        /// Benchmark workload manifest JSON file to solve.
        #[arg(long, value_name = "FILE")]
        workload: PathBuf,

        /// Root used to resolve portable input paths in the workload manifest.
        #[arg(long, value_name = "DIR", default_value = ".")]
        workspace_root: PathBuf,

        /// Hard maximum layout bounds for this experiment only.
        #[arg(long, value_name = "FILE")]
        placement_request: PathBuf,

        /// Zero-based phase-zero route indices. Repeat for every selected requirement.
        #[arg(long = "route-index", value_name = "INDEX", required = true)]
        route_indices: Vec<usize>,

        /// Exact solver wall-clock budget in milliseconds.
        #[arg(long, value_name = "MILLISECONDS")]
        time_limit_ms: u64,

        /// JSON artifact path.
        #[arg(long, value_name = "FILE")]
        output: PathBuf,

        /// Standalone HTML result, including structured failure evidence.
        #[arg(long, value_name = "FILE")]
        visualization_output: PathBuf,
    },
    /// Solve one diagnostic external connector port domain in isolation.
    SolveFirstPhaseExternalPortDomain {
        /// Benchmark workload manifest JSON file to solve.
        #[arg(long, value_name = "FILE")]
        workload: PathBuf,

        /// Root used to resolve portable input paths in the workload manifest.
        #[arg(long, value_name = "DIR", default_value = ".")]
        workspace_root: PathBuf,

        /// Hard maximum layout bounds for this experiment only.
        #[arg(long, value_name = "FILE")]
        placement_request: PathBuf,

        /// Stable label for this isolated matrix case.
        #[arg(long, value_name = "ID")]
        case_id: String,

        /// Whether this case retains the full legal port domain.
        #[arg(long, value_enum)]
        classification: PortDomainClassificationArg,

        /// Zero-based phase-zero route index containing one external requirement.
        #[arg(long, value_name = "INDEX")]
        route_index: usize,

        /// Compatible port ID retained by this case. Repeat for every retained port.
        #[arg(long = "port-id", value_name = "PORT", required = true)]
        port_ids: Vec<String>,

        /// Exact solver wall-clock budget in milliseconds.
        #[arg(long, value_name = "MILLISECONDS")]
        time_limit_ms: u64,

        /// JSON artifact path.
        #[arg(long, value_name = "FILE")]
        output: PathBuf,

        /// Standalone HTML result, including structured failure evidence.
        #[arg(long, value_name = "FILE")]
        visualization_output: PathBuf,
    },
    /// Decompose the first-phase cliff for one exact two-network model.
    DecomposeFirstPhasePair {
        /// Benchmark workload manifest JSON file to solve.
        #[arg(long, value_name = "FILE")]
        workload: PathBuf,

        /// Root used to resolve portable input paths in the workload manifest.
        #[arg(long, value_name = "DIR", default_value = ".")]
        workspace_root: PathBuf,

        /// Hard maximum layout bounds for this experiment only.
        #[arg(long, value_name = "FILE")]
        placement_request: PathBuf,

        /// Two zero-based phase-zero network indices. Repeat this flag twice.
        #[arg(long = "network-index", value_name = "INDEX")]
        network_indices: Vec<usize>,

        /// Wall-clock budget for each comparable ablation case.
        #[arg(long, value_name = "MILLISECONDS")]
        case_time_limit_ms: u64,

        /// Wall-clock budget used only to obtain a validated joint reference.
        #[arg(long, value_name = "MILLISECONDS")]
        reference_time_limit_ms: u64,

        /// Directory receiving summary JSON and per-case JSON/HTML artifacts.
        #[arg(long, value_name = "DIR")]
        output_dir: PathBuf,
    },
    /// Compare the dense and shared-layer exact formulations on cumulative SCC phase 0.
    CompareFirstPhaseSharedLayer {
        /// Benchmark workload manifest JSON file to solve.
        #[arg(long, value_name = "FILE")]
        workload: PathBuf,

        /// Root used to resolve portable input paths in the workload manifest.
        #[arg(long, value_name = "DIR", default_value = ".")]
        workspace_root: PathBuf,

        /// Hard maximum layout bounds for this experiment only.
        #[arg(long, value_name = "FILE")]
        placement_request: PathBuf,

        /// Wall-clock budget given independently to each formulation.
        #[arg(long, value_name = "MILLISECONDS")]
        time_limit_ms: u64,

        /// Directory receiving comparison JSON and dense/shared HTML artifacts.
        #[arg(long, value_name = "DIR")]
        output_dir: PathBuf,
    },
    /// Compare flattened and factored endpoint encodings on the shared-layer phase-0 model.
    CompareFirstPhaseFactoredEndpoints {
        /// Benchmark workload manifest JSON file to solve.
        #[arg(long, value_name = "FILE")]
        workload: PathBuf,

        /// Root used to resolve portable input paths in the workload manifest.
        #[arg(long, value_name = "DIR", default_value = ".")]
        workspace_root: PathBuf,

        /// Hard maximum layout bounds for this experiment only.
        #[arg(long, value_name = "FILE")]
        placement_request: PathBuf,

        /// Wall-clock budget given independently to each formulation.
        #[arg(long, value_name = "MILLISECONDS")]
        time_limit_ms: u64,

        /// Directory receiving comparison JSON and flattened/factored HTML artifacts.
        #[arg(long, value_name = "DIR")]
        output_dir: PathBuf,
    },
    /// Rebuild the factored shared-layer phase-0 model for every network subset size.
    DecomposeFirstPhaseFactoredNetworks {
        /// Benchmark workload manifest JSON file to solve.
        #[arg(long, value_name = "FILE")]
        workload: PathBuf,

        /// Root used to resolve portable input paths in the workload manifest.
        #[arg(long, value_name = "DIR", default_value = ".")]
        workspace_root: PathBuf,

        /// Hard maximum layout bounds for this experiment only.
        #[arg(long, value_name = "FILE")]
        placement_request: PathBuf,

        /// Wall-clock budget given independently to every network-subset case.
        #[arg(long, value_name = "MILLISECONDS")]
        case_time_limit_ms: u64,

        /// Directory receiving summary JSON and per-case JSON/HTML artifacts.
        #[arg(long, value_name = "DIR")]
        output_dir: PathBuf,
    },
    /// Rebuild one factored shared-layer network from each logical requirement subset.
    DecomposeFirstPhaseFactoredRequirements {
        /// Benchmark workload manifest JSON file to solve.
        #[arg(long, value_name = "FILE")]
        workload: PathBuf,

        /// Root used to resolve portable input paths in the workload manifest.
        #[arg(long, value_name = "DIR", default_value = ".")]
        workspace_root: PathBuf,

        /// Hard maximum layout bounds for this experiment only.
        #[arg(long, value_name = "FILE")]
        placement_request: PathBuf,

        /// Zero-based phase-zero commodity network index to decompose.
        #[arg(long, value_name = "INDEX")]
        network_index: usize,

        /// Wall-clock budget given independently to every requirement-subset case.
        #[arg(long, value_name = "MILLISECONDS")]
        case_time_limit_ms: u64,

        /// Directory receiving summary JSON and per-case JSON/HTML artifacts.
        #[arg(long, value_name = "DIR")]
        output_dir: PathBuf,
    },
    /// Measure root propagation through facility and transport occupancy constraints.
    ProbePhysicalOccupancy {
        /// Validated facility catalog JSON file.
        #[arg(long, value_name = "FILE")]
        facility_catalog: PathBuf,

        /// Stable ID of the 5 by 5 facility used by the controlled probe.
        #[arg(long, value_name = "ID")]
        facility_id: String,

        /// Hard maximum layout bounds for this experiment only.
        #[arg(long, value_name = "FILE")]
        placement_request: PathBuf,

        /// JSON artifact path.
        #[arg(long, value_name = "FILE")]
        output: PathBuf,

        /// Self-contained HTML comparison path.
        #[arg(long, value_name = "FILE")]
        visualization_output: PathBuf,
    },
}

pub(crate) fn run(command: ResearchCommand) -> Result<bool> {
    match command {
        ResearchCommand::ValidateWorkload { file } => validate_workload(file),
        ResearchCommand::AnalyzeWorkload {
            workload,
            workspace_root,
            placement_request,
            output,
        } => analyze_workload(workload, workspace_root, placement_request, output),
        ResearchCommand::SolveFirstPhase {
            workload,
            workspace_root,
            placement_request,
            time_limit_ms,
            output,
            visualization_output,
        } => first_phase::solve(
            workload,
            workspace_root,
            placement_request,
            time_limit_ms,
            output,
            visualization_output,
        ),
        ResearchCommand::SolveCumulativeSccGrowth {
            workload,
            workspace_root,
            placement_request,
            target_phase,
            phase_time_limit_ms,
            output,
            visualization_output,
        } => cumulative_growth::solve(
            workload,
            workspace_root,
            placement_request,
            target_phase,
            phase_time_limit_ms,
            output,
            visualization_output,
        ),
        ResearchCommand::SolveFirstPhaseExternalSubset {
            workload,
            workspace_root,
            placement_request,
            route_indices,
            time_limit_ms,
            output,
            visualization_output,
        } => external_connectors::solve(
            workload,
            workspace_root,
            placement_request,
            route_indices,
            time_limit_ms,
            output,
            visualization_output,
        ),
        ResearchCommand::SolveFirstPhaseExternalPortDomain {
            workload,
            workspace_root,
            placement_request,
            case_id,
            classification,
            route_index,
            port_ids,
            time_limit_ms,
            output,
            visualization_output,
        } => external_connectors::solve_port_domain(
            workload,
            workspace_root,
            placement_request,
            case_id,
            classification,
            route_index,
            port_ids,
            time_limit_ms,
            output,
            visualization_output,
        ),
        ResearchCommand::DecomposeFirstPhasePair {
            workload,
            workspace_root,
            placement_request,
            network_indices,
            case_time_limit_ms,
            reference_time_limit_ms,
            output_dir,
        } => pair_cliff::run(
            workload,
            workspace_root,
            placement_request,
            network_indices,
            case_time_limit_ms,
            reference_time_limit_ms,
            output_dir,
        ),
        ResearchCommand::CompareFirstPhaseSharedLayer {
            workload,
            workspace_root,
            placement_request,
            time_limit_ms,
            output_dir,
        } => shared_layer::run(
            workload,
            workspace_root,
            placement_request,
            time_limit_ms,
            output_dir,
            shared_layer::Comparison::SharedLayer,
        ),
        ResearchCommand::CompareFirstPhaseFactoredEndpoints {
            workload,
            workspace_root,
            placement_request,
            time_limit_ms,
            output_dir,
        } => shared_layer::run(
            workload,
            workspace_root,
            placement_request,
            time_limit_ms,
            output_dir,
            shared_layer::Comparison::FactoredEndpoints,
        ),
        ResearchCommand::DecomposeFirstPhaseFactoredNetworks {
            workload,
            workspace_root,
            placement_request,
            case_time_limit_ms,
            output_dir,
        } => factored_networks::run(
            workload,
            workspace_root,
            placement_request,
            case_time_limit_ms,
            output_dir,
        ),
        ResearchCommand::DecomposeFirstPhaseFactoredRequirements {
            workload,
            workspace_root,
            placement_request,
            network_index,
            case_time_limit_ms,
            output_dir,
        } => requirement_cliff::run(
            workload,
            workspace_root,
            placement_request,
            network_index,
            case_time_limit_ms,
            output_dir,
        ),
        ResearchCommand::ProbePhysicalOccupancy {
            facility_catalog,
            facility_id,
            placement_request,
            output,
            visualization_output,
        } => physical_occupancy::run(
            facility_catalog,
            facility_id,
            placement_request,
            output,
            visualization_output,
        ),
    }
}

fn validate_workload(file: PathBuf) -> Result<bool> {
    let manifest = load_benchmark_workload_manifest(&file)?;
    let report = validate_benchmark_workload_manifest(&manifest);
    serde_json::to_writer_pretty(std::io::stdout().lock(), &report)
        .context("failed to write benchmark workload validation report")?;
    println!();
    Ok(report.valid)
}

pub(super) struct ResolvedWorkloadPaths {
    pub(super) recipes: PathBuf,
    pub(super) source_plan: PathBuf,
    pub(super) facility_catalog: PathBuf,
    pub(super) item_catalog: PathBuf,
    pub(super) transport_catalog: PathBuf,
    pub(super) logistics_component_catalog: PathBuf,
    pub(super) localization_catalog: Option<PathBuf>,
}

fn analyze_workload(
    workload_path: PathBuf,
    workspace_root: PathBuf,
    placement_request_path: PathBuf,
    output: Option<PathBuf>,
) -> Result<bool> {
    let manifest = load_benchmark_workload_manifest(&workload_path)?;
    let validated = match ValidatedBenchmarkWorkloadManifest::try_from_manifest(manifest) {
        Ok(validated) => validated,
        Err(report) => {
            serde_json::to_writer_pretty(std::io::stdout().lock(), &report)
                .context("failed to write benchmark workload validation report")?;
            println!();
            return Ok(false);
        }
    };
    let manifest = validated.manifest();
    let paths = resolve_workload_paths(&workspace_root, &manifest.inputs);
    let (book, source_plan) = load_contextual_recipe_request(&paths.recipes, &paths.source_plan)?;
    ensure!(
        source_plan.target.item == manifest.expected_target.item
            && source_plan.target.quantity == manifest.expected_target.quantity
            && source_plan.target.duration_ms == manifest.expected_target.duration_ms,
        "benchmark workload '{}' expected target does not match source plan '{}'",
        manifest.id,
        paths.source_plan.display()
    );
    let throughput = book.calculate_contextual_throughput(&source_plan);
    ensure!(throughput.success, "benchmark contextual throughput failed");
    let requirements = calculate_contextual_facility_requirements(&throughput);
    ensure!(
        requirements.success,
        "benchmark facility requirements failed"
    );
    let wiring = build_contextual_facility_instance_wiring(&throughput, &requirements);
    ensure!(wiring.success, "benchmark facility instance wiring failed");

    let facilities =
        ValidatedFacilityCatalog::try_from_catalog(load_facility_catalog(&paths.facility_catalog)?)
            .map_err(|report| anyhow::anyhow!("facility catalog validation failed: {report:?}"))?;
    let items = ValidatedItemCatalog::try_from_catalog(load_item_catalog(&paths.item_catalog)?)
        .map_err(|report| anyhow::anyhow!("item catalog validation failed: {report:?}"))?;
    let transports = ValidatedTransportCatalog::try_from_catalog(load_transport_catalog(
        &paths.transport_catalog,
    )?)
    .map_err(|report| anyhow::anyhow!("transport catalog validation failed: {report:?}"))?;
    let components = ValidatedLogisticsComponentCatalog::try_from_catalog(
        load_logistics_component_catalog(&paths.logistics_component_catalog)?,
    )
    .map_err(|report| anyhow::anyhow!("logistics component validation failed: {report:?}"))?;
    let placement_request_path = workspace_root.join(placement_request_path);
    let placement_request_json =
        std::fs::read_to_string(&placement_request_path).with_context(|| {
            format!(
                "failed to read research placement request '{}'",
                placement_request_path.display()
            )
        })?;
    let placement_request = serde_json::from_str::<FacilityPlacementRequest>(
        &placement_request_json,
    )
    .with_context(|| {
        format!(
            "failed to parse research placement request '{}'",
            placement_request_path.display()
        )
    })?;
    let static_analysis = analyze_integrated_layout_search_space(
        &wiring,
        &facilities,
        &items,
        &transports,
        &components,
        &placement_request,
    )
    .map_err(|diagnostic| {
        anyhow::anyhow!(
            "search-space analysis failed with {}: {}",
            diagnostic.code,
            diagnostic.message
        )
    })?;

    let mut input_identities = input_identities(&paths)?;
    input_identities.push(AnalysisInputFileIdentity {
        role: AnalysisInputRole::PlacementRequest,
        path: placement_request_path.display().to_string(),
        sha256: file_sha256(&placement_request_path)?,
    });
    let report = SearchSpaceAnalysisReport {
        schema_version: SEARCH_SPACE_ANALYSIS_SCHEMA_VERSION,
        workload: WorkloadIdentity {
            workload_id: manifest.id.clone(),
            manifest_sha256: validated.manifest_sha256().to_string(),
            inputs: input_identities,
        },
        formulation: FormulationIdentity {
            formulation: "joint-lexicographic-layout-v5".to_string(),
            solver: "pumpkin".to_string(),
            solver_version: "0.5".to_string(),
            source_revision: None,
            configuration_sha256: text_sha256(
                "joint-lexicographic-layout-v5:iterative-scc-one-ready:circulation-permitted",
            ),
        },
        request_bounds: BenchmarkRequestBounds {
            max_width: u32::try_from(placement_request.max_width)
                .context("research max_width does not fit report domain")?,
            max_height: u32::try_from(placement_request.max_height)
                .context("research max_height does not fit report domain")?,
        },
        ir: static_analysis.ir,
        model_estimate: static_analysis.model_estimate,
        model_actual: None,
        estimate_error: None,
        diagnostics: static_analysis.diagnostics,
    };
    let encoded = serde_json::to_vec_pretty(&report)
        .context("failed to serialize search-space analysis report")?;
    if let Some(output) = output {
        if let Some(parent) = output
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
        {
            std::fs::create_dir_all(parent).with_context(|| {
                format!(
                    "failed to create analysis output directory '{}'",
                    parent.display()
                )
            })?;
        }
        std::fs::write(&output, &encoded)
            .with_context(|| format!("failed to write analysis report '{}'", output.display()))?;
    }
    std::io::stdout()
        .lock()
        .write_all(&encoded)
        .context("failed to write search-space analysis report")?;
    println!();
    Ok(true)
}

pub(super) fn load_contextual_recipe_request(
    recipes: &Path,
    source_plan: &Path,
) -> Result<(ValidatedRecipeBook, RecipeSourcePlanRequest)> {
    let recipe_book = load_recipe_book(recipes)?;
    let book = ValidatedRecipeBook::try_from_recipe_book(recipe_book)
        .map_err(|report| anyhow::anyhow!("recipe validation failed: {report:?}"))?;
    let source_plan_json = std::fs::read_to_string(source_plan).with_context(|| {
        format!(
            "failed to read recipe source-plan request '{}'",
            source_plan.display()
        )
    })?;
    let source_plan = serde_json::from_str(&source_plan_json).with_context(|| {
        format!(
            "failed to parse recipe source-plan request '{}'",
            source_plan.display()
        )
    })?;
    Ok((book, source_plan))
}

pub(super) fn resolve_workload_paths(
    workspace_root: &Path,
    inputs: &BenchmarkWorkloadInputs,
) -> ResolvedWorkloadPaths {
    ResolvedWorkloadPaths {
        recipes: workspace_root.join(&inputs.recipes),
        source_plan: workspace_root.join(&inputs.source_plan),
        facility_catalog: workspace_root.join(&inputs.facility_catalog),
        item_catalog: workspace_root.join(&inputs.item_catalog),
        transport_catalog: workspace_root.join(&inputs.transport_catalog),
        logistics_component_catalog: workspace_root.join(&inputs.logistics_component_catalog),
        localization_catalog: inputs
            .localization_catalog
            .as_ref()
            .map(|path| workspace_root.join(path)),
    }
}

fn input_identities(paths: &ResolvedWorkloadPaths) -> Result<Vec<AnalysisInputFileIdentity>> {
    let mut inputs = vec![
        input_identity(AnalysisInputRole::Recipes, &paths.recipes)?,
        input_identity(AnalysisInputRole::SourcePlan, &paths.source_plan)?,
        input_identity(AnalysisInputRole::FacilityCatalog, &paths.facility_catalog)?,
        input_identity(AnalysisInputRole::ItemCatalog, &paths.item_catalog)?,
        input_identity(
            AnalysisInputRole::TransportCatalog,
            &paths.transport_catalog,
        )?,
        input_identity(
            AnalysisInputRole::LogisticsComponentCatalog,
            &paths.logistics_component_catalog,
        )?,
    ];
    if let Some(localization) = &paths.localization_catalog {
        inputs.push(input_identity(
            AnalysisInputRole::LocalizationCatalog,
            localization,
        )?);
    }
    Ok(inputs)
}

fn input_identity(role: AnalysisInputRole, path: &Path) -> Result<AnalysisInputFileIdentity> {
    Ok(AnalysisInputFileIdentity {
        role,
        path: path.display().to_string(),
        sha256: file_sha256(path)?,
    })
}

fn file_sha256(path: &Path) -> Result<String> {
    let bytes = std::fs::read(path)
        .with_context(|| format!("failed to hash research input '{}'", path.display()))?;
    Ok(hex_sha256(Sha256::digest(bytes)))
}

fn text_sha256(value: &str) -> String {
    hex_sha256(Sha256::digest(value.as_bytes()))
}

fn hex_sha256(digest: impl AsRef<[u8]>) -> String {
    digest
        .as_ref()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
