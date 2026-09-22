//! Argument parsing only; implementation lives in the learning/backend crates.
#[cfg(feature = "learning")]
use aic_learning::update;
use anyhow::Result;
use clap::Subcommand;
use std::path::PathBuf;

#[derive(Debug, Subcommand)]
pub enum LearningCommand {
    /// Evaluate diverse native-valid routes on frozen held-out construction states.
    RouteCandidateHarness {
        #[arg(long)]
        panel: PathBuf,
        /// Directory containing normalized facilities, transports and logistics components.
        #[arg(long)]
        catalog_root: PathBuf,
        #[arg(long)]
        output: PathBuf,
    },
    /// Warm-start dense exact-search guidance from frozen continuation outcomes.
    #[cfg(feature = "learning")]
    TrainRouteGuidance {
        #[arg(long)]
        config: PathBuf,
        #[arg(long)]
        training_panel: PathBuf,
        #[arg(long)]
        evaluation_panel: PathBuf,
        /// Directory containing normalized facilities, transports and logistics components.
        #[arg(long)]
        catalog_root: PathBuf,
        #[arg(long)]
        output: PathBuf,
        #[arg(long, default_value = "cuda:0")]
        device: String,
    },
    /// Compare beam search and stable-state MCTS over complete macro actions.
    #[cfg(feature = "learning")]
    HierarchicalSearchHarness {
        #[arg(long)]
        config: PathBuf,
        #[arg(long)]
        panel: PathBuf,
        /// Completed train-route-guidance output directory.
        #[arg(long)]
        lower_training_output: PathBuf,
        /// Completed train-hierarchical-outer output directory.
        #[arg(long)]
        outer_training_output: PathBuf,
        /// Directory containing normalized facilities, transports and logistics components.
        #[arg(long)]
        catalog_root: PathBuf,
        #[arg(long)]
        output: PathBuf,
        #[arg(long, default_value = "cuda:0")]
        device: String,
    },
    /// Train the separate outer macro policy/value model from exact continuations.
    #[cfg(feature = "learning")]
    TrainHierarchicalOuter {
        #[arg(long)]
        config: PathBuf,
        #[arg(long)]
        training_panel: PathBuf,
        #[arg(long)]
        evaluation_panel: PathBuf,
        /// Completed train-route-guidance output directory.
        #[arg(long)]
        lower_training_output: PathBuf,
        #[arg(long)]
        catalog_root: PathBuf,
        #[arg(long)]
        output: PathBuf,
        #[arg(long, default_value = "cuda:0")]
        device: String,
    },
    /// Distill one frozen MCTS collection into the next lower and outer generation.
    #[cfg(feature = "learning")]
    TrainHierarchicalGeneration {
        #[arg(long)]
        config: PathBuf,
        #[arg(long)]
        panel: PathBuf,
        /// Hierarchical search report containing frozen root visit records.
        #[arg(long)]
        collection: PathBuf,
        #[arg(long)]
        lower_training_output: PathBuf,
        #[arg(long)]
        outer_training_output: PathBuf,
        #[arg(long)]
        catalog_root: PathBuf,
        #[arg(long)]
        output: PathBuf,
        #[arg(long, default_value = "cuda:0")]
        device: String,
        /// Resume the latest atomic epoch-boundary checkpoint in the pending output.
        #[arg(long)]
        resume: bool,
    },
    /// Run fresh-graph hierarchical generations with training-success curriculum promotion.
    #[cfg(feature = "learning")]
    TrainHierarchicalCurriculum {
        #[arg(long)]
        config: PathBuf,
        #[arg(long)]
        search_config: PathBuf,
        #[arg(long)]
        generation_config: PathBuf,
        #[arg(long)]
        sampling: PathBuf,
        /// Fixed held-out panel; its outcomes never affect curriculum promotion.
        #[arg(long)]
        heldout_panel: PathBuf,
        #[arg(long)]
        initial_lower_output: PathBuf,
        #[arg(long)]
        initial_outer_output: PathBuf,
        #[arg(long)]
        catalog_root: PathBuf,
        #[arg(long)]
        output: PathBuf,
        #[arg(long, default_value = "cuda:0")]
        device: String,
        #[arg(long)]
        resume: bool,
    },
    /// Prepare empty-prefix hierarchical cases from real production graphs.
    #[cfg(feature = "learning")]
    PrepareHierarchicalPanel {
        #[arg(long)]
        sampling: PathBuf,
        /// Directory containing normalized recipes, items and physical catalogs.
        #[arg(long)]
        catalog_root: PathBuf,
        #[arg(long)]
        output: PathBuf,
        /// Panel partition: training or held-out.
        #[arg(long)]
        partition: String,
        #[arg(long)]
        cases: usize,
        #[arg(long)]
        seed: u64,
        #[arg(long)]
        namespace: String,
        #[arg(long, default_value_t = 1)]
        min_facilities: usize,
        #[arg(long, default_value_t = 2)]
        max_facilities: usize,
        #[arg(long, default_value_t = 6)]
        max_terminals: usize,
        #[arg(long, default_value_t = 8)]
        max_connections: usize,
        #[arg(long, default_value_t = 10_000)]
        max_turns: u64,
        #[arg(long, default_value_t = 32)]
        max_extension: i64,
    },
    /// Collect the fixed bounded procedural physical-action outcome dataset.
    CollectOutcomes {
        #[arg(long)]
        config: PathBuf,
        /// Directory containing normalized facilities, transports and logistics components.
        #[arg(long)]
        catalog_root: PathBuf,
        #[arg(long)]
        output: PathBuf,
    },
    /// Explicitly reload and verify a published physical-action outcome dataset.
    ValidateOutcomes {
        #[arg(long)]
        dataset: PathBuf,
    },
    /// Train and evaluate the fixed physical-action outcome predictor.
    #[cfg(feature = "learning")]
    PretrainOutcomes {
        #[arg(long)]
        config: PathBuf,
        #[arg(long)]
        dataset: PathBuf,
        #[arg(long)]
        output: PathBuf,
        #[arg(long, default_value = "cuda:0")]
        device: String,
    },
    /// Train the physical outcome encoder from disposable live-generated roots.
    #[cfg(feature = "learning")]
    PretrainOutcomesOnline {
        #[arg(long)]
        config: PathBuf,
        /// Directory containing normalized facilities, transports and logistics components.
        #[arg(long)]
        catalog_root: PathBuf,
        #[arg(long)]
        output: PathBuf,
        #[arg(long, default_value = "cuda:0")]
        device: String,
        /// Stop after this total completed update count; useful for readiness runs.
        #[arg(long)]
        updates: Option<u64>,
        /// Restore the latest committed update from this output directory.
        #[arg(long)]
        resume: bool,
        #[arg(long)]
        workers: Option<usize>,
    },
    /// Measure all physical-outcome heads on held-out factory inputs.
    #[cfg(feature = "learning")]
    EvaluateOutcomeHeads {
        #[arg(long)]
        config: PathBuf,
        /// Directory containing the five normalized runtime catalog files.
        #[arg(long)]
        catalog_root: PathBuf,
        #[arg(long)]
        panel: PathBuf,
        #[arg(long)]
        checkpoint: PathBuf,
        #[arg(long)]
        output: PathBuf,
        #[arg(long, default_value = "cuda:0")]
        device: String,
    },
    /// Recompute physical reservations from verified v1 panel payloads into a new directory.
    #[cfg(feature = "learning")]
    MigratePanel {
        #[arg(long)]
        input: PathBuf,
        #[arg(long)]
        output: PathBuf,
        /// JSON file containing max_individualization_nodes.
        #[arg(long)]
        identity_limits: PathBuf,
        /// Directory containing the five normalized runtime catalog files.
        #[arg(long)]
        catalog_root: PathBuf,
    },
    /// Ask an active training run to save and stop at its next complete wave.
    #[cfg(feature = "learning")]
    Stop {
        #[arg(long)]
        output: PathBuf,
    },
    /// Train the native policy through full PPO epochs, replay and evaluation.
    #[cfg(feature = "learning")]
    Train {
        #[arg(long)]
        config: PathBuf,
        #[arg(long)]
        output: PathBuf,
        #[arg(long, default_value = "cuda:0")]
        device: String,
        /// Total completed update target; otherwise stop at the configured duration.
        #[arg(long)]
        updates: Option<u64>,
        /// Restore this output's latest completed training checkpoint.
        #[arg(long)]
        resume: bool,
        #[arg(long)]
        workers: Option<usize>,
    },
    /// Generate an immutable held-out panel with a separate seed namespace.
    #[cfg(feature = "learning")]
    PreparePanel {
        #[arg(long)]
        config: PathBuf,
        #[arg(long)]
        output: PathBuf,
        #[arg(long)]
        cases: usize,
        #[arg(long)]
        seed: u64,
    },
    /// Train on freshly sampled full production graphs and a fixed held-out panel.
    #[cfg(feature = "learning")]
    TrainConstructive {
        #[arg(long)]
        config: PathBuf,
        #[arg(long)]
        sampling: PathBuf,
        /// Directory containing normalized recipes, items and physical catalogs.
        #[arg(long)]
        catalog_root: PathBuf,
        #[arg(long)]
        output: PathBuf,
        #[arg(long, default_value = "auto")]
        device: String,
    },
    /// Train one-connection route options with policy-generated partial occupancy.
    #[cfg(feature = "learning")]
    TrainConstructiveRoutes {
        #[arg(long)]
        config: PathBuf,
        /// Directory containing normalized facilities, transports and logistics components.
        #[arg(long)]
        catalog_root: PathBuf,
        #[arg(long)]
        output: PathBuf,
        #[arg(long, default_value = "auto")]
        device: String,
    },
    /// Probe a frozen route checkpoint at its first crossed continuation frontier.
    #[cfg(feature = "learning")]
    ProbeConstructiveRouteFrontier {
        /// Completed train-constructive-routes output directory.
        #[arg(long)]
        training_output: PathBuf,
        #[arg(long)]
        probe_config: PathBuf,
        /// Directory containing normalized facilities, transports and logistics components.
        #[arg(long)]
        catalog_root: PathBuf,
        #[arg(long)]
        output: PathBuf,
        #[arg(long, default_value = "auto")]
        device: String,
    },
    /// Render greedy held-out layouts produced by a constructive checkpoint.
    #[cfg(feature = "learning")]
    EvaluateConstructive {
        /// Completed train-constructive output directory.
        #[arg(long)]
        training_output: PathBuf,
        /// Directory containing normalized facilities, transports and logistics components.
        #[arg(long)]
        catalog_root: PathBuf,
        #[arg(long)]
        output: PathBuf,
        #[arg(long, default_value_t = 8)]
        cases: usize,
        #[arg(long, default_value = "auto")]
        device: String,
    },
    /// Search one explicit production graph inside a fixed W x H bound with policy/value MCTS.
    #[cfg(feature = "learning")]
    SearchConstructive {
        /// Completed train-constructive output directory.
        #[arg(long)]
        training_output: PathBuf,
        /// Explicit recipe source plan compiled through the training problem pipeline.
        #[arg(long)]
        source_plan: PathBuf,
        /// Directory containing normalized recipes, items and physical catalogs.
        #[arg(long)]
        catalog_root: PathBuf,
        #[arg(long)]
        output: PathBuf,
        #[arg(long)]
        width: i64,
        #[arg(long)]
        height: i64,
        #[arg(long, default_value_t = 32)]
        simulations_per_move: usize,
        /// Maximum simulations used when no branch has yet demonstrated physical progress.
        #[arg(long, default_value_t = 256)]
        proof_simulations_per_move: usize,
        #[arg(long, default_value_t = 16)]
        horizon: usize,
        #[arg(long, default_value_t = 32)]
        max_actions_per_widening: usize,
        #[arg(long, default_value_t = 4)]
        initial_children: usize,
        #[arg(long, default_value_t = 1.0)]
        widening: f64,
        #[arg(long, default_value_t = 1.25)]
        exploration: f64,
        /// Potential-based route guidance; zero disables Manhattan shaping.
        #[arg(long, default_value_t = 1.0)]
        route_potential_scale: f64,
        #[arg(long, default_value_t = 8192)]
        max_policy_evaluations: usize,
        #[arg(long, default_value_t = 910_021)]
        baseline_seed: u64,
        /// CPU workers for native action-bank construction and policy scoring.
        #[arg(long, default_value_t = 8)]
        cpu_threads: usize,
        #[arg(long, default_value = "auto")]
        device: String,
    },
}
pub fn run(command: LearningCommand) -> Result<()> {
    let config = match &command {
        LearningCommand::RouteCandidateHarness { panel, .. } => panel,
        #[cfg(feature = "learning")]
        LearningCommand::TrainRouteGuidance { config, .. } => config,
        #[cfg(feature = "learning")]
        LearningCommand::HierarchicalSearchHarness { config, .. } => config,
        #[cfg(feature = "learning")]
        LearningCommand::TrainHierarchicalOuter { config, .. } => config,
        #[cfg(feature = "learning")]
        LearningCommand::TrainHierarchicalGeneration { config, .. } => config,
        #[cfg(feature = "learning")]
        LearningCommand::TrainHierarchicalCurriculum { config, .. } => config,
        #[cfg(feature = "learning")]
        LearningCommand::PrepareHierarchicalPanel { sampling, .. } => sampling,
        LearningCommand::CollectOutcomes { config, .. } => config,
        LearningCommand::ValidateOutcomes { dataset } => dataset,
        #[cfg(feature = "learning")]
        LearningCommand::PretrainOutcomes { config, .. } => config,
        #[cfg(feature = "learning")]
        LearningCommand::PretrainOutcomesOnline { config, .. } => config,
        #[cfg(feature = "learning")]
        LearningCommand::EvaluateOutcomeHeads { config, .. } => config,
        #[cfg(feature = "learning")]
        LearningCommand::MigratePanel { input: config, .. } => config,
        #[cfg(feature = "learning")]
        LearningCommand::Stop { output: config } => config,
        #[cfg(feature = "learning")]
        LearningCommand::Train { config, .. } => config,
        #[cfg(feature = "learning")]
        LearningCommand::PreparePanel { config, .. } => config,
        #[cfg(feature = "learning")]
        LearningCommand::TrainConstructive { config, .. } => config,
        #[cfg(feature = "learning")]
        LearningCommand::TrainConstructiveRoutes { config, .. } => config,
        #[cfg(feature = "learning")]
        LearningCommand::ProbeConstructiveRouteFrontier { probe_config, .. } => probe_config,
        #[cfg(feature = "learning")]
        LearningCommand::EvaluateConstructive {
            training_output: config,
            ..
        } => config,
        #[cfg(feature = "learning")]
        LearningCommand::SearchConstructive { source_plan, .. } => source_plan,
    };
    let entity = config.display().to_string();
    let report = execute(command).map_err(|error| {
        if let Some(diagnostic) = error.chain().find_map(|cause| {
            let value: serde_json::Value = serde_json::from_str(&cause.to_string()).ok()?;
            (value["code"].is_string() && value["severity"].is_string()
                && value["stage"].is_string() && value["entity"].is_string()).then_some(value)
        }) {
            return anyhow::anyhow!(diagnostic.to_string());
        }
        let message = format!("{error:#}");
        let code = error.chain().find_map(|cause| {
            let text = cause.to_string();
            let prefix = text.split(':').next()?;
            (prefix.starts_with("aic-learning.") || prefix.starts_with("aic-torch.")).then(|| prefix.to_owned())
        }).unwrap_or_else(|| "aic-learning.failure".into());
        anyhow::anyhow!(serde_json::json!({"code":code,"severity":"error","stage":"native-learning","message":message,"entity":entity}).to_string())
    })?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}
