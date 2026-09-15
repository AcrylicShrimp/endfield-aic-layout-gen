use super::{
    config::Config,
    model::{
        Batch, HEADS, Labels, Predictor, geometric_features, labels_for, masked_loss, sigmoid,
    },
};
use crate::{
    input::{Tensors, sha256},
    outcomes::{
        config::Split,
        storage::{Row, VerifiedIndex, hash, verified_index},
    },
    training::{
        config::Resources as TrainingResources,
        resources::{Guard, Ledger},
    },
};
use aic_torch::{
    Adam, AdamBackend, Generator, deterministic,
    tch::{Device, Kind, Tensor, no_grad},
};
use anyhow::{Context, Result, ensure};
use serde::Serialize;
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs,
    io::{BufWriter, Write},
    path::{Path, PathBuf},
    time::Instant,
};

const RUN_SCHEMA: &str = "aic-physical-outcome-pretraining-run-v1";

#[derive(Serialize)]
struct CheckpointManifest {
    schema: &'static str,
    config_sha256: String,
    dataset_manifest_sha256: String,
    seed: u64,
    epoch: usize,
    development_loss: f64,
    tensors_sha256: String,
}

struct Geometry {
    parameters: Vec<(String, Tensor)>,
    optimizer: Adam,
    weights: [f64; 12],
    clip: f64,
    device: Device,
}

impl Geometry {
    fn new(config: &Config, seed: u64, device: Device) -> Result<Self> {
        aic_torch::tch::manual_seed(i64::try_from(seed + 10_000)?);
        let input = 42;
        let bound = 1.0 / (input as f64).sqrt();
        let parameters: Vec<(String, Tensor)> = vec![
            (
                "geometry.weight".into(),
                Tensor::empty([12, input], (Kind::Float, device))
                    .uniform_(-bound, bound)
                    .set_requires_grad(true),
            ),
            (
                "geometry.bias".into(),
                Tensor::zeros([12], (Kind::Float, device)).set_requires_grad(true),
            ),
        ];
        let optimizer = Adam::new(
            parameters
                .iter()
                .map(|(name, value)| (name.clone(), value.shallow_clone()))
                .collect(),
            config.optimizer.learning_rate,
            config.optimizer.beta1,
            config.optimizer.beta2,
            config.optimizer.epsilon,
            AdamBackend::Foreach,
        )?;
        Ok(Self {
            parameters,
            optimizer,
            weights: config.losses.values(),
            clip: config.optimizer.gradient_clip,
            device,
        })
    }

    fn logits(&self, rows: &[&Row]) -> Tensor {
        let features = rows
            .iter()
            .flat_map(|row| geometric_features(row))
            .collect::<Vec<_>>();
        Tensor::from_slice(&features)
            .view([-1, 42])
            .to_device(self.device)
            .linear(&self.parameters[0].1, Some(&self.parameters[1].1))
    }

    fn train(&mut self, rows: &[&Row], count_scale: f64) -> Result<f64> {
        let labels = row_labels(rows, count_scale)?;
        self.optimizer.zero_grad()?;
        let (loss, _) = masked_loss(&self.logits(rows), &labels, &self.weights, self.device)?;
        let scalar = f64::try_from(&loss)?;
        loss.backward();
        self.optimizer.clip_grad_norm(self.clip)?;
        self.optimizer.step()?;
        Ok(scalar)
    }

    fn loss(&self, rows: &[&Row], count_scale: f64) -> Result<f64> {
        let labels = row_labels(rows, count_scale)?;
        let (loss, _) =
            no_grad(|| masked_loss(&self.logits(rows), &labels, &self.weights, self.device))?;
        f64::try_from(loss).map_err(Into::into)
    }

    fn predictions(&self, rows: &[&Row]) -> Result<Vec<[f32; 12]>> {
        let values: Vec<f32> = Vec::try_from(self.logits(rows).to_device(Device::Cpu).view([-1]))?;
        Ok(values
            .chunks_exact(12)
            .map(|row| row.try_into().unwrap())
            .collect())
    }

    fn snapshot(&self) -> Vec<Tensor> {
        self.parameters
            .iter()
            .map(|(_, value)| value.detach().to_device(Device::Cpu).copy())
            .collect()
    }

