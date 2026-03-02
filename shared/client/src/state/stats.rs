use psyche_coordinator::{
    Coordinator, MAX_TOKENS_TO_SEND, WitnessEvalResult, WitnessMetadata, model,
};
use psyche_core::{BoundedQueue, FixedVec, LearningRateSchedule, NodeIdentity};
use psyche_metrics::ClientMetrics;
use psyche_modeling::Trainer;
use psyche_network::P2PEndpointInfo;
use std::{collections::HashMap, sync::Arc, time::Duration};
use tokenizers::Tokenizer;
use tracing::{debug, trace, warn};
use wandb::{DataValue, LogData};

use crate::state::evals::{EnumModelTask, PROMPT_TASK_NAME};

use super::evals::ModelTaskRunner;

pub struct StatsLogger {
    tokenizer: Arc<Tokenizer>,
    wandb_run: Option<Arc<wandb::Run>>,
    pub metrics: Arc<ClientMetrics>,
    model_task_runner: ModelTaskRunner,

    step_durations: BoundedQueue<Duration, 16>,
    training_round_durations: BoundedQueue<Duration, 16>,

    losses: Vec<f32>,
    last_optim_stats: HashMap<String, f64>,
    eval_history: HashMap<String, Vec<f64>>,
    lr_schedule: LearningRateSchedule,

    pub endpoint_info: Vec<P2PEndpointInfo>,
}

impl StatsLogger {
    pub fn new(
        tokenizer: Arc<Tokenizer>,
        model_task_runner: ModelTaskRunner,
        lr_schedule: LearningRateSchedule,
        wandb_run: Option<wandb::Run>,
        metrics: Arc<ClientMetrics>,
    ) -> Self {
        Self {
            tokenizer,
            wandb_run: wandb_run.map(Arc::new),
            losses: Vec::new(),
            step_durations: Default::default(),
            training_round_durations: Default::default(),
            model_task_runner,
            lr_schedule,
            eval_history: HashMap::new(),
            last_optim_stats: HashMap::new(),
            endpoint_info: Vec::new(),
            metrics,
        }
    }

    pub fn publish_round_stats<T: NodeIdentity>(&self, state: &Coordinator<T>) {
        let mut round_log = LogData::new();

        round_log.insert("_step", state.progress.step);

        // Training metrics
        if let Some(loss) = self.losses().last() {
            let loss_val = *loss;
            let perplexity_val = perplexity(loss_val);
            let confidence_val = self.confidence(loss_val);

            round_log.insert("train/loss", loss_val);
            round_log.insert("train/perplexity", perplexity_val);
            round_log.insert("train/confidence", confidence_val);

            // Log to metrics
            self.metrics.record_training_loss(loss_val as f64);
            self.metrics
                .record_training_perplexity(perplexity_val as f64);
            self.metrics
                .record_training_confidence(confidence_val as f64);
        }

        let lr = Trainer::get_lr(
            &self.lr_schedule,
            state.progress.step,
            state.get_cold_start_warmup_bounds(),
        );
        round_log.insert("train/lr", lr);
        self.metrics.record_learning_rate(lr);

        let total_tokens_val = total_tokens(state);
        let tokens_per_sec_val = self.global_tokens_per_second(state);
        let token_batch_size_val = token_batch_size(state);
        let efficiency_val = self.efficency();

        round_log.insert("train/total_tokens", total_tokens_val);
        round_log.insert("train/tokens_per_sec", tokens_per_sec_val);
        round_log.insert("train/global_token_batch_size", token_batch_size_val);
        round_log.insert("train/efficency", efficiency_val);

        self.metrics.record_total_tokens(total_tokens_val);
        self.metrics
            .record_tokens_per_second(tokens_per_sec_val as f64);
        self.metrics
            .record_token_batch_size(token_batch_size_val as u64);
        self.metrics
            .record_training_efficiency(efficiency_val as f64);
        if let Some(last_train_time) = self.training_round_durations.iter().last() {
            self.metrics
                .record_last_train_time(last_train_time.as_secs_f64());
        }
        // Coordinator metrics
        let num_clients = state.epoch_state.clients.len();
        let epoch = state.progress.epoch;
        let round_height = state.current_round().map(|x| x.height).unwrap_or_default();

        round_log.insert("coordinator/num_clients", num_clients);
        round_log.insert("coordinator/epoch", epoch);
        round_log.insert("coordinator/round", round_height);

        // Eval metrics
        for (key, val) in self.current_eval_results() {
            let formatted_key = format!(
                "eval/{}",
                key.to_lowercase()
                    .chars()
                    .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
                    .collect::<String>()
            );
            round_log.insert(formatted_key.clone(), val);

            self.metrics.record_eval_metric(&key, val);
        }

        // Optimizer metrics
        for (name, value) in &self.last_optim_stats {
            let optim_key = format!("optim/{name}");
            round_log.insert(optim_key, *value);

            self.metrics.record_optimizer_stat(name, *value);
        }

        // P2P nodes (only for wandb, not metrics as requested)
        let p2p_nodes: HashMap<String, DataValue> = self
            .endpoint_info
            .iter()
            .map(
                |P2PEndpointInfo {
                     id: endpoint_id,
                     selected_path,
                     bandwidth,
                     latency,
                 }| {
                    let path_str = selected_path
                        .as_ref()
                        .map(|p| p.to_string())
                        .unwrap_or_else(|| "none".to_string());
                    (
                        endpoint_id.to_string(),
                        HashMap::from([
                            ("path", DataValue::from(path_str)),
                            ("bandwidth", DataValue::from(*bandwidth)),
                            ("latency", DataValue::from(*latency)),
                        ])
                        .into(),
                    )
                },
            )
            .collect();

        round_log.insert("p2p/nodes", p2p_nodes);

        // Log to wandb
        if let Some(run) = self.wandb_run.clone() {
            tokio::spawn(async move {
                run.log(round_log).await;
            });
        }
    }

