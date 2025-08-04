use crate::traits::{Document, GenerateUntilTask, LogLikelihoodTask};
use crate::{ASCII_UPPERCASE, ArcChallenge, ArcEasy, Hellaswag, MMLU, MMLUPro, OpenbookQA, PIQA};
use indicatif::{ProgressBar, ProgressStyle};
use psyche_core::RunningAverage;
use psyche_modeling::{CausalLM, LogitsProcessor, Sampling};
use rand::{SeedableRng, seq::SliceRandom};
use rand_chacha::ChaCha8Rng;
use regex::Regex;
use std::sync::RwLock;
use std::{collections::HashMap, fmt::Display, sync::Arc};
use tch::{Kind, Tensor};
use tokenizers::Tokenizer;
use tokio_util::sync::CancellationToken;
use tracing::info;
const GENERATE_UNTIL_MAX_TOKENS: usize = 600;
const GENERATE_UNTIL_MAX_CONTEXT_SIZE: usize = 2048;

const TASKS_WITH_ACC_NORM: [&str; 6] = [
    ArcChallenge::name(),
    ArcEasy::name(),
    Hellaswag::name(),
    MMLU::name(),
    OpenbookQA::name(),
    PIQA::name(),
];

pub enum TaskType {
    LogLikelihood(Box<dyn LogLikelihoodTask>),
    GenerateUntil(Box<dyn GenerateUntilTask>),
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
            TaskType::GenerateUntil(x) => write!(f, "{x}"),
        }
    }
}
#[allow(clippy::large_enum_variant)]
#[derive(Debug)]
enum PreparedTaskType {
    LogLikelihood {
        docs: Vec<TokenizedLLHDocument>,
        tokenized_fewshot: Vec<i64>,
    },
    GenerateUntil {
        requests: Vec<TokenizedGenerateUntilDocument>,
        tokenizer: Tokenizer,
        // Since a single GenerateUntil request can take a long time to generate a answer, we cache the generated tokens
        // in case the task gets interrupted, so next time we can resume from where we left off.
        cache: Arc<RwLock<HashMap<usize, Vec<u32>>>>,
        answer_regex: Regex,
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

#[derive(Debug)]
pub struct TokenizedGenerateUntilDocument {
    _request_str: String,
    request: Vec<i64>,
    answer: usize,
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
            TaskType::GenerateUntil(gu_docs) => {
                let mut docs = gu_docs.get_documents();
                docs.shuffle(&mut self.rand);
                if let Some(limit) = limit {
                    docs.truncate(limit);
                }

                let fewshot = gu_docs.get_fewshot_documents();

                let mut requests = Vec::new();

                // Prepare prompts for each document
                for doc in &docs {
                    // Get the category for this document
                    let category = doc.category.as_deref().unwrap();

                    // Get fewshot examples for this category
                    let fewshot_examples = fewshot.get(category).map(|v| v.as_slice()).unwrap();

                    // Build the prompt string

                    let mut request_str = format!(
                        "The following are multiple choice questions (with answers) about {}. Think step by step and then finish your answer with \"the answer is (X)\" where X is the correct letter choice.\n",
                        category
                    );

                    // Add fewshot examples with their answers
                    for example in fewshot_examples.iter().take(self.num_fewshot) {
                        request_str.push_str("Question:\n");

                        request_str.push_str(&example.text);
                        request_str.push_str("\nOptions:\n");

                        // Format choices with letter labels
                        for (i, choice) in example.choices.iter().enumerate() {
                            let letter = ASCII_UPPERCASE[i];
                            request_str.push_str(&format!("{}. {}\n", letter, choice));
                        }

                        // Replace "A:" with "Answer:" in cot_content
                        let mut cot_content = example.cot_content.as_ref().unwrap().clone();
                        if cot_content.starts_with("A:") {
                            cot_content = format!("Answer:{}", &cot_content[2..]);
                        }
                        request_str.push_str(&cot_content);
                        request_str.push_str("\n\n");
                    }

                    // Add the current question without answer
                    request_str.push_str("Question:\n");
                    request_str.push_str(&doc.text);
                    request_str.push_str("\nOptions:\n");

                    // Format choices with letter labels
                    for (i, choice) in doc.choices.iter().enumerate() {
                        let letter = ASCII_UPPERCASE[i];
                        request_str.push_str(&format!("{}. {}\n", letter, choice));
                    }

                    request_str.push_str("Answer: Let's think step by step.");

                    // Tokenize the request
                    let request = tokenizer
                        .encode(request_str.clone(), false)
                        .unwrap()
                        .get_ids()
                        .iter()
                        .map(|x| *x as i64)
                        .collect::<Vec<_>>();

                    // Create the tokenized document
                    let tokenized_doc = TokenizedGenerateUntilDocument {
                        _request_str: request_str,
                        request,
                        answer: doc.answer,
                    };

                    requests.push(tokenized_doc);
                }

                // Regex to match "The answer is (X)." where X is a single uppercase letter;
                let answer_regex = Regex::new(r"The answer is \(([A-Z])\)\.").unwrap();

                PreparedTask {
                    name,
                    num: docs.len(),
                    prepared_task_type: PreparedTaskType::GenerateUntil {
                        requests,
                        tokenizer: tokenizer.clone(),
                        cache: Arc::new(RwLock::new(HashMap::new())),
                        answer_regex,
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
            } => Self::run_log_likelihood(&self.name, options, docs, tokenized_fewshot, pbar),
            PreparedTaskType::GenerateUntil {
                requests,
                tokenizer,
                cache,
                answer_regex,
            } => Self::run_generate_until(
                &self.name,
                options,
                cache.clone(),
                requests,
                tokenizer,
                answer_regex,
                pbar,
            ),
        }
    }

    fn run_log_likelihood(
        eval_name: &String,
        options: EvalTaskOptions,
        docs: &[TokenizedLLHDocument],
        tokenized_fewshot: &[i64],
        pbar: Option<ProgressBar>,
    ) -> PreparedTaskResult {
        let results = options.live_results.unwrap_or_default();
        let (mut skip, step_by) = options.skip_and_step_by.unwrap_or((0, 1));
        results.add_entry_if_needed("acc", docs.len());
        if TASKS_WITH_ACC_NORM.contains(&eval_name.as_str()) {
            results.add_entry_if_needed("acc_norm", docs.len());
        }
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

            if TASKS_WITH_ACC_NORM.contains(&eval_name.as_str()) {
                results.push(
                    "acc_norm",
                    match selected_norm as usize == doc.answer {
                        true => 1.,
                        false => 0.,
                    },
                );
            }

            if let Some(pbar) = &pbar {
                pbar.set_message(format!("acc: {:.3}", results.sample("acc").unwrap()));
                pbar.inc(1);
            };
        }

        PreparedTaskResult {
            scores: get_all_averages(results, eval_name),
            next_index: next_index + fast_forward,
            cancelled,
        }
    }

    fn run_generate_until(
        eval_name: &String,
        options: EvalTaskOptions,
        cache: Arc<RwLock<HashMap<usize, Vec<u32>>>>,
        requests: &[TokenizedGenerateUntilDocument],
        tokenizer: &Tokenizer,
        answer_regex: &Regex,
        pbar: Option<ProgressBar>,
    ) -> PreparedTaskResult {
        let results = options.live_results.unwrap_or_default();
        let (mut skip, step_by) = options.skip_and_step_by.unwrap_or((0, 1));
        results.add_entry_if_needed("acc", requests.len());
        let fast_forward = (skip / requests.len()) * requests.len();
        skip -= fast_forward;
        let mut cancelled = false;
        let mut documents_processed = 0;

        // Simple sampling setup
        let mut logits_processor = LogitsProcessor::from_sampling(
            0,
            Sampling::ArgMax, // Greedy decoding for deterministic results
        );

        // Get EOS token IDs from model
        let eos_token_ids = options.model.eos_token_ids();

        for (
            num_iterations,
            (
                doc_index,
                &TokenizedGenerateUntilDocument {
                    ref _request_str,
                    ref request,
                    answer,
                },
            ),
        ) in requests
            .iter()
            .cycle()
            .enumerate()
            .skip(skip)
            .step_by(step_by)
            .enumerate()
        {
            if let Some(cancel) = options.cancel.as_ref() {
                if cancel.is_cancelled() {
                    cancelled = true;
                    break;
                }
            }
            if doc_index >= requests.len() {
                break;
            }
            if let Some(limit) = options.limit {
                if num_iterations >= limit {
                    break;
                }
            }

            let mut generated_answer = None;
            let mut generation_complete = false;

            // Start with the tokenized prompt
            let mut full_sequence = request.clone();

            // Check if we have cached generated tokens for this document
            let mut generated_tokens = {
                cache
                    .read()
                    .unwrap()
                    .get(&doc_index)
                    .cloned()
                    .unwrap_or_else(Vec::new)
            };

            if !generated_tokens.is_empty() {
                tracing::trace!(
                    "Resuming generation for document {} from checkpoint with {} tokens",
                    doc_index,
                    generated_tokens.len()
                );
            }

            // If we have cached tokens, append them to the prompt
            if !generated_tokens.is_empty() {
                full_sequence.extend(generated_tokens.iter().map(|&t| t as i64));
            }

            // Generate tokens until we find "The answer is" pattern or reach limit
            let mut tokens_generated_count = generated_tokens.len();
            while !generation_complete {
                if let Some(cancel) = options.cancel.as_ref() {
                    if cancel.is_cancelled() {
                        // Save progress before cancelling
                        cache
                            .write()
                            .unwrap()
                            .insert(doc_index, generated_tokens.clone());
                        tracing::trace!(
                            "Cancellation requested: saving {} tokens for document {}",
                            generated_tokens.len(),
                            doc_index,
                        );
                        cancelled = true;
                        break;
                    }
                }
                if full_sequence.len() > GENERATE_UNTIL_MAX_CONTEXT_SIZE {
                    full_sequence.drain(0..(full_sequence.len() - GENERATE_UNTIL_MAX_CONTEXT_SIZE));
                }
                let model_input = Tensor::from_slice(&full_sequence)
                    .to(options.model.device())
                    .unsqueeze(0);

                let (logits, _) =
                    options
                        .model
                        .forward(&model_input, None, None, None, Some(1), None);
                let logits = logits.squeeze();

                let next_token = logits_processor.sample(&logits).unwrap();
                full_sequence.push(next_token as i64);
                generated_tokens.push(next_token);
                tokens_generated_count += 1;

                // Check if we hit an EOS token
                if let Some(eos_ids) = &eos_token_ids {
                    if eos_ids.contains(next_token as i64) {
                        generation_complete = true;
                        break;
                    }
                }

                // Decode all generated tokens together
                if let Ok(generated_text) = tokenizer.decode(&generated_tokens, false) {
                    // Check if we've generated "The answer is (X)" pattern using regex
                    if let Some(captures) = answer_regex.captures(&generated_text) {
                        if let Some(answer_char) = captures.get(1) {
                            generated_answer = Some(
                                crate::ASCII_UPPERCASE
                                    .iter()
                                    .position(|&c| c == answer_char.as_str())
                                    .unwrap_or(usize::MAX),
                            );
                            generation_complete = true;

                            break;
                        }
                    }
                }

                if tokens_generated_count >= GENERATE_UNTIL_MAX_TOKENS {
                    generation_complete = true;
                    break;
                }
            }

            // Clear the cache for this document after successful completion
            if generation_complete {
                cache.write().unwrap().remove(&doc_index);

                let score = if generated_answer == Some(answer) {
                    1.
                } else {
                    0.
                };
                results.push("acc", score);
                documents_processed += 1;

                if let Some(pbar) = &pbar {
                    pbar.set_message(format!("acc: {:.3}", results.sample("acc").unwrap()));
                    pbar.inc(1);
                };
            }
        }

        PreparedTaskResult {
            scores: get_all_averages(results, eval_name),
            next_index: fast_forward + skip + (documents_processed * step_by),
            cancelled,
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn main_metric_name(&self) -> &str {
        if TASKS_WITH_ACC_NORM.contains(&self.name()) {
            "acc_norm"
        } else {
            "acc"
        }
    }
}

/// Get averages of all metrics with sufficient samples
/// Filters out metrics with too few samples to ensure confident scores
fn get_all_averages(
    running_average: Arc<RunningAverage>,
    eval_name: &String,
) -> HashMap<String, f64> {
    let entries = running_average.entries.read().unwrap();
    entries
        .iter()
        .filter(|(_metric_name, entry)| {
            if eval_name == MMLUPro::name() {
                entry.buffer.len() > entry.max_size / 10
            } else {
                entry.buffer.len() > entry.max_size / 2
            }
        })
        .map(|(metric_name, entry)| (metric_name.clone(), entry.average().unwrap()))
        .collect()
}