fn execute(command: LearningCommand) -> Result<serde_json::Value> {
    match command {
        LearningCommand::RouteCandidateHarness {
            panel,
            catalog_root,
            output,
        } => aic_learning::route_candidates::run(&panel, &catalog_root, &output),
        #[cfg(feature = "learning")]
        LearningCommand::TrainRouteGuidance {
            config,
            training_panel,
            evaluation_panel,
            catalog_root,
            output,
            device,
        } => aic_learning::route_guidance_training::run(
            &config,
            &training_panel,
            &evaluation_panel,
            &catalog_root,
            &output,
            update::parse_device(&device)?,
        ),
        #[cfg(feature = "learning")]
        LearningCommand::HierarchicalSearchHarness {
            config,
            panel,
            lower_training_output,
            outer_training_output,
            catalog_root,
            output,
            device,
        } => aic_learning::hierarchical_search::run(
            &config,
            &panel,
            &lower_training_output,
            &outer_training_output,
            &catalog_root,
            &output,
            update::parse_device(&device)?,
        ),
        #[cfg(feature = "learning")]
        LearningCommand::TrainHierarchicalOuter {
            config,
            training_panel,
            evaluation_panel,
            lower_training_output,
            catalog_root,
            output,
            device,
        } => aic_learning::outer_training::run(
            &config,
            &training_panel,
            &evaluation_panel,
            &lower_training_output,
            &catalog_root,
            &output,
            update::parse_device(&device)?,
        ),
        #[cfg(feature = "learning")]
        LearningCommand::TrainHierarchicalGeneration {
            config,
            panel,
            collection,
            lower_training_output,
            outer_training_output,
            catalog_root,
            output,
            device,
            resume,
        } => aic_learning::hierarchical_generation::run(
            &config,
            &panel,
            &collection,
            &lower_training_output,
            &outer_training_output,
            &catalog_root,
            &output,
            update::parse_device(&device)?,
            resume,
        ),
        #[cfg(feature = "learning")]
        LearningCommand::TrainHierarchicalCurriculum {
            config,
            search_config,
            generation_config,
            sampling,
            heldout_panel,
            initial_lower_output,
            initial_outer_output,
            catalog_root,
            output,
            device,
            resume,
        } => aic_learning::hierarchical_curriculum::run(
            &config,
            &search_config,
            &generation_config,
            &sampling,
            &heldout_panel,
            &initial_lower_output,
            &initial_outer_output,
            &catalog_root,
            &output,
            update::parse_device(&device)?,
            resume,
        ),
        #[cfg(feature = "learning")]
        LearningCommand::PrepareHierarchicalPanel {
            sampling,
            catalog_root,
            output,
            partition,
            cases,
            seed,
            namespace,
            min_facilities,
            max_facilities,
            max_terminals,
            max_connections,
            max_turns,
            max_extension,
        } => aic_learning::hierarchical_search::prepare_panel(
            &sampling,
            &catalog_root,
            &output,
            &partition,
            cases,
            seed,
            &namespace,
            min_facilities,
            max_facilities,
            max_terminals,
            max_connections,
            max_turns,
            max_extension,
        ),
        LearningCommand::CollectOutcomes {
            config,
            catalog_root,
            output,
        } => aic_learning::outcomes::collect(&config, &catalog_root, &output),
        LearningCommand::ValidateOutcomes { dataset } => aic_learning::outcomes::validate(&dataset),
        #[cfg(feature = "learning")]
        LearningCommand::PretrainOutcomes {
            config,
            dataset,
            output,
            device,
        } => aic_learning::pretraining::run(
            &config,
            &dataset,
            &output,
            update::parse_device(&device)?,
        ),
        #[cfg(feature = "learning")]
        LearningCommand::PretrainOutcomesOnline {
            config,
            catalog_root,
            output,
            device,
            updates,
            resume,
            workers,
        } => aic_learning::pretraining::run_online(
            &config,
            &catalog_root,
            &output,
            update::parse_device(&device)?,
            updates,
            resume,
            workers,
        ),
        #[cfg(feature = "learning")]
        LearningCommand::EvaluateOutcomeHeads {
            config,
            catalog_root,
            panel,
            checkpoint,
            output,
            device,
        } => aic_learning::pretraining::evaluate(
            &config,
            &catalog_root,
            &panel,
            &checkpoint,
            &output,
            update::parse_device(&device)?,
        ),
        #[cfg(feature = "learning")]
        LearningCommand::MigratePanel {
            input,
            output,
            identity_limits,
            catalog_root,
        } => aic_learning::training::evaluation::migrate(
            &input,
            &output,
            &identity_limits,
            &catalog_root,
        ),
        #[cfg(feature = "learning")]
        LearningCommand::Stop { output } => {
            anyhow::ensure!(
                output.is_dir(),
                "aic-learning.stop-output: missing {}",
                output.display()
            );
            std::fs::write(output.join("STOP"), b"requested\n")?;
            Ok(
                serde_json::json!({"schema":"aic-training-stop-v1","status":"requested","output":output}),
            )
        }
        #[cfg(feature = "learning")]
        LearningCommand::Train {
            config,
            output,
            device,
            updates,
            resume,
            workers,
        } => aic_learning::training::run::run(
            &config,
            &output,
            update::parse_device(&device)?,
            updates,
            resume,
            workers,
        ),
        #[cfg(feature = "learning")]
        LearningCommand::PreparePanel {
            config,
            output,
            cases,
            seed,
        } => aic_learning::training::evaluation::prepare(&config, &output, cases, seed),
        #[cfg(feature = "learning")]
        LearningCommand::TrainConstructive {
            config,
            sampling,
            catalog_root,
            output,
            device,
        } => aic_learning::constructive::run::run(
            &config,
            &sampling,
            &catalog_root,
            &output,
            update::parse_device(&device)?,
        ),
        #[cfg(feature = "learning")]
        LearningCommand::TrainConstructiveRoutes {
            config,
            catalog_root,
            output,
            device,
        } => aic_learning::constructive::route::run(
            &config,
            &catalog_root,
            &output,
            update::parse_device(&device)?,
        ),
        #[cfg(feature = "learning")]
        LearningCommand::ProbeConstructiveRouteFrontier {
            training_output,
            probe_config,
            catalog_root,
            output,
            device,
        } => aic_learning::constructive::route::probe_frontier(
            &training_output,
            &probe_config,
            &catalog_root,
            &output,
            update::parse_device(&device)?,
        ),
        #[cfg(feature = "learning")]
        LearningCommand::EvaluateConstructive {
            training_output,
            catalog_root,
            output,
            cases,
            device,
        } => aic_learning::constructive::run::evaluate_checkpoint(
            &training_output,
            &catalog_root,
            &output,
            cases,
            update::parse_device(&device)?,
        ),
        #[cfg(feature = "learning")]
        LearningCommand::SearchConstructive {
            training_output,
            source_plan,
            catalog_root,
            output,
            width,
            height,
            simulations_per_move,
            proof_simulations_per_move,
            horizon,
            max_actions_per_widening,
            initial_children,
            widening,
            exploration,
            route_potential_scale,
            max_policy_evaluations,
            baseline_seed,
            cpu_threads,
            device,
        } => aic_learning::constructive::search::run(
            &training_output,
            &catalog_root,
            &source_plan,
            &output,
            aic_learning::constructive::search::Config {
                width,
                height,
                simulations_per_move,
                proof_simulations_per_move,
                horizon,
                max_actions_per_widening,
                initial_children,
                widening,
                exploration,
                route_potential_scale,
                max_policy_evaluations,
                baseline_seed,
                cpu_threads,
            },
            update::parse_device(&device)?,
        ),
    }
}
