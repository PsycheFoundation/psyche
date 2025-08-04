use psyche_coordinator::MAX_TOKENS_TO_SEND;
use psyche_core::FixedVec;
use psyche_modeling::{CausalLM, EosToks};
use psyche_modeling::{LogitsProcessor, Sampling, Trainer};
use std::sync::RwLock;
use tch::Tensor;
use tokenizers::Tokenizer;
use tokio_util::sync::CancellationToken;
use tracing::{debug, trace};

const MAX_CONTEXT_LENGTH: usize = 2048;

#[derive(Debug)]
pub struct PromptTask {
    pub selected_prompt: usize,
    tokens: RwLock<Vec<i32>>,
    pub tokens_to_send: RwLock<FixedVec<i32, MAX_TOKENS_TO_SEND>>,
    /// A flag set to `true` once the end-of-sequence token has been generated.
    pub prompt_finished: RwLock<bool>,
    pub is_running: RwLock<bool>,
}

impl PromptTask {
    pub fn new(selected_prompt: usize, task: String, tokenizer: &Tokenizer) -> Self {
        let encoding = tokenizer.encode(task.clone(), true).unwrap();
        let tokens = encoding.get_ids().iter().map(|x| *x as i32).collect();

        Self {
            selected_prompt,
            tokens: RwLock::new(tokens),
            tokens_to_send: RwLock::new(FixedVec::new()),
            prompt_finished: RwLock::new(false),
            is_running: RwLock::new(false),
        }
    }
}

impl PromptTask {
    pub fn run(&self, trainer: &mut Trainer, cancel: CancellationToken) {
        if *self.prompt_finished.read().unwrap() {
            trace!("Prompt already finished");
            return;
        }
        if self.tokens_to_send.read().unwrap().is_full() {
            trace!("Prompt Buffer Full");
            return;
        }
        if cancel.is_cancelled() {
            trace!("Prompt cancelled");
            return;
        }

        // read input tokens
        let token_len = self.tokens.read().unwrap().len();
        if token_len > MAX_CONTEXT_LENGTH {
            self.tokens
                .write()
                .unwrap()
                .drain(0..token_len - MAX_CONTEXT_LENGTH);
        }

        let input = {
            let tokens = self.tokens.read().unwrap();
            Tensor::from_slice(&tokens)
                .to(trainer.device())
                .unsqueeze(0)
        };

        // Run forward pass
        let (logits, _) = trainer.forward(&input, None, Some(1), None);

        let logits = logits.squeeze();

        // sample next token
        let mut logits_processor =
            LogitsProcessor::from_sampling(rand::random(), Sampling::All { temperature: 0.6 });

        let next_token = logits_processor
            .sample(&logits)
            .expect("Failed to sample next token");

        // check if we have reached the end-of-sequence token
        match trainer.eos_token_ids() {
            Some(EosToks::Single(eos_tok_id)) if next_token as i64 == eos_tok_id => {
                *self.prompt_finished.write().unwrap() = true;
            }
            Some(EosToks::Multiple(ref eos_ids)) if eos_ids.contains(&(next_token as i64)) => {
                *self.prompt_finished.write().unwrap() = true;
            }
            _ => (),
        }

        self.tokens_to_send
            .write()
            .unwrap()
            .push(next_token as i32)
            .unwrap();
        self.tokens.write().unwrap().push(next_token as i32);

        debug!("Prompt generated token: {:?}", next_token);
    }
}
