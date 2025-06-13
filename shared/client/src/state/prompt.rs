use psyche_core::FixedVec;
use psyche_modeling::CausalLM;
use psyche_modeling::{LogitsProcessor, Sampling, Trainer};
use std::sync::RwLock;
use std::sync::{atomic::AtomicUsize, Arc};
use tch::Tensor;
use tokenizers::Tokenizer;
use tokio_util::sync::CancellationToken;

const MAX_CONTEXT_LENGTH: usize = 1000;

#[derive(Debug)]
pub struct PromptTask {
    task: String,
    pub tokens: RwLock<Vec<i32>>,
    pub tokens_to_send: RwLock<FixedVec<i32, 8>>,
    next_index: Arc<AtomicUsize>,
    in_use: RwLock<bool>,
}

impl PromptTask {
    pub fn new(task: String, tokenizer: &Tokenizer) -> Self {
        let encoding = tokenizer.encode(task.clone(), true).unwrap();
        let tokens = encoding.get_ids().iter().map(|x| *x as i32).collect();
        Self {
            task,
            tokens: RwLock::new(tokens),
            tokens_to_send: RwLock::new(FixedVec::new()),
            next_index: Arc::new(AtomicUsize::new(0)),
            in_use: RwLock::new(false),
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
        tracing::info!("PromptTask Run");
        if self.tokens_to_send.read().unwrap().is_full() {
            tracing::info!("Prompt Buffer Full");
            return;
        }
        tracing::info!("PromptTask::run");
        if cancel.is_cancelled() {
            tracing::info!("Prompt cancelled");
            return;
        }

        // let mut in_use = self.in_use.write().unwrap();
        // *in_use = true;

        // Read tokens for creating input
        let token_len = self.tokens.read().unwrap().len();
        if token_len > MAX_CONTEXT_LENGTH {
            self.tokens.write().unwrap().drain(..MAX_CONTEXT_LENGTH / 2);
        }

        let tokens = self.tokens.read().unwrap();

        let input = Tensor::from_slice(&tokens)
            .to(*trainer.device())
            .unsqueeze(0);

        // drop tokens to release lock
        drop(tokens);
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

        self.tokens_to_send
            .write()
            .unwrap()
            .push(next_token as i32)
            .unwrap();
        self.tokens.write().unwrap().push(next_token as i32);

        // *in_use = false;
        tracing::info!("Prompt Next token: {}", next_token);
    }
}