    fn restore(&mut self, values: &[Tensor]) {
        no_grad(|| {
            for ((_, target), source) in self.parameters.iter_mut().zip(values) {
                target.copy_(&source.to_device(self.device));
            }
        });
    }
}

struct SeedRun {
    seed: u64,
    predictor: Predictor,
    geometry: Geometry,
    best_model_epoch: usize,
    best_geometry_epoch: usize,
    development: Vec<Value>,
}

pub fn run(
    config_path: &Path,
    dataset_path: &Path,
    output: &Path,
    device: Device,
) -> Result<Value> {
    let started = Instant::now();
    ensure!(
        !output.exists(),
        "aic-learning.pretraining-output: output already exists: {}",
        output.display()
    );
    let (config, config_bytes) = Config::load(config_path)?;
    deterministic()?;
    let name = output
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| anyhow::anyhow!("aic-learning.pretraining-output: invalid output path"))?;
    let stage = output.with_file_name(format!(".{name}.stage-{}", std::process::id()));
    ensure!(
        !stage.exists(),
        "aic-learning.pretraining-output: staging path exists: {}",
        stage.display()
    );
    fs::create_dir_all(stage.join("checkpoints"))?;
    let resource_config = TrainingResources {
        max_observation_bytes: config.resources.max_rss_bytes,
        max_rss_bytes: config.resources.max_rss_bytes,
        max_output_bytes: config.resources.max_output_bytes,
        min_free_bytes: 0,
    };
    let guard = Guard::new(
        &stage,
        &resource_config,
        config.resources.max_elapsed_seconds,
    )?;
    let check = guard.check();
    let mut ledger = Ledger::default();
    write_bytes(&stage.join("config.json"), &config_bytes)?;
    ledger.track(&stage.join("config.json"))?;
    let config_sha256 = hash(&config_bytes);
    ensure!(
        sha256(&dataset_path.join("manifest.json"))? == config.dataset.manifest_sha256,
        "aic-learning.pretraining-dataset: manifest hash mismatch"
    );
    let index = verified_index(dataset_path)?;
    ensure!(
        index.manifest.status == "complete"
            && index.manifest.config_sha256 == config.dataset.config_sha256
            && index.manifest.catalog_sha256 == config.dataset.catalog_sha256,
        "aic-learning.pretraining-dataset: dataset binding mismatch"
    );
    let roots = split_roots(&index)?;
    let constant = fit_constant_and_check(&index, &roots[&Split::Training], config.count_scale)?;
    check.run()?;

    let mut runs = Vec::new();
    let mut latest_paths = BTreeMap::new();
    let mut best_paths = BTreeMap::new();
    for &seed in &config.seeds {
        let mut predictor = Predictor::new(&config, seed, device)?;
        let mut geometry = Geometry::new(&config, seed, device)?;
        let mut shuffle = Generator::new(seed + 20_000)?;
        let mut best_model = None;
        let mut best_geometry = None;
        let mut best_model_loss = f64::INFINITY;
        let mut best_geometry_loss = f64::INFINITY;
        let mut best_model_epoch = 0;
        let mut best_geometry_epoch = 0;
        let mut development = Vec::new();
        for epoch in 1..=config.epochs {
            check.run()?;
            let order: Vec<i64> =
                Vec::try_from(shuffle.permutation(roots[&Split::Training].len() as i64)?)?;
            let ordered = order
                .into_iter()
                .map(|i| roots[&Split::Training][i as usize].as_str())
                .collect::<Vec<_>>();
            let mut train_loss = 0.0;
            let mut geometry_train_loss = 0.0;
            let mut batches = 0usize;
            let mut min_gradient = f64::INFINITY;
            for ids in ordered.chunks(config.roots_per_batch) {
                check.run()?;
                let owned = read_roots(&index, ids)?;
                let refs = owned.iter().collect::<Vec<_>>();
                let batch = Batch::from_rows(&refs, config.count_scale, device)?;
                let step = predictor.train(&batch)?;
                train_loss += step.loss;
                min_gradient = min_gradient.min(step.gradient_norm);
                geometry_train_loss += geometry.train(&refs, config.count_scale)?;
                batches += 1;
            }
            let model_dev = predictor_loss(
                &index,
                &roots[&Split::Development],
                config.roots_per_batch,
                config.count_scale,
                device,
                &predictor,
            )?;
            let geometry_dev = geometry_loss(
                &index,
                &roots[&Split::Development],
                config.roots_per_batch,
                config.count_scale,
                &geometry,
            )?;
            let improved_model = model_dev < best_model_loss;
            let improved_geometry = geometry_dev < best_geometry_loss;
            if improved_model {
                best_model_loss = model_dev;
                best_model_epoch = epoch;
                best_model = Some(predictor.parameter_snapshot());
            }
            if improved_geometry {
                best_geometry_loss = geometry_dev;
                best_geometry_epoch = epoch;
                best_geometry = Some(geometry.snapshot());
            }
            let values = predictor.checkpoint_snapshot(&mut shuffle)?;
            let checkpoint = save_checkpoint(
                &stage,
                &config_sha256,
                &config.dataset.manifest_sha256,
                seed,
                epoch,
                model_dev,
                &values,
                &mut ledger,
                config.resources.max_output_bytes,
            )?;
            update_pointer(
                &stage,
                &format!("latest-{seed}.json"),
                &checkpoint,
                &mut ledger,
            )?;
            let previous_latest = latest_paths.insert(seed, checkpoint.clone());
            if improved_model {
                update_pointer(
                    &stage,
                    &format!("best-{seed}.json"),
                    &checkpoint,
                    &mut ledger,
                )?;
                let previous_best = best_paths.insert(seed, checkpoint.clone());
                prune(previous_best, &latest_paths, &best_paths, &mut ledger)?;
            }
            prune(previous_latest, &latest_paths, &best_paths, &mut ledger)?;
            ledger.enforce(config.resources.max_output_bytes)?;
            development.push(json!({"epoch":epoch,"train_loss":train_loss/batches as f64,
                "geometric_train_loss":geometry_train_loss/batches as f64,
                "development_loss":model_dev,"geometric_development_loss":geometry_dev,
                "minimum_gradient_norm":min_gradient}));
        }
        let best_model = best_model.expect("epochs are positive");
        predictor.restore_parameters(&best_model)?;
        let best_geometry = best_geometry.expect("epochs are positive");
        geometry.restore(&best_geometry);
        runs.push(SeedRun {
            seed,
            predictor,
            geometry,
            best_model_epoch,
            best_geometry_epoch,
            development,
        });
    }

    // Confirmation rows remain opaque until every model and baseline is locked.
    write_json(&stage.join("constant.json"), &constant)?;
    ledger.track(&stage.join("constant.json"))?;
    let constant_sha256 = sha256(&stage.join("constant.json"))?;
    let mut locked = Vec::new();
    for run in &runs {
        let geometry_path = stage.join(format!("geometry-{}.safetensors", run.seed));
        Tensor::write_safetensors(&run.geometry.parameters, &geometry_path)?;
        fs::File::open(&geometry_path)?.sync_all()?;
        ledger.track(&geometry_path)?;
        let model_path = &best_paths[&run.seed];
        locked.push(json!({"seed":run.seed,"model_epoch":run.best_model_epoch,
            "model_checkpoint":model_path.strip_prefix(&stage)?,
            "model_sha256":sha256(&model_path.join("state.safetensors"))?,
            "geometry_epoch":run.best_geometry_epoch,
            "geometry_path":geometry_path.strip_prefix(&stage)?,
            "geometry_sha256":sha256(&geometry_path)?}));
    }
    let locked = json!({"constant_sha256":constant_sha256,"seeds":locked});
    write_json(&stage.join("locked.json"), &locked)?;
    ledger.track(&stage.join("locked.json"))?;
    ledger.enforce(config.resources.max_output_bytes)?;

    let mut seed_reports = Vec::new();
    let mut model_metrics = Vec::new();
    let mut geometry_metrics = Vec::new();
    for run in &mut runs {
        check.run()?;
        let model_rows = predict_split(
            &index,
            &roots[&Split::Confirmation],
            config.roots_per_batch,
            config.count_scale,
            device,
            |rows| {
                let batch = Batch::from_rows(rows, config.count_scale, device)?;
                run.predictor.predictions(&batch)
            },
        )?;
        let geometry_rows = predict_split(
            &index,
            &roots[&Split::Confirmation],
            config.roots_per_batch,
            config.count_scale,
            device,
            |rows| run.geometry.predictions(rows),
        )?;
        let model = summarize(&model_rows);
        let geometric = summarize(&geometry_rows);
        model_metrics.push(primary(&model)?);
        geometry_metrics.push(primary(&geometric)?);
        let encoder = run.predictor.encoder_snapshot();
        ensure!(
            encoder
                .keys()
                .all(|name| super::model::is_encoder(name.strip_prefix("model.").unwrap_or(name))),
            "aic-learning.pretraining-transfer: non-encoder tensor selected"
        );
        let encoder_path = stage.join(format!("encoder-{}.safetensors", run.seed));
        Tensor::write_safetensors(&encoder.iter().collect::<Vec<_>>(), &encoder_path)?;
        ledger.track(&encoder_path)?;
        seed_reports.push(
            json!({"seed":run.seed,"best_model_epoch":run.best_model_epoch,
            "best_geometry_epoch":run.best_geometry_epoch,"development":run.development,
            "confirmation":{"multi_head":model,"geometric":geometric},
            "encoder":{"path":encoder_path.file_name().unwrap(),"sha256":sha256(&encoder_path)?}}),
        );
    }
    let constant_rows = predict_split(
        &index,
        &roots[&Split::Confirmation],
        config.roots_per_batch,
        config.count_scale,
        Device::Cpu,
        |rows| Ok(vec![constant; rows.len()]),
    )?;
    let constant_report = summarize(&constant_rows);
    let constant_metric = primary(&constant_report)?;
    let gate = gate(&model_metrics, &geometry_metrics, &constant_metric, &config);
    let report = json!({"schema":RUN_SCHEMA,"status":"complete","config_sha256":config_sha256,
        "dataset_manifest_sha256":config.dataset.manifest_sha256,"device":format!("{device:?}"),
        "seeds":seed_reports,"constant_confirmation":constant_report,"transfer_gate":gate,
        "elapsed_seconds":started.elapsed().as_secs_f64(),"peak_rss_bytes":guard.peak_rss()});
    write_json(&stage.join("report.json"), &report)?;
    ledger.track(&stage.join("report.json"))?;
    ledger.enforce(config.resources.max_output_bytes)?;
    let final_manifest = json!({"schema":RUN_SCHEMA,"status":"complete","report":"report.json",
        "report_sha256":sha256(&stage.join("report.json"))?,"output_bytes":ledger.bytes});
    write_json(&stage.join("manifest.json"), &final_manifest)?;
    ledger.track(&stage.join("manifest.json"))?;
    ledger.enforce(config.resources.max_output_bytes)?;
    fs::File::open(&stage)?.sync_all()?;
    fs::rename(&stage, output)?;
    fs::File::open(output.parent().unwrap_or_else(|| Path::new(".")))?.sync_all()?;
    Ok(
        json!({"status":"complete","output":output,"transfer_gate":gate,
        "elapsed_seconds":started.elapsed().as_secs_f64(),"peak_rss_bytes":guard.peak_rss(),
        "output_bytes":ledger.bytes}),
    )
}

