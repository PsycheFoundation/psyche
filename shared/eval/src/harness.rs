use crate::traits::{Document, LogLikelihoodTask};
use indicatif::{ProgressBar, ProgressStyle};
use psyche_core::RunningAverage;
use psyche_modeling::CausalLM;
use rand::{SeedableRng, seq::SliceRandom};
use rand_chacha::ChaCha8Rng;
use std::{
    collections::HashMap,
    fmt::{Debug, Display},
    sync::Arc,
};
use tch::{Kind, Tensor};
use tokenizers::Tokenizer;
use tokio_util::sync::CancellationToken;
use tracing::info;

pub enum TaskType {
    LogLikelihood(Box<dyn LogLikelihoodTask>),
}

impl Debug for TaskType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TaskType::LogLikelihood(x) => write!(f, "Log likelihood task"),
        }
    }
}

#[derive(Debug)]
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
    choices_str: Vec<String>,
    answer: usize,
    choices_token_len: Vec<usize>,
    requests: Vec<Vec<i64>>,
}

impl TokenizedLLHDocument {
    pub fn from_document(doc: Document, tokenizer: &Tokenizer) -> Self {
        let text = tokenizer
            .encode(doc.text.clone(), false)
            .unwrap()
            .get_ids()
            .iter()
            .map(|x| *x as i64)
            .collect::<Vec<_>>();
        // e.g.
        // choice: 'Sunlight is the source of energy for nearly all ecosystems.'
        // text: 'Which statement best explains why photosynthesis is the foundation of most food webs?'
        // request: 'Which statement best explains why photosynthesis is the foundation of most food webs? Sunlight is the source of energy for nearly all ecosystems.'

        let mut requests: Vec<Vec<i64>> = Vec::new();
        let mut choices_str = Vec::new();
        let mut choices_token_len = Vec::new();
        let mut choices: Vec<Vec<i64>> = Vec::new();

        for choice in doc.choices.iter() {
            choices_str.push(choice.clone());

            let request = tokenizer
                .encode(format!("{} {}", doc.text, choice), false)
                .unwrap()
                .get_ids()
                .iter()
                .map(|x| *x as i64)
                .collect::<Vec<_>>();
            requests.push(request.clone());

            // Tokenizing "choice" alone produces different tokens than tokenizing "text + choice" together.
            // So, we extract choice tokens iterating the full request backwards to ensure exact matching.
            for idx in 1..request.len() {
                let choice_tokens = &request[request.len() - idx..]
                    .iter()
                    .map(|x| *x as u32)
                    .collect::<Vec<_>>();
                let choice_str = tokenizer.decode(choice_tokens, false).unwrap();
                if choice_str.contains(choice) {
                    let choice_tokens = choice_tokens.iter().map(|x| *x as i64).collect::<Vec<_>>();
                    choices.push(choice_tokens.clone());
                    choices_token_len.push(choice_tokens.len());

                    break;
                }
            }
        }

        // verify correctness
        for x in 0..requests.len() {
            debug_assert_eq!(
                requests[x][requests[x].len() - choices_token_len[x]..],
                choices[x]
            );
        }

        Self {
            text,
            choices_str,
            answer: doc.answer,
            requests,
            choices_token_len,
        }
    }
}

impl Task {
    pub fn prepare(mut self, tokenizer: &Tokenizer, limit: Option<usize>) -> PreparedTask {
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

                let tokenized_fewshot = tokenizer
                    .encode(fewshot, false)
                    .unwrap()
                    .get_ids()
                    .iter()
                    .map(|x| *x as i64)
                    .collect::<Vec<_>>();
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
    pub min_reporting_ratio: Option<f32>,
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
        let min_samples = options
            .min_reporting_ratio
            .map(|x| (x * docs.len() as f32) as usize);
        results.add_entry_if_needed("acc", docs.len(), min_samples);
        results.add_entry_if_needed("acc_norm", docs.len(), min_samples);
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
            if doc_index >= docs.len() {
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

            for idx in 0..doc.requests.len() {
                // e.g:
                // request: 'Which statement best explains why photosynthesis is the foundation of most food webs? Sunlight is the source of energy for nearly all ecosystems.'
                let mut request = doc.requests[idx].clone();
                // choice: 'Sunlight is the source of energy for nearly all ecosystems.'
                let choice = &doc.requests[idx][request.len() - doc.choices_token_len[idx]..];

                // Remove the last token since we dont want to pass it to the model
                // request: 'Which statement best explains why photosynthesis is the foundation of most food webs? Sunlight is the source of energy for nearly all ecosystems'
                request.pop();
                let input_lenght = &request.len();

                let request = Tensor::from_slice(&request)
                    .to(options.model.device())
                    .unsqueeze(0);

                let (logits, _) = {
                    let _no_grad = tch::no_grad_guard();
                    options
                        .model
                        .forward(&request, None, None, None, None, None)
                };

                let logits = logits.squeeze_dim(0).slice(0, 0, None, 1);

                // Get tensor of shape `[choice.len(), vocab_size]` containing the
                // model's logits for each token of the `choice` text.
                let logits = logits.slice(
                    0,
                    *input_lenght as i64 - choice.len() as i64,
                    *input_lenght as i64,
                    1,
                );

                let greedy_tokens: Vec<i64> = logits.argmax(-1, false).try_into().unwrap();
                let exact_match = greedy_tokens.eq(&choice);

                let choice_log_prob = logits.log_softmax(-1, None).gather(
                    -1,
                    &Tensor::from_slice(choice).to(logits.device()).unsqueeze(-1),
                    false,
                );

                let loglikelihood: f32 = choice_log_prob.sum(Kind::Float).try_into().unwrap();
                scores.push((loglikelihood, exact_match));
            }

            let selected: i64 = Tensor::from_slice(&scores.iter().map(|x| x.0).collect::<Vec<_>>())
                .argmax(-1, false)
                .try_into()
                .unwrap();
            let selected_norm: i64 = Tensor::from_slice(
                &scores
                    .iter()
                    .enumerate()
                    .map(|(idx, score)| score.0 / (doc.choices_str[idx].len() as f32))
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
