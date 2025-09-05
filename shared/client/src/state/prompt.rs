use crate::state::prompt_texts::get_prompt_texts;
use psyche_coordinator::MAX_TOKENS_TO_SEND;
use psyche_core::FixedVec;
use psyche_modeling::{CausalLM, EosToks};
use psyche_modeling::{LogitsProcessor, Sampling, Trainer};
use std::sync::{Mutex, RwLock};
use tch::Tensor;
use tokenizers::Tokenizer;
use tokio_util::sync::CancellationToken;
use tracing::{debug, trace};

const MAX_TOKENS_TO_GENERATE: usize = 256;

#[derive(Debug)]
pub struct PromptTask {
    pub selected_prompt: RwLock<usize>,
    tokens: RwLock<Vec<i32>>,
    pub tokens_to_send: RwLock<FixedVec<i32, MAX_TOKENS_TO_SEND>>,
    /// A flag set to `true` once the end-of-sequence token has been generated.
    pub prompt_finished: RwLock<bool>,
    pub is_running: Mutex<bool>,
    original_prompt_len: RwLock<usize>,
    tokenizer: std::sync::Arc<Tokenizer>,
}

impl PromptTask {
    pub fn new(
        selected_prompt: usize,
        task: String,
        tokenizer: &std::sync::Arc<Tokenizer>,
    ) -> Self {
        let encoding = tokenizer.encode(task.clone(), true).unwrap();
        let tokens: Vec<i32> = encoding.get_ids().iter().map(|x| *x as i32).collect();
        let original_prompt_len = tokens.len();

        Self {
            selected_prompt: RwLock::new(selected_prompt),
            tokens: RwLock::new(tokens),
            tokens_to_send: RwLock::new(FixedVec::new()),
            prompt_finished: RwLock::new(false),
            is_running: Mutex::new(false),
            original_prompt_len: RwLock::new(original_prompt_len),
            tokenizer: tokenizer.clone(),
        }
    }

    fn reset_with_new_prompt(&self) {
        use rand::Rng;
        let mut rng = rand::thread_rng();
        let prompt_texts = get_prompt_texts();
        let new_prompt_index = rng.gen_range(0..prompt_texts.len());

        let old_prompt_index = *self.selected_prompt.read().unwrap();
        debug!(
            "Switching from prompt {} to prompt {}",
            old_prompt_index, new_prompt_index
        );

        let new_prompt_text = &prompt_texts[new_prompt_index];
        let encoding = self
            .tokenizer
            .encode(new_prompt_text.as_str(), true)
            .unwrap();
        let new_tokens: Vec<i32> = encoding.get_ids().iter().map(|x| *x as i32).collect();

        // Update the prompt data
        let new_prompt_len = new_tokens.len();
        *self.selected_prompt.write().unwrap() = new_prompt_index;
        *self.tokens.write().unwrap() = new_tokens;
        *self.original_prompt_len.write().unwrap() = new_prompt_len;
        *self.prompt_finished.write().unwrap() = false;

        debug!(
            "Reset to new prompt {}: '{}...'",
            new_prompt_index,
            &new_prompt_text.chars().take(50).collect::<String>()
        );
    }
}

impl PromptTask {
    pub fn run(&self, trainer: &mut Trainer, cancel: CancellationToken) {
        if *self.prompt_finished.read().unwrap() {
            trace!("Prompt already finished, getting new prompt");
            // Reset with a completely new prompt instead of same one
            self.reset_with_new_prompt();
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
        let max_context_length = trainer.max_context_length();
        if token_len > max_context_length {
            self.tokens
                .write()
                .unwrap()
                .drain(0..token_len - max_context_length);
        }

        let input = {
            let tokens = self.tokens.read().unwrap();
            Tensor::from_slice(&tokens)
                .to(trainer.device())
                .unsqueeze(0)
        };

        // Run forward pass
        let (logits, _) = trainer.forward(&input, None, None, None, Some(1), None);

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

        let generated_tokens = token_len - *self.original_prompt_len.read().unwrap();
        if generated_tokens >= MAX_TOKENS_TO_GENERATE {
            *self.prompt_finished.write().unwrap() = true;
        }

        self.tokens_to_send
            .write()
            .unwrap()
            .push(next_token as i32)
            .unwrap();
        self.tokens.write().unwrap().push(next_token as i32);
    }
}