fn split_roots(index: &VerifiedIndex) -> Result<BTreeMap<Split, Vec<String>>> {
    for root in &index.manifest.roots {
        ensure!(
            root.status == "collected" && root.rows == 32,
            "aic-learning.pretraining-dataset: incomplete root {}",
            root.slot.root_id
        );
    }
    let roots = partition_roots(
        index
            .manifest
            .roots
            .iter()
            .map(|root| (root.slot.root_id.as_str(), root.slot.split)),
    )?;
    ensure!(
        roots.get(&Split::Training).is_some_and(|v| v.len() == 64)
            && roots
                .get(&Split::Development)
                .is_some_and(|v| v.len() == 16)
            && roots
                .get(&Split::Confirmation)
                .is_some_and(|v| v.len() == 16),
        "aic-learning.pretraining-dataset: wrong split cardinality"
    );
    Ok(roots)
}

pub(super) fn partition_roots<'a>(
    assignments: impl IntoIterator<Item = (&'a str, Split)>,
) -> Result<BTreeMap<Split, Vec<String>>> {
    let mut roots = BTreeMap::<Split, Vec<String>>::new();
    let mut owners = BTreeMap::new();
    for (id, split) in assignments {
        ensure!(
            owners.insert(id, split).is_none(),
            "aic-learning.pretraining-dataset: duplicate root split assignment for {id}"
        );
        roots.entry(split).or_default().push(id.to_owned());
    }
    Ok(roots)
}

