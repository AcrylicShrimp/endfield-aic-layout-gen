use super::{
    config::Config,
    model::{Batch, Labels, Predictor, is_encoder, labels_for, masked_loss},
    run::partition_roots,
};
use crate::outcomes::{
    config::Split,
    sample::{Assignment, SelectedAction},
    storage::{ObservedOutcome, Row, WorkRecord},
};
use aic_layout::physical::Metrics;
use aic_pilot_native::learning::{Input, schema::*};
use aic_torch::tch::{Device, Kind, Tensor};

fn metrics(area: u64, transport_cells: u64, turns: u64) -> Metrics {
    Metrics {
        width: 1,
        height: 1,
        area,
        body_cells: 1,
        transport_cells,
        turns,
        occupancy: 1.0,
        weak_components: 1,
        branch_blocks: 0,
        merge_blocks: 0,
        max_degree: 1,
        mean_port_distance: 0.0,
        mean_detour: 0.0,
        belt_connections: 0,
        pipe_connections: 0,
        bridges: 0,
    }
}

fn outcome(status: &str, censored: bool) -> ObservedOutcome {
    let changed = matches!(status, "draft_accepted" | "validated" | "improved");
    ObservedOutcome {
        status: status.into(),
        charged: true,
        censored,
        attempts: 1,
        revision: 1,
        done: status != "draft_accepted",
        work: WorkRecord {
            expanded: 0,
            preflight_expanded: 0,
            route_calls: 0,
            pending: 0,
            routes: vec![],
            committed: changed,
        },
        best_metrics: metrics(100, 20, 10),
        current_pending: changed.then_some(2),
        current_conflicts: changed.then_some(1),
        current_metrics: changed.then(|| metrics(80, 15, 8)),
        route_budget_exhausted: None,
        diagnostics: vec![],
    }
}

fn label_row(root: &str, status: &str, censored: bool) -> Row {
    let mut input = Input::default();
    input.global_features = vec![0.0; GLOBAL_FEATURES];
    for (slot, value) in input.global_features[3..6]
        .iter_mut()
        .zip([100.0f32, 20.0, 10.0])
    {
        *slot = value.ln_1p();
    }
    Row {
        root_id: root.into(),
        step: 0,
        pre_action: input,
        selected: SelectedAction {
            root: 0,
            family: 0,
            requested_port_mode: None,
            actual_port_mode: None,
            ports: vec![],
            assignments: vec![],
        },
        outcome: outcome(status, censored),
    }
}

#[test]
fn split_isolation_and_label_masks_match_observed_native_outcomes() {
    let split = partition_roots([
        ("train-root", Split::Training),
        ("development-root", Split::Development),
    ])
    .unwrap();
    assert_eq!(split[&Split::Training], ["train-root"]);
    assert_eq!(split[&Split::Development], ["development-root"]);
    assert!(
        partition_roots([
            ("same-root", Split::Training),
            ("same-root", Split::Confirmation),
        ])
        .is_err()
    );

    let rows = [
        label_row("train-root", "improved", false),
        label_row("train-root", "unchanged", false),
        label_row("train-root", "rejected", false),
        label_row("train-root", "improved", true),
    ];
    let mut labels = Labels {
        targets: vec![],
        masks: vec![],
    };
    for row in &rows {
        let (target, mask) = labels_for(row, 64.0).unwrap();
        labels.targets.push(target);
        labels.masks.push(mask);
    }
    assert_eq!(labels.targets[0][0], 1.0);
    assert_eq!(labels.targets[0][2..4], [1.0, 1.0]);
    assert!((labels.targets[0][8] - 0.8).abs() < 1e-6);
    assert_eq!(labels.targets[1][4], 1.0);
    assert_eq!(labels.targets[2][5], 1.0);
    assert_eq!(labels.masks[3], [0.0; 12]);
    let logits = Tensor::zeros([4, 12], (Kind::Float, Device::Cpu));
    let (_, denominators) = masked_loss(&logits, &labels, &[1.0; 12], Device::Cpu).unwrap();
    assert_eq!(denominators, [1, 3, 1, 1, 3, 3, 1, 1, 1, 1, 1, 0]);
}

fn model_row(swapped: bool) -> Row {
    let mut row = label_row("graph-root", "improved", false);
    row.pre_action.pending = 1;
    row.pre_action.conflicts = 1;
    row.pre_action.node_static = vec![[0.1; NODE_STATIC], [0.7; NODE_STATIC]];
    row.pre_action.node_state = vec![[0.2; NODE_STATE], [0.8; NODE_STATE]];
    row.pre_action.edge_index = vec![[0, 1]];
    row.pre_action.edge_static = vec![[0.3; EDGE_STATIC]];
    row.pre_action.edge_state = vec![[0.4; EDGE_STATE]];
    row.pre_action.path_cells = vec![[0.5; PATH_FEATURES]];
    row.pre_action.path_edges = vec![0];
    row.pre_action.action_features = vec![[0.6; ACTION_FEATURES]];
    row.pre_action.action_references = vec![[0, 1]];
    row.pre_action.root_meta = vec![[0, 0, 0]];
    row.pre_action.kind_log_counts = vec![0.0];
    row.pre_action.root_invalid = vec![false];
    let ports = [[0.15; PORT_FEATURES], [0.85; PORT_FEATURES]];
    row.selected.assignments = (0..2)
        .map(|terminal| Assignment {
            terminal,
            port: ports[if swapped {
                1 - terminal as usize
            } else {
                terminal as usize
            }],
        })
        .collect();
    row
}

#[test]
fn terminal_bindings_reach_shared_encoder_and_transfer_allowlist_is_exact() {
    let _lock = crate::TORCH_TEST.lock().unwrap();
    aic_torch::deterministic().unwrap();
    let config: Config = serde_json::from_str(include_str!(
        "../../../../docs/benchmarks/physical-outcome-pretraining/config.json"
    ))
    .unwrap();
    let original = model_row(false);
    let swapped = model_row(true);
    let predictor = Predictor::new(&config, 930_000, Device::Cpu).unwrap();
    let original_batch = Batch::from_rows(&[&original], 64.0, Device::Cpu).unwrap();
    let swapped_batch = Batch::from_rows(&[&swapped], 64.0, Device::Cpu).unwrap();
    assert!(
        !predictor
            .action_embedding(&original_batch)
            .equal(&predictor.action_embedding(&swapped_batch))
    );

    predictor
        .logits(&original_batch)
        .sum(Kind::Float)
        .backward();
    assert!(
        predictor
            .encoder_gradient_families()
            .into_iter()
            .all(|(family, reached)| reached || panic!("missing gradient for {family}"))
    );
    let encoder = predictor.encoder_snapshot();
    assert!(!encoder.is_empty());
    assert!(encoder.keys().all(|name| {
        name.strip_prefix("model.").is_some_and(is_encoder)
            && !name.contains("actor")
            && !name.contains("critic")
            && !name.contains("heads")
            && !name.starts_with("adam.")
    }));
    assert!(
        predictor
            .parameter_snapshot()
            .keys()
            .any(|name| name.starts_with("model.heads."))
    );
}
