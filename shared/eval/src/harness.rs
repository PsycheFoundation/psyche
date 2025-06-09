use crate::traits::{Document, LogLikelihoodTask};
use indicatif::{ProgressBar, ProgressStyle};
use psyche_core::RunningAverage;
use psyche_modeling::CausalLM;
use rand::{seq::SliceRandom, SeedableRng};
use rand_chacha::ChaCha8Rng;
use std::{collections::HashMap, fmt::Display, sync::Arc};
use tch::{Kind, Tensor};
use tokenizers::Tokenizer;
use tokio_util::sync::CancellationToken;
use tracing::info;

pub enum TaskType {
    LogLikelihood(Box<dyn LogLikelihoodTask>),
}

pub struct Task {
    task_type: TaskType,
    num_fewshot: usize,
    rand: ChaCha8Rng,
}

impl Task {
    pub fn new(task_type: TaskType, num_fewshot: usize, random_seed: u64) -> Self {
        let mut seed = [0u8; 32];
        seed[24..32].copy_from_slice(&random_seed.to_be_bytes());
        Task {
            task_type,
            num_fewshot,
            rand: ChaCha8Rng::from_seed(seed),
        }
    }
}

impl Display for Task {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.task_type {
            TaskType::LogLikelihood(x) => write!(f, "{x}"),
        }
    }
}

#[derive(Debug)]
enum PreparedTaskType {
    LogLikelihood {
        docs: Vec<TokenizedLLHDocument>,
        tokenized_fewshot: Vec<i64>,
    },
}

#[derive(Debug)]
pub struct PreparedTask {
    prepared_task_type: PreparedTaskType,
    name: String,
    num: usize,
}

pub struct PreparedTaskResult {
    pub scores: HashMap<String, f64>,
    pub next_index: usize,
    pub cancelled: bool,
}

#[derive(Debug)]
struct TokenizedLLHDocument {
    text: Vec<i64>,
    choices: Vec<Vec<i64>>,
    answer: usize,
}

impl TokenizedLLHDocument {
    pub fn from_document(doc: Document, tokenizer: &Tokenizer) -> Self {
        let text = tokenizer
            .encode(doc.text, false)
            .unwrap()
            .get_ids()
            .iter()
            .map(|x| *x as i64)
            .collect::<Vec<_>>();
        let choices = doc
            .choices
            .into_iter()
            .map(|x| {
                let choice = tokenizer
                    .encode(x.clone(), false)
                    .unwrap()
                    .get_ids()
                    .iter()
                    .map(|x| *x as i64)
                    .collect::<Vec<_>>();
                choice
            })
            .collect();
        Self {
            text,
            choices,
            answer: doc.answer,
        }
    }
}

impl Task {
    pub fn prepare(
        mut self,
        tokenizer: &Tokenizer,
        bos_token_id: Option<i64>,
        limit: Option<usize>,
    ) -> PreparedTask {
        let name = format!("{}", &self);
        info!("Preparing {name}");
        match self.task_type {
            TaskType::LogLikelihood(llh) => {
                let mut docs = llh.get_documents();
                docs.shuffle(&mut self.rand);
                if let Some(limit) = limit {
                    docs.truncate(limit);
                }
                let fewshot = if self.num_fewshot > 0 {
                    let mut fewshot_docs = llh.get_fewshot_documents();
                    fewshot_docs.shuffle(&mut self.rand);
                    fewshot_docs
                        .into_iter()
                        .take(self.num_fewshot)
                        .map(|x| format!("{}{}", x.text, x.choices[x.answer]))
                        .collect::<Vec<_>>()
                        .join("\n\n")
                        + "\n\n"
                } else {
                    String::new()
                };
                let mut tokenized_fewshot = match bos_token_id {
                    Some(bos_token_id) => vec![bos_token_id],
                    None => Vec::new(),
                };
                tokenized_fewshot.append(
                    &mut tokenizer
                        .encode(fewshot, false)
                        .unwrap()
                        .get_ids()
                        .iter()
                        .map(|x| *x as i64)
                        .collect::<Vec<_>>(),
                );
                let docs = docs
                    .into_iter()
                    .map(|x| TokenizedLLHDocument::from_document(x, tokenizer))
                    .collect::<Vec<_>>();
                PreparedTask {
                    name,
                    num: docs.len(),
                    prepared_task_type: PreparedTaskType::LogLikelihood {
                        docs,
                        tokenized_fewshot,
                    },
                }
            }
        }
    }
}

pub struct EvalTaskOptions<'a> {
    pub model: &'a mut dyn CausalLM,
    pub skip_and_step_by: Option<(usize, usize)>,
    pub live_results: Option<Arc<RunningAverage>>,
    pub cancel: Option<CancellationToken>,
    pub limit: Option<usize>,
    pub loop_if_empty: bool,
}

impl PreparedTask {
    pub fn run(&self, options: EvalTaskOptions, progress_bar: bool) -> PreparedTaskResult {
        let pbar = match progress_bar {
            false => None,
            true => {
                info!("Running {}", self.name);
                let pbar = ProgressBar::new(self.num as u64);
                pbar.set_style(ProgressStyle::default_bar()
                    .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} ({eta}) {msg}")
                    .unwrap()
                    .progress_chars("#>-"));
                Some(pbar)
            }
        };