fn read_roots(index: &VerifiedIndex, ids: &[&str]) -> Result<Vec<Row>> {
    let mut rows = Vec::with_capacity(ids.len() * 32);
    for id in ids {
        rows.extend(index.read_root(id)?);
    }
    Ok(rows)
}

fn row_labels(rows: &[&Row], count_scale: f64) -> Result<Labels> {
    let mut labels = Labels {
        targets: Vec::with_capacity(rows.len()),
        masks: Vec::with_capacity(rows.len()),
    };
    for row in rows {
        let (target, mask) = labels_for(row, count_scale)?;
        labels.targets.push(target);
        labels.masks.push(mask);
    }
    Ok(labels)
}

fn fit_constant_and_check(
    index: &VerifiedIndex,
    roots: &[String],
    count_scale: f64,
) -> Result<[f32; 12]> {
    let mut sums = [0f64; 12];
    let mut counts = [0u64; 12];
    for id in roots {
        let rows = index.read_root(id)?;
        let mut initial = None;
        for (step, row) in rows.iter().enumerate() {
            ensure!(
                row.step == step as u64 && row.root_id == *id,
                "aic-learning.pretraining-dataset: invalid row sequence for {id}"
            );
            let current: [u64; 3] = row
                .pre_action
                .global_features
                .get(3..6)
                .ok_or_else(|| {
                    anyhow::anyhow!(
                        "aic-learning.pretraining-dataset: missing initial score for {id}"
                    )
                })?
                .iter()
                .map(|v| v.exp_m1().round() as u64)
                .collect::<Vec<_>>()
                .try_into()
                .unwrap();
            ensure!(
                current.iter().all(|v| *v > 0) && initial.is_none_or(|prior| prior == current),
                "aic-learning.pretraining-dataset: inconsistent initial score for {id}"
            );
            initial = Some(current);
            let (target, mask) = labels_for(row, count_scale)?;
            for head in 0..12 {
                if mask[head] > 0.0 {
                    sums[head] += target[head] as f64;
                    counts[head] += 1;
                }
            }
        }
    }
    let mut result = [0f32; 12];
    for head in 0..12 {
        ensure!(
            counts[head] > 0 || matches!(head, 3 | 5 | 11),
            "aic-learning.pretraining-dataset: primary training head {} has no labels",
            HEADS[head]
        );
        if counts[head] == 0 {
            continue;
        }
        let mean = sums[head] / counts[head] as f64;
        result[head] = if matches!(head, 0..=5 | 11) {
            let p = mean.clamp(1e-4, 1.0 - 1e-4);
            (p / (1.0 - p)).ln() as f32
        } else {
            mean as f32
        };
    }
    Ok(result)
}

