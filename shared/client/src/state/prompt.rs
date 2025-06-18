use psyche_coordinator::TOKENS_TO_SEND_LENGTH;
use psyche_core::FixedVec;
use psyche_modeling::CausalLM;
use psyche_modeling::{LogitsProcessor, Sampling, Trainer};
use std::sync::atomic::Ordering;
use std::sync::RwLock;
use std::sync::{atomic::AtomicUsize, Arc};
use tch::Tensor;
use tokenizers::Tokenizer;
use tokio_util::sync::CancellationToken;

const MAX_CONTEXT_LENGTH: usize = 1000;

#[derive(Debug)]
pub struct PromptTask {
    _selected_promt: usize,
    tokens: RwLock<Vec<i32>>,
    pub tokens_to_send: RwLock<FixedVec<i32, TOKENS_TO_SEND_LENGTH>>,
    pub next_index: Arc<AtomicUsize>,
}

impl PromptTask {
    pub fn new(selected_promt: usize, task: String, tokenizer: &Tokenizer) -> Self {
        let encoding = tokenizer.encode(task.clone(), true).unwrap();
        let tokens = encoding.get_ids().iter().map(|x| *x as i32).collect();
        Self {
            _selected_promt: selected_promt,
            tokens: RwLock::new(tokens),
            tokens_to_send: RwLock::new(FixedVec::new()),
            next_index: Arc::new(AtomicUsize::new(0)),
        }
    }
}

impl PromptTask {
    pub fn run(&self, trainer: &mut Trainer, cancel: CancellationToken) {
        if self.tokens_to_send.read().unwrap().is_full() {
            tracing::info!("Prompt Buffer Full");
            return;
        }
        if cancel.is_cancelled() {
            tracing::info!("Prompt cancelled");
            return;
        }

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

        self.next_index.fetch_add(1, Ordering::SeqCst);
        println!("Prompt next index: {:?}", &self.next_index);

        tracing::info!("Prompt Next token: {:?}", &self.tokens);
    }
}