    pub fn get_witness_metadata<T: NodeIdentity>(&self, state: &Coordinator<T>) -> WitnessMetadata {
        let bandwidth_total: f64 = self.endpoint_info.iter().map(|v| v.bandwidth).sum();

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

        let prompt_results = self.get_prompt_results();
        let prompt_index = self.get_prompt_index();

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
            prompt_results,
            prompt_index,
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
                model::Model::LLM(_) => {
                    let tokens = state.get_target_global_batch_size(state.current_round()) as u32
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
        self.model_task_runner
            .tasks()
            .iter()
            .flatten()
            .filter(|model_task| model_task.name() != PROMPT_TASK_NAME)
            .flat_map(|model_task| match &model_task.task {
                EnumModelTask::EvalTask(eval_task) => {
                    let metric_name: &str = eval_task.task.main_metric_name();
                    let task_name = model_task.name();
                    match eval_task.results().sample(metric_name) {
                        Some(metric) => Some((task_name.to_owned(), metric)),
                        None => {
                            warn!("{} missing metric {}", task_name, metric_name);
                            None
                        }
                    }
                }
                EnumModelTask::PromptTask(_) => None,
            })
            .collect()
    }

    // clear tokens_to_send buffer
    pub fn get_prompt_results(&self) -> FixedVec<i32, MAX_TOKENS_TO_SEND> {
        let mut results = FixedVec::new();
        for eval_task in self.model_task_runner.tasks().iter().flatten() {
            if let EnumModelTask::PromptTask(prompt_task) = &eval_task.task {
                {
                    let tokens = prompt_task.tokens_to_send.read().unwrap();
                    results.extend(tokens.iter().cloned()).unwrap();
                }
                if let Ok(decoded) = prompt_task
                    .tokenizer
                    .decode(&results.iter().map(|x| *x as u32).collect::<Vec<_>>(), true)
                {
                    debug!("Prompt result: {}", decoded);
                }
                prompt_task.tokens_to_send.write().unwrap().clear();
            }
        }
        trace!(
            "Final witness prompt results: {:?}",
            results.iter().collect::<Vec<_>>()
        );
        results
    }

    // Get current prompt index for witness metadata
    pub fn get_prompt_index(&self) -> u8 {
        for eval_task in self.model_task_runner.tasks().iter().flatten() {
            if let EnumModelTask::PromptTask(prompt_task) = &eval_task.task {
                return *prompt_task.selected_prompt.read().unwrap() as u8;
            }
        }
        // Default to 0 if no prompt task found
        0
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
            model::Model::LLM(llm) => llm.max_seq_len as u64,
        }
}

fn perplexity(loss: f32) -> f32 {
    loss.exp()
}

fn no_nan(val: f32, replacement: f32) -> f32 {
    if val.is_nan() { replacement } else { val }
}

fn token_batch_size<T: NodeIdentity>(state: &Coordinator<T>) -> u32 {
    state.get_target_global_batch_size(state.current_round()) as u32 * state.get_sequence_length()
}