fn predictor_loss(
    index: &VerifiedIndex,
    roots: &[String],
    batch: usize,
    scale: f64,
    device: Device,
    model: &Predictor,
) -> Result<f64> {
    let mut total = 0.0;
    let mut n = 0;
    for ids in roots.chunks(batch) {
        let ids = ids.iter().map(String::as_str).collect::<Vec<_>>();
        let owned = read_roots(index, &ids)?;
        let rows = owned.iter().collect::<Vec<_>>();
        let packed = Batch::from_rows(&rows, scale, device)?;
        total += f64::try_from(no_grad(|| model.evaluate_loss(&packed).map(|v| v.0))?)?;
        n += 1;
    }
    Ok(total / n as f64)
}

fn geometry_loss(
    index: &VerifiedIndex,
    roots: &[String],
    batch: usize,
    scale: f64,
    model: &Geometry,
) -> Result<f64> {
    let mut total = 0.0;
    let mut n = 0;
    for ids in roots.chunks(batch) {
        let ids = ids.iter().map(String::as_str).collect::<Vec<_>>();
        let owned = read_roots(index, &ids)?;
        let rows = owned.iter().collect::<Vec<_>>();
        total += model.loss(&rows, scale)?;
        n += 1;
    }
    Ok(total / n as f64)
}