        match &self.prepared_task_type {
            PreparedTaskType::LogLikelihood {
                docs,
                tokenized_fewshot,
            } => Self::run_log_likelihood(options, docs, tokenized_fewshot, pbar),
        }
    }

    fn run_log_likelihood(
        options: EvalTaskOptions,
        docs: &[TokenizedLLHDocument],
        tokenized_fewshot: &[i64],
        pbar: Option<ProgressBar>,
    ) -> PreparedTaskResult {
        let results = options.live_results.unwrap_or_default();
        let (mut skip, step_by) = options.skip_and_step_by.unwrap_or((0, 1));
        results.add_entry_if_needed("acc", docs.len());
        results.add_entry_if_needed("acc_norm", docs.len());
        let mut next_index = skip;
        let fast_forward = (skip / docs.len()) * docs.len();
        skip -= fast_forward;
        let mut cancelled = false;

        for (num_iterations, (doc_index, doc)) in docs
            .iter()
            .cycle()
            .enumerate()
            .skip(skip)
            .step_by(step_by)
            .enumerate()
        {
            next_index = doc_index;
            if let Some(cancel) = options.cancel.as_ref() {
                if cancel.is_cancelled() {
                    cancelled = true;
                    break;
                }
            }
            if !options.loop_if_empty && doc_index >= docs.len() {
                break;
            }
            if let Some(limit) = options.limit {
                if num_iterations >= limit {
                    break;
                }
            }
            let mut context = tokenized_fewshot.to_vec();
            context.extend_from_slice(&doc.text);
            let mut scores: Vec<(f32, bool)> = Vec::new();
            if doc.choices.iter().all(|x| x.len() == 1) {
                let ids = Tensor::from_slice(&context)
                    .to(options.model.device())
                    .unsqueeze(0);
                let (logits, _) = options.model.forward(&ids, None, Some(1));
                let logits = logits.squeeze().log_softmax(-1, None);
                let greedy: i64 = logits.argmax(-1, false).try_into().unwrap();
                let index =
                    Tensor::from_slice(&doc.choices.iter().map(|x| x[0]).collect::<Vec<_>>())
                        .to(logits.device());
                let logits = logits.gather(-1, &index, false);
                let logits: Vec<f32> = logits.try_into().unwrap();
                scores.extend(
                    logits
                        .into_iter()
                        .zip(doc.choices.iter())
                        .map(|(score, choice)| (score, choice[0] == greedy)),
                );
            } else {
                for choice in &doc.choices {
                    let mut ids = context.clone();
                    ids.extend_from_slice(choice);
                    let ids = Tensor::from_slice(&ids)
                        .to(options.model.device())
                        .unsqueeze(0);
                    // if the continuation is N tokens, we need the the last N + 1 logits, since we are getting the
                    // probs at the last token of the prompt (so prediction of the first continuation)
                    let (logits, _) =
                        options
                            .model
                            .forward(&ids, None, Some((choice.len() + 1) as i64));
                    // drop the last logit, since we don't want to score what comes after the continuation
                    let logits = logits.log_softmax(-1, None).squeeze_dim(0).slice(
                        0,
                        0,
                        choice.len() as i64,
                        1,
                    );
                    let greedy_tokens: Vec<i64> = logits.argmax(-1, false).try_into().unwrap();
                    let exact_match = greedy_tokens.eq(choice);
                    let index = Tensor::from_slice(choice).to(logits.device()).unsqueeze(-1);
                    let logits = logits.gather(-1, &index, false);
                    let loglikelihood: f32 = logits.sum(Kind::Float).try_into().unwrap();
                    scores.push((loglikelihood, exact_match));
                }
            }
            let selected: i64 = Tensor::from_slice(&scores.iter().map(|x| x.0).collect::<Vec<_>>())
                .argmax(-1, false)
                .try_into()
                .unwrap();
            let selected_norm: i64 = Tensor::from_slice(
                &scores
                    .iter()
                    .enumerate()
                    .map(|(idx, x)| x.0 / doc.choices[idx].len() as f32)
                    .collect::<Vec<_>>(),
            )
            .argmax(-1, false)
            .try_into()
            .unwrap();

            results.push(
                "acc",
                match selected as usize == doc.answer {
                    true => 1.,
                    false => 0.,
                },
            );
            results.push(
                "acc_norm",
                match selected_norm as usize == doc.answer {
                    true => 1.,
                    false => 0.,
                },
            );

            if let Some(pbar) = &pbar {
                pbar.set_message(format!(
                    "acc_norm: {:.3}",
                    results.sample("acc_norm").unwrap()
                ));
                pbar.inc(1);
            };
        }
        PreparedTaskResult {
            scores: results
                .get_all_averages()
                .into_iter()
                .map(|(key, value)| (key, value.unwrap_or_default()))
                .collect(),
            next_index: next_index + fast_forward,
            cancelled,
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn main_metric_name(&self) -> &str {
        match &self.prepared_task_type {
            PreparedTaskType::LogLikelihood {
                docs: _,
                tokenized_fewshot: _,
            } => "acc_norm",
        }
    }
}
