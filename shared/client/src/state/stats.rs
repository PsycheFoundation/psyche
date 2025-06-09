use psyche_coordinator::{model, Coordinator, WitnessEvalResult, WitnessMetadata};
use psyche_core::{BoundedQueue, FixedVec, LearningRateSchedule, NodeIdentity};
use psyche_modeling::Trainer;
use std::{collections::HashMap, sync::Arc, time::Duration};
use tokenizers::Tokenizer;
use tracing::warn;
use wandb::{DataValue, LogData};

use crate::{
    client::P2PNodeInfo,
    state::evals::{EnumTask, EvalTask},
};

use super::evals::EvalRunner;

pub struct StatsLogger {
    tokenizer: Arc<Tokenizer>,
    wandb_run: Option<Arc<wandb::Run>>,
    eval_runner: EvalRunner,

    step_durations: BoundedQueue<Duration, 16>,
    training_round_durations: BoundedQueue<Duration, 16>,

    losses: Vec<f32>,
    last_optim_stats: HashMap<String, f64>,
    eval_history: HashMap<String, Vec<f64>>,
    lr_schedule: LearningRateSchedule,

    pub node_info: HashMap<String, P2PNodeInfo>,
}

impl StatsLogger {
    pub fn new(
        tokenizer: Arc<Tokenizer>,
        eval_runner: EvalRunner,
        lr_schedule: LearningRateSchedule,
        wandb_run: Option<wandb::Run>,
    ) -> Self {
        Self {
            tokenizer,
            wandb_run: wandb_run.map(Arc::new),
            losses: Vec::new(),
            step_durations: Default::default(),
            training_round_durations: Default::default(),
            eval_runner,
            lr_schedule,
            eval_history: HashMap::new(),
            last_optim_stats: HashMap::new(),
            node_info: HashMap::new(),
        }
    }

    pub fn publish_round_stats<T: NodeIdentity>(&self, state: &Coordinator<T>) {
        let mut round_log = LogData::new();

        round_log.insert("_step", state.progress.step);

        if let Some(loss) = self.losses().last() {
            round_log.insert("train/loss", *loss);
            round_log.insert("train/perplexity", perplexity(*loss));
            round_log.insert("train/confidence", self.confidence(*loss));
        }
        round_log.insert(
            "train/lr",
            Trainer::get_lr(
                &self.lr_schedule,
                state.progress.step,
                state.get_cold_start_warmup_bounds(),
            ),
        );

        round_log.insert("train/total_tokens", total_tokens(state));
        round_log.insert("train/tokens_per_sec", self.global_tokens_per_second(state));
        round_log.insert("train/global_token_batch_size", token_batch_size(state));
        round_log.insert("train/efficency", self.efficency());

        round_log.insert("coordinator/num_clients", state.epoch_state.clients.len());
        round_log.insert("coordinator/epoch", state.progress.epoch);
        round_log.insert(
            "coordinator/round",
            state.current_round().map(|x| x.height).unwrap_or_default(),
        );

        for (key, val) in self.current_eval_results() {
            round_log.insert(
                format!(
                    "eval/{}",
                    key.to_lowercase()
                        .chars()
                        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
                        .collect::<String>()
                ),
                val,
            );
        }

        for (name, value) in &self.last_optim_stats {
            round_log.insert(format!("optim/{name}"), *value);
        }

        let p2p_nodes: HashMap<String, DataValue> = self
            .node_info
            .iter()
            .map(|(node_id, P2PNodeInfo { ips, bandwidth })| {
                (
                    node_id.to_string(),
                    HashMap::from([
                        ("ips", DataValue::from(ips.join(","))),
                        ("bandwidth", DataValue::from(*bandwidth)),
                    ])
                    .into(),
                )
            })
            .collect();

        round_log.insert("p2p/nodes", p2p_nodes);

        if let Some(run) = self.wandb_run.clone() {
            tokio::spawn(async move {
                run.log(round_log).await;
            });
        }
    }

    pub fn get_witness_metadata<T: NodeIdentity>(&self, state: &Coordinator<T>) -> WitnessMetadata {
        let bandwidth_total: f64 = self.node_info.values().map(|v| v.bandwidth).sum();

        let evals = {
            let mut evals: FixedVec<WitnessEvalResult, 8> = Default::default();
            for (key, val) in self.current_eval_results() {
                let value = WitnessEvalResult::new_trunc_name(&key, no_nan(val as f32, 0.0));
                if evals.push(value).is_err() {
                    // fixedvec is full, that's ok! nothing we can do.
                    break;
                }
            }
            evals
        };

        // NOTE: no NaNs allowed in borsh serialized data.
        let tokens_per_sec = self.global_tokens_per_second(state);
        WitnessMetadata {
            step: state.progress.step,
            tokens_per_sec: no_nan(tokens_per_sec, 0.0),
            bandwidth_per_sec: no_nan(bandwidth_total as f32, 0.0),
            loss: no_nan(
                self.losses().last().copied().unwrap_or(f32::INFINITY),
                f32::INFINITY,
            ),
            efficency: no_nan(self.efficency(), 0.0),
            evals,
            // ACA add prompt results
            prompt_results: FixedVec::default(),
        }
    }