#[derive(Clone)]
struct Scored {
    root: String,
    blocks: usize,
    family: usize,
    state: &'static str,
    target: [f32; 12],
    mask: [f32; 12],
    prediction: [f32; 12],
    initial: [f64; 3],
}

fn predict_split<F>(
    index: &VerifiedIndex,
    roots: &[String],
    batch: usize,
    scale: f64,
    _device: Device,
    mut predict: F,
) -> Result<Vec<Scored>>
where
    F: FnMut(&[&Row]) -> Result<Vec<[f32; 12]>>,
{
    let block_map = index
        .manifest
        .roots
        .iter()
        .map(|root| (root.slot.root_id.as_str(), root.slot.blocks))
        .collect::<BTreeMap<_, _>>();
    let mut result = Vec::new();
    for ids in roots.chunks(batch) {
        let ids = ids.iter().map(String::as_str).collect::<Vec<_>>();
        let owned = read_roots(index, &ids)?;
        let rows = owned.iter().collect::<Vec<_>>();
        let predictions = predict(&rows)?;
        ensure!(
            predictions.len() == rows.len(),
            "aic-learning.pretraining-evaluation: prediction count mismatch"
        );
        for (row, prediction) in rows.into_iter().zip(predictions) {
            let (target, mask) = labels_for(row, scale)?;
            let initial: [f64; 3] = row.pre_action.global_features[3..6]
                .iter()
                .map(|v| f64::from(*v).exp_m1().round().max(1.0))
                .collect::<Vec<_>>()
                .try_into()
                .unwrap();
            result.push(Scored {
                root: row.root_id.clone(),
                blocks: block_map[&row.root_id.as_str()],
                family: row.selected.family,
                state: if row.pre_action.pending > 0 || row.pre_action.conflicts > 0 {
                    "pending"
                } else {
                    "complete"
                },
                target,
                mask,
                prediction,
                initial,
            });
        }
    }
    Ok(result)
}

fn summarize(rows: &[Scored]) -> Value {
    let mut groups = BTreeMap::<String, Vec<&Scored>>::new();
    groups.insert("all".into(), rows.iter().collect());
    for row in rows {
        groups
            .entry(format!("size={}", row.blocks))
            .or_default()
            .push(row);
        groups
            .entry(format!("family={}", row.family))
            .or_default()
            .push(row);
        groups
            .entry(format!("state={}", row.state))
            .or_default()
            .push(row);
        groups
            .entry(format!(
                "size={}/family={}/state={}",
                row.blocks, row.family, row.state
            ))
            .or_default()
            .push(row);
    }
    Value::Object(
        groups
            .into_iter()
            .map(|(name, rows)| (name, summarize_group(&rows)))
            .collect(),
    )
}

fn summarize_group(rows: &[&Scored]) -> Value {
    let completion = weighted_completion(rows);
    let mut metric = Vec::new();
    for (head, scale) in [(8, 0usize), (9, 1), (10, 2)] {
        let mut normalized = 0.0;
        let mut raw = 0.0;
        let mut weight = 0.0;
        let root_counts = valid_per_root(rows, head);
        for row in rows.iter().copied().filter(|row| row.mask[head] > 0.0) {
            let w = 1.0 / root_counts[&row.root] as f64;
            let error = (f64::from(row.prediction[head]) - f64::from(row.target[head])).abs();
            normalized += w * error;
            raw += w * error * row.initial[scale];
            weight += w;
        }
        metric.push(json!({"head":HEADS[head],"labels":root_counts.values().sum::<usize>(),
            "roots":root_counts.len(),"normalized_mae":option(normalized,weight),"raw_mae":option(raw,weight)}));
    }
    let labels = Value::Object(
        HEADS
            .iter()
            .enumerate()
            .map(|(head, name)| {
                (
                    (*name).to_owned(),
                    json!({"labels":rows.iter().filter(|row| row.mask[head] > 0.0).count(),
                        "roots":valid_per_root(rows, head).len()}),
                )
            })
            .collect(),
    );
    json!({"labels":labels,"completion":completion,"metrics":metric})
}

