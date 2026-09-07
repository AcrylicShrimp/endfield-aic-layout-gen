//! Charged edit outcomes, completion promotion and reward accounting.

use std::time::Instant;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::acceptance;
use crate::command::{Command, OPERATIONS};
use crate::diagnostic::{Diagnostic, Result};
use crate::edits;
use crate::engine::Engine;

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
pub struct Outcome {
    pub applied: bool,
    pub complete: bool,
    pub new_completion: bool,
    pub improved: bool,
    pub diagnostics: Vec<Diagnostic>,
}

impl Engine {
    pub fn attempt(
        &mut self,
        command: Result<Command>,
        trace_command: Value,
    ) -> Result<(f64, bool, Outcome)> {
        if self.steps >= self.config.horizon {
            return Err(Diagnostic::new(
                "engine.ended",
                "engine",
                "Episode has ended; reset before another command",
                None,
            ));
        }
        let started = Instant::now();
        let before_area = self.best_score[0];
        self.steps += 1;
        *self.episode_counts.entry("commands".into()).or_default() += 1;
        let operation = trace_command
            .get("op")
            .and_then(Value::as_str)
            .unwrap_or("unknown");
        *self.episode_counts.entry(operation.into()).or_default() += 1;
        let mut outcome = Outcome::default();
        let apply = command.and_then(|command| {
            if trace_command
                .get("revision")
                .is_some_and(|r| r.as_u64() != Some(self.revision))
            {
                return Err(Diagnostic::new(
                    "edit.rejected",
                    "edit",
                    "Stale observation revision",
                    command.entity(&self.catalog),
                ));
            }
            edits::apply(
                &self.catalog,
                &self.config,
                &self.draft,
                &self.base,
                &command,
            )
        });
        match apply {
            Ok(draft) => {
                outcome.applied = true;
                let validation_started = Instant::now();
                let accepted = acceptance::check(&self.catalog, &draft);
                *self.seconds.entry("validation".into()).or_default() +=
                    validation_started.elapsed().as_secs_f64();
                match accepted {
                    Ok((score, _)) => {
                        outcome.complete = true;
                        outcome.new_completion = draft != self.base;
                        self.base = draft.clone();
                        self.base_score = score;
                        if score < self.best_score {
                            self.best = draft.clone();
                            self.best_score = score;
                            outcome.improved = true;
                        }
                    }
                    Err(error) => {
                        *self.episode_counts.entry(error.code.clone()).or_default() += 1;
                        outcome.diagnostics.push(error);
                    }
                }
                self.draft = draft;
            }
            Err(error) => {
                let entity = trace_command
                    .get("terminal")
                    .or_else(|| trace_command.get("network"))
                    .and_then(Value::as_str);
                let mut diagnostic =
                    Diagnostic::new("edit.rejected", "edit", error.message, entity);
                diagnostic.severity = "warning".into();
                outcome.diagnostics.push(diagnostic);
            }
        }
        self.revision += 1;
        self.domain = None;
        for (key, flag) in [
            ("applied", outcome.applied),
            ("complete", outcome.complete),
            ("new_completion", outcome.new_completion),
            ("improved", outcome.improved),
        ] {
            *self.episode_counts.entry(key.into()).or_default() += u64::from(flag);
        }
        let reward = (before_area - self.best_score[0]) as f64 / self.initial_score[0] as f64;
        *self
            .episode_counts
            .entry("positive_rewards".into())
            .or_default() += u64::from(reward > 0.0);
        self.trace.push(json!({"command":trace_command,"reward":reward,"applied":outcome.applied,"complete":outcome.complete,"new_completion":outcome.new_completion,"improved":outcome.improved,"diagnostics":outcome.diagnostics}));
        *self.seconds.entry("step".into()).or_default() += started.elapsed().as_secs_f64();
        Ok((reward, self.steps == self.config.horizon, outcome))
    }

    pub fn diagnostic_step(&mut self, value: Value) -> Result<(f64, bool, Outcome)> {
        let command = Command::import(&self.catalog, &value);
        self.attempt(command, value)
    }

    pub fn operation_names(&self) -> &'static [&'static str] {
        &OPERATIONS
    }
}
