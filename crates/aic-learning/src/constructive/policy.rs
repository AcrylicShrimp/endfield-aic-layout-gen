//! Batched conditional sampling with one joint likelihood per native command.

use aic_pilot_native::constructive::learning::{Advance, Cursor, Observation, SelectedCommand};
use aic_torch::{
    Generator,
    tch::{Device, Tensor, no_grad},
};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};

use super::{Batch, Decision, Model, Prediction};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Factor {
    pub cursor: Cursor,
    pub selected: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Choice {
    pub factors: Vec<Factor>,
    pub command: SelectedCommand,
    pub old_logp: f64,
    pub old_value: f64,
    pub entropy: f64,
}

pub fn sample(
    model: &Model,
    rows: &[&Observation],
    sampler: &mut Generator,
    device: Device,
) -> Result<Vec<Choice>> {
    if device == Device::Cpu {
        sample_with(model, rows, device, |batch, prediction| {
            cpu_sample(batch, prediction, sampler)
        })
    } else {
        sample_with(model, rows, device, |batch, prediction| {
            device_sample(batch, prediction, sampler, device)
        })
    }
}

pub fn greedy(model: &Model, rows: &[&Observation], device: Device) -> Result<Vec<Choice>> {
    sample_with(model, rows, device, |batch, prediction| {
        greedy_selections(batch, prediction)
    })
}

fn cpu_sample(batch: &Batch, prediction: &Prediction, sampler: &mut Generator) -> Result<Vec<i64>> {
    let logp = prediction.logp.to_device(Device::Cpu);
    (0..batch.graphs() as usize)
        .map(|graph| {
            let range = batch.choice_range(graph);
            let probabilities = logp.narrow(0, range.start, range.end - range.start).exp();
            Ok(range.start + i64::try_from(sampler.categorical(&probabilities)?)?)
        })
        .collect()
}

fn greedy_selections(batch: &Batch, prediction: &Prediction) -> Result<Vec<i64>> {
    let logp = prediction.logp.to_device(Device::Cpu);
    (0..batch.graphs() as usize)
        .map(|graph| {
            let range = batch.choice_range(graph);
            Ok(range.start
                + i64::try_from(
                    logp.narrow(0, range.start, range.end - range.start)
                        .argmax(None, false),
                )?)
        })
        .collect()
}

fn device_sample(
    batch: &Batch,
    prediction: &Prediction,
    sampler: &mut Generator,
    device: Device,
) -> Result<Vec<i64>> {
    let offsets = batch.choice_offsets();
    // A single packed CDF avoids one host round trip per graph. Accumulate in
    // f64 so segment totals remain distinct even when thousands of normalized
    // distributions share the packed vector.
    let probabilities = prediction.logp.exp().to_kind(aic_torch::tch::Kind::Double);
    let cumulative = probabilities.cumsum(0, probabilities.kind());
    let ends = Tensor::from_slice(
        &offsets[1..]
            .iter()
            .map(|offset| offset - 1)
            .collect::<Vec<_>>(),
    )
    .to_device(device);
    let totals = cumulative.index_select(0, &ends);
    let bases = if offsets.len() == 2 {
        Tensor::zeros([1], (probabilities.kind(), device))
    } else {
        Tensor::cat(
            &[
                Tensor::zeros([1], (probabilities.kind(), device)),
                cumulative.index_select(
                    0,
                    &Tensor::from_slice(
                        &offsets[1..offsets.len() - 1]
                            .iter()
                            .map(|offset| offset - 1)
                            .collect::<Vec<_>>(),
                    )
                    .to_device(device),
                ),
            ],
            0,
        )
    };
    let draws = sampler
        .uniforms(batch.graphs())?
        .to_device(device)
        .to_kind(probabilities.kind());
    let thresholds = &bases + draws * (totals - &bases);
    let selected = thresholds.searchsorted(&cumulative, false, true, "right", None::<&Tensor>);
    Ok(Vec::<i64>::try_from(selected.to_device(Device::Cpu))?)
}

fn sample_with(
    model: &Model,
    rows: &[&Observation],
    device: Device,
    mut select: impl FnMut(&Batch, &Prediction) -> Result<Vec<i64>>,
) -> Result<Vec<Choice>> {
    ensure!(
        !rows.is_empty(),
        "aic-learning.constructive-sample: empty observation batch"
    );
    no_grad(|| {
        let mut cursors = vec![Cursor::Command; rows.len()];
        let mut factors = vec![Vec::new(); rows.len()];
        let mut commands = vec![None; rows.len()];
        let mut old_logp = vec![0.; rows.len()];
        let mut old_value = vec![0.; rows.len()];
        let mut entropy = vec![0.; rows.len()];
        for depth in 0..8 {
            let owners: Vec<_> = commands
                .iter()
                .enumerate()
                .filter_map(|(owner, command)| command.is_none().then_some(owner))
                .collect();
            if owners.is_empty() {
                break;
            }
            let banks = owners
                .iter()
                .map(|&owner| rows[owner].domain.choices(&cursors[owner]))
                .collect::<Vec<_>>();
            ensure!(
                banks.iter().all(|bank| !bank.is_empty()),
                "aic-learning.constructive-sample: empty conditional factor"
            );
            let decisions = owners
                .iter()
                .zip(&banks)
                .map(|(&owner, choices)| Decision {
                    input: &rows[owner].input,
                    choices,
                })
                .collect::<Vec<_>>();
            let batch = Batch::from_decisions(&decisions, device)?;
            let prediction = model.predict(&batch)?;
            let selected = select(&batch, &prediction)?;
            ensure!(
                selected.len() == owners.len(),
                "aic-learning.constructive-sample: selector returned wrong graph count"
            );
            let selected_logp = Vec::<f32>::try_from(
                prediction
                    .logp
                    .index_select(0, &Tensor::from_slice(&selected).to_device(device))
                    .to_device(Device::Cpu),
            )?;
            let values = Vec::<f32>::try_from(prediction.values.to_device(Device::Cpu))?;
            let entropies = Vec::<f32>::try_from(prediction.entropy.to_device(Device::Cpu))?;
            for (local, &owner) in owners.iter().enumerate() {
                let range = batch.choice_range(local);
                let absolute = selected[local];
                ensure!(
                    range.contains(&absolute),
                    "aic-learning.constructive-sample: selector returned invalid choice"
                );
                let selected = usize::try_from(absolute - range.start)?;
                old_logp[owner] += selected_logp[local] as f64;
                entropy[owner] += entropies[local] as f64;
                if depth == 0 {
                    old_value[owner] = values[local] as f64;
                }
                let cursor = cursors[owner].clone();
                factors[owner].push(Factor {
                    cursor: cursor.clone(),
                    selected,
                });
                match rows[owner].domain.advance(&cursor, selected)? {
                    Advance::Continue(next) => cursors[owner] = next,
                    Advance::Command(command) => commands[owner] = Some(command),
                }
            }
        }
        ensure!(
            commands.iter().all(Option::is_some),
            "aic-learning.constructive-sample: conditional decoder exceeded depth guard"
        );
        Ok((0..rows.len())
            .map(|owner| Choice {
                factors: std::mem::take(&mut factors[owner]),
                command: commands[owner].take().unwrap(),
                old_logp: old_logp[owner],
                old_value: old_value[owner],
                entropy: entropy[owner],
            })
            .collect())
    })
}

pub(crate) fn selected_likelihood(
    model: &Model,
    rows: &[TrainingDecision<'_>],
    device: Device,
) -> Result<(Tensor, Tensor, Tensor)> {
    ensure!(
        !rows.is_empty(),
        "aic-learning.constructive-likelihood: empty training batch"
    );
    let mut banks = Vec::new();
    let mut owners = Vec::new();
    let mut selected = Vec::new();
    let mut roots = Vec::with_capacity(rows.len());
    for (owner, row) in rows.iter().enumerate() {
        ensure!(
            !row.choice.factors.is_empty(),
            "aic-learning.constructive-likelihood: command has no factors"
        );
        roots.push(banks.len() as i64);
        let mut final_command = None;
        let mut expected = Cursor::Command;
        for (depth, factor) in row.choice.factors.iter().enumerate() {
            ensure!(
                factor.cursor == expected && final_command.is_none(),
                "aic-learning.constructive-likelihood: discontinuous factor prefix"
            );
            let choices = row.observation.domain.choices(&factor.cursor);
            ensure!(
                factor.selected < choices.len(),
                "aic-learning.constructive-likelihood: selected factor is stale"
            );
            match row
                .observation
                .domain
                .advance(&factor.cursor, factor.selected)?
            {
                Advance::Continue(next) => expected = next,
                Advance::Command(command) => {
                    ensure!(
                        depth + 1 == row.choice.factors.len(),
                        "aic-learning.constructive-likelihood: factors continue after command"
                    );
                    final_command = Some(command);
                }
            }
            banks.push(choices);
            owners.push(owner as i64);
            selected.push(factor.selected as i64);
        }
        ensure!(
            final_command.as_ref() == Some(&row.choice.command),
            "aic-learning.constructive-likelihood: factors do not reproduce command"
        );
    }
    let decisions = rows
        .iter()
        .flat_map(|row| {
            row.choice.factors.iter().enumerate().map(|(factor, _)| {
                // Prefix sums are computed below; this iterator only supplies inputs.
                (row.observation.input.as_ref(), factor)
            })
        })
        .enumerate()
        .map(|(index, (input, _))| Decision {
            input,
            choices: banks[index].as_slice(),
        })
        .collect::<Vec<_>>();
    let batch = Batch::from_decisions(&decisions, device)?;
    let prediction = model.predict(&batch)?;
    let mut absolute = Vec::with_capacity(selected.len());
    for (factor, selected) in selected.into_iter().enumerate() {
        absolute.push(batch.choice_range(factor).start + selected);
    }
    let selected = Tensor::from_slice(&absolute).to_device(device);
    let owners = Tensor::from_slice(&owners).to_device(device);
    let joint_logp = Tensor::zeros([rows.len() as i64], (prediction.logp.kind(), device))
        .index_add_(0, &owners, &prediction.logp.index_select(0, &selected));
    let joint_entropy = Tensor::zeros([rows.len() as i64], (prediction.entropy.kind(), device))
        .index_add_(0, &owners, &prediction.entropy);
    let roots = Tensor::from_slice(&roots).to_device(device);
    let values = prediction.values.index_select(0, &roots);
    Ok((joint_logp, joint_entropy, values))
}

pub(crate) struct TrainingDecision<'a> {
    pub observation: &'a Observation,
    pub choice: &'a Choice,
}