fn valid_per_root(rows: &[&Scored], head: usize) -> BTreeMap<String, usize> {
    let mut counts = BTreeMap::new();
    for row in rows.iter().copied().filter(|row| row.mask[head] > 0.0) {
        *counts.entry(row.root.clone()).or_default() += 1;
    }
    counts
}

fn weighted_completion(rows: &[&Scored]) -> Value {
    let counts = valid_per_root(rows, 0);
    let selected = rows
        .iter()
        .copied()
        .filter(|row| row.mask[0] > 0.0)
        .collect::<Vec<_>>();
    let positives = selected.iter().filter(|row| row.target[0] > 0.5).count();
    let mut log_loss = 0.0;
    let mut brier = 0.0;
    let mut weight = 0.0;
    let mut bins = vec![(0.0, 0.0, 0.0); 10];
    for row in &selected {
        let w = 1.0 / counts[&row.root] as f64;
        let p = sigmoid(row.prediction[0]).clamp(1e-7, 1.0 - 1e-7);
        let y = f64::from(row.target[0]);
        log_loss += w * (-(y * p.ln() + (1.0 - y) * (1.0 - p).ln()));
        brier += w * (p - y).powi(2);
        weight += w;
        let bin = ((p * 10.0).floor() as usize).min(9);
        bins[bin].0 += w * p;
        bins[bin].1 += w * y;
        bins[bin].2 += w;
    }
    let ece = if weight > 0.0 {
        bins.into_iter()
            .filter(|b| b.2 > 0.0)
            .map(|b| b.2 / weight * (b.0 / b.2 - b.1 / b.2).abs())
            .sum()
    } else {
        f64::NAN
    };
    let auroc = weighted_auroc(&selected, &counts);
    json!({"labels":selected.len(),"roots":counts.len(),"positives":positives,"negatives":selected.len()-positives,
        "log_loss":option(log_loss,weight),"brier":option(brier,weight),"ece10":if ece.is_finite(){Some(ece)}else{None},"auroc":auroc})
}

fn weighted_auroc(rows: &[&Scored], counts: &BTreeMap<String, usize>) -> Option<f64> {
    let positives = rows
        .iter()
        .filter(|r| r.target[0] > 0.5)
        .collect::<Vec<_>>();
    let negatives = rows
        .iter()
        .filter(|r| r.target[0] <= 0.5)
        .collect::<Vec<_>>();
    if positives.is_empty() || negatives.is_empty() {
        return None;
    }
    let mut score = 0.0;
    let mut total = 0.0;
    for p in positives {
        for n in &negatives {
            let w = 1.0 / counts[&p.root] as f64 / counts[&n.root] as f64;
            let pp = sigmoid(p.prediction[0]);
            let np = sigmoid(n.prediction[0]);
            score += w * if pp > np {
                1.0
            } else if pp == np {
                0.5
            } else {
                0.0
            };
            total += w;
        }
    }
    Some(score / total)
}

fn option(sum: f64, weight: f64) -> Option<f64> {
    (weight > 0.0).then_some(sum / weight)
}

#[derive(Clone, Copy)]
struct Primary {
    log_loss: f64,
    auroc: f64,
    metric: f64,
}
fn primary(report: &Value) -> Result<Primary> {
    let all = &report["all"];
    Ok(Primary {
        log_loss: all["completion"]["log_loss"]
            .as_f64()
            .context("completion log loss")?,
        auroc: all["completion"]["auroc"]
            .as_f64()
            .context("completion AUROC")?,
        metric: all["metrics"]
            .as_array()
            .unwrap()
            .iter()
            .map(|m| m["normalized_mae"].as_f64().unwrap())
            .sum::<f64>()
            / 3.0,
    })
}