    pub fn push_round_stats(
        &mut self,
        round_losses: &[f32],
        training_round_duration: Duration,
        step_duration: Option<Duration>,
        optim_stats: HashMap<String, f64>,
    ) -> Option<f32> {
        let loss = if !round_losses.is_empty() {
            let loss = round_losses.iter().sum::<f32>() / round_losses.len() as f32;
            self.losses.push(loss);
            Some(loss)
        } else {
            None
        };

        self.training_round_durations.push(training_round_duration);
        if let Some(step_duration) = step_duration {
            self.step_durations.push(step_duration);
        }

        self.last_optim_stats = optim_stats;
        loss
    }

    /// only call this once per step
    /// take the current eval results and push them
    pub fn push_eval_results(&mut self) {
        for (key, value) in self.current_eval_results() {
            self.eval_history
                .entry(key.clone())
                .or_default()
                .push(value);
        }
    }

    pub fn eval_history(&self) -> &HashMap<String, Vec<f64>> {
        &self.eval_history
    }

    pub fn losses(&self) -> &[f32] {
        &self.losses
    }

    pub fn global_tokens_per_second<T: NodeIdentity>(&self, state: &Coordinator<T>) -> f32 {
        match self.step_durations.is_empty() {
            true => 0.,
            false => match &state.model {
                model::Model::LLM(llm) => match llm.data_type {
                    model::LLMTrainingDataType::Pretraining => {
                        let tokens = state.get_target_global_batch_size(state.current_round())
                            as u32
                            * state.get_sequence_length()
                            * self.step_durations.len() as u32;
                        let seconds = self
                            .step_durations
                            .iter()
                            .fold(0f32, |acc, ele| acc + ele.as_secs_f32());
                        if seconds == 0.0 {
                            0.0
                        } else {
                            tokens as f32 / seconds
                        }
                    }
                    model::LLMTrainingDataType::Finetuning => todo!(),
                },
            },
        }
    }

    pub fn efficency(&self) -> f32 {
        let step_seconds = self
            .step_durations
            .iter()
            .fold(0f32, |acc, ele| acc + ele.as_secs_f32());
        let training_round_seconds = self
            .training_round_durations
            .iter()
            .skip(self.training_round_durations.len() - self.step_durations.len())
            .fold(0f32, |acc, ele| acc + ele.as_secs_f32());
        training_round_seconds / step_seconds
    }

    pub fn current_eval_results(&self) -> HashMap<String, f64> {
        self.eval_runner
            .tasks()
            .iter()
            .flatten()
            .flat_map(|eval_task| {
                let task = eval_task.task();
                let metric_name: &str = task.main_metric_name();
                let task_name = task.name();
                match &eval_task.task {
                    EnumTask::EvalTask(eval_task) => {
                        match eval_task.results().sample(metric_name) {
                            Some(metric) => Some((task_name.to_owned(), metric)),
                            None => {
                                warn!("{} missing metric {}", task_name, metric_name);
                                None
                            }
                        }
                    }
                    _ => panic!("Unexpected eval task type"),
                }
            })
            .collect()
    }

    // ACA
    pub fn current_prompt_results(&self) -> Vec<u32> {
        todo!()
    }

    // normalized metric for how "confident" a model is, regardless of vocab size.
    // 1.0 indicates completely certain (no loss), 0.0 indicates random guessing, negative values are worse than guessing
    fn confidence(&self, loss: f32) -> f32 {
        let max_entropy = (self.tokenizer.get_vocab_size(false) as f32).log2();
        1.0 - (loss / max_entropy)
    }
}

fn total_tokens<T: NodeIdentity>(state: &Coordinator<T>) -> u64 {
    state
        .current_round()
        .map(|y| y.data_index)
        .unwrap_or_default()
        * match &state.model {
            model::Model::LLM(llm) => match llm.data_type {
                model::LLMTrainingDataType::Pretraining => llm.max_seq_len as u64,
                model::LLMTrainingDataType::Finetuning => todo!(),
            },
        }
}

fn perplexity(loss: f32) -> f32 {
    loss.exp()
}

fn no_nan(val: f32, replacement: f32) -> f32 {
    if val.is_nan() {
        replacement
    } else {
        val
    }
}

fn token_batch_size<T: NodeIdentity>(state: &Coordinator<T>) -> u32 {
    state.get_target_global_batch_size(state.current_round()) as u32 * state.get_sequence_length()
}
