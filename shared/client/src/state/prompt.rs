use psyche_modeling::CausalLM;
use psyche_modeling::{LogitsProcessor, Sampling, Trainer};
use std::sync::RwLock;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use tch::Tensor;
use tokenizers::Tokenizer;
use tokio_util::sync::CancellationToken;

const MAX_CONTEXT_LENGTH: usize = 1000;

#[derive(Debug)]
pub struct PromptTask {
    task: String,
    tokens: RwLock<Vec<i64>>,
    next_index: Arc<AtomicUsize>,
}

impl PromptTask {
    pub fn new(task: String, tokenizer: &Tokenizer) -> Self {
        let encoding = tokenizer.encode(task.clone(), true).unwrap();
        let tokens = encoding.get_ids().iter().map(|x| *x as i64).collect();
        Self {
            task,
            tokens: RwLock::new(tokens),
            next_index: Arc::new(AtomicUsize::new(0)),
        }
    }

    pub fn next_index(&self) -> &Arc<AtomicUsize> {
        &self.next_index
    }
}

#[derive(Debug)]
pub struct PromptResult {
    pub tokens: Vec<i64>,
    pub next_token: u32,
    pub cancelled: bool,
}

impl PromptTask {
    pub fn run(
        &self,
        trainer: &mut Trainer,
        cancel: CancellationToken,
        skip_and_step_by: Option<(usize, usize)>,
        limit: Option<usize>,
        loop_if_empty: bool,
    ) {
        for x in 0..11 {
            if cancel.is_cancelled() {
                // cancelled = true;
                break;
            }

            // Create input tensor
            let device = trainer.device();

            // Read tokens for creating input
            let tokens_snapshot: Vec<i64> = {
                let tokens = self.tokens.read().unwrap();
                if tokens.len() > MAX_CONTEXT_LENGTH {
                    tokens[tokens.len() - MAX_CONTEXT_LENGTH..].to_vec()
                } else {
                    tokens.clone()
                }
            };
            let input = Tensor::from_slice(&tokens_snapshot)
                .to(device.clone())
                .unsqueeze(0);

            // Run forward pass
            let (logits, _) = trainer.forward(&input, None, Some(1));

            let logits = logits.squeeze();

            // Sample next token
            let mut logits_processor =
                LogitsProcessor::from_sampling(rand::random(), Sampling::All { temperature: 0.6 });

            let next_token = match logits_processor.sample(&logits) {
                Ok(token) => token,
                Err(_) => {
                    panic!("Failed to sample next token");
                }
            };

            self.tokens.write().unwrap().push(next_token as i64);

            // println!("Prompt Tokens: {:?}", &self.tokens);
            println!("Prompt Tokens");
        }
    }
}