fn gate(models: &[Primary], geometry: &[Primary], constant: &Primary, config: &Config) -> Value {
    let mean = |values: Vec<f64>| values.iter().sum::<f64>() / values.len() as f64;
    let model_log = mean(models.iter().map(|m| m.log_loss).collect());
    let geometry_log = mean(geometry.iter().map(|m| m.log_loss).collect());
    let auroc = mean(models.iter().map(|m| m.auroc).collect());
    let metric = mean(models.iter().map(|m| m.metric).collect());
    let clauses = json!({"completion_beats_constant":model_log<constant.log_loss,"completion_beats_geometric":model_log<geometry_log,
        "auroc_at_least_threshold":auroc>=config.gate.minimum_auroc,
        "metric_mae_improvement_at_least_threshold":metric<=constant.metric*(1.0-config.gate.minimum_metric_mae_improvement)});
    let passed = clauses.as_object().unwrap().values().all(|v| v == true);
    json!({"passed":passed,"clauses":clauses,"multi_head":{"completion_log_loss":model_log,"auroc":auroc,"normalized_metric_mae":metric},
        "geometric_completion_log_loss":geometry_log,"constant":{"completion_log_loss":constant.log_loss,"normalized_metric_mae":constant.metric}})
}

fn save_checkpoint(
    root: &Path,
    config_hash: &str,
    dataset_hash: &str,
    seed: u64,
    epoch: usize,
    dev: f64,
    values: &Tensors,
    ledger: &mut Ledger,
    limit: u64,
) -> Result<PathBuf> {
    let final_path = root.join("checkpoints").join(format!("{seed}-{epoch:02}"));
    let stage = final_path.with_extension("pending");
    fs::create_dir(&stage)?;
    let tensors = stage.join("state.safetensors");
    Tensor::write_safetensors(&values.iter().collect::<Vec<_>>(), &tensors)?;
    fs::File::open(&tensors)?.sync_all()?;
    let manifest = CheckpointManifest {
        schema: RUN_SCHEMA,
        config_sha256: config_hash.into(),
        dataset_manifest_sha256: dataset_hash.into(),
        seed,
        epoch,
        development_loss: dev,
        tensors_sha256: sha256(&tensors)?,
    };
    write_json(&stage.join("manifest.json"), &manifest)?;
    fs::File::open(stage.join("manifest.json"))?.sync_all()?;
    fs::File::open(&stage)?.sync_all()?;
    fs::rename(&stage, &final_path)?;
    fs::File::open(final_path.parent().unwrap())?.sync_all()?;
    ledger.track(&final_path.join("state.safetensors"))?;
    ledger.track(&final_path.join("manifest.json"))?;
    ledger.enforce(limit)?;
    Ok(final_path)
}

fn update_pointer(root: &Path, name: &str, path: &Path, ledger: &mut Ledger) -> Result<()> {
    let target = root.join(name);
    let pending = target.with_extension("pending");
    write_json(&pending, &json!({"checkpoint":path.strip_prefix(root)?}))?;
    fs::File::open(&pending)?.sync_all()?;
    fs::rename(&pending, &target)?;
    ledger.track(&target)?;
    Ok(())
}

fn prune(
    path: Option<PathBuf>,
    latest: &BTreeMap<u64, PathBuf>,
    best: &BTreeMap<u64, PathBuf>,
    ledger: &mut Ledger,
) -> Result<()> {
    let Some(path) = path else { return Ok(()) };
    if !path.exists() {
        return Ok(());
    }
    if latest
        .values()
        .chain(best.values())
        .any(|keep| keep == &path)
    {
        return Ok(());
    }
    for name in ["state.safetensors", "manifest.json"] {
        let file = path.join(name);
        if file.exists() {
            ledger.remove(&file)?;
        }
    }
    fs::remove_dir(path)?;
    Ok(())
}

fn write_json(path: &Path, value: &impl Serialize) -> Result<()> {
    let mut writer = BufWriter::new(fs::File::create(path)?);
    serde_json::to_writer_pretty(&mut writer, value)?;
    writer.write_all(b"\n")?;
    writer.flush()?;
    writer.get_ref().sync_all()?;
    Ok(())
}
fn write_bytes(path: &Path, value: &[u8]) -> Result<()> {
    let mut writer = BufWriter::new(fs::File::create(path)?);
    writer.write_all(value)?;
    writer.flush()?;
    writer.get_ref().sync_all()?;
    Ok(())
}
