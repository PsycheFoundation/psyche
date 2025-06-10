use psyche_modeling::CausalLM;
use psyche_modeling::{LogitsProcessor, Sampling, Trainer};
use tch::Tensor;
use tokenizers::Tokenizer;
use tokio_util::sync::CancellationToken;

#[derive(Debug)]
pub struct PromptTask {
    task: String,
    tokens: Vec<i64>,
}

impl PromptTask {
    pub fn new(task: String, tokenizer: &Tokenizer) -> Self {
        let encoding = tokenizer.encode(task.clone(), true).unwrap();
        let tokens = encoding.get_ids().iter().map(|x| *x as i64).collect();
        Self { task, tokens }
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
    ) -> PromptResult {
        // Check if cancelled before starting
        if cancel.is_cancelled() {
            return PromptResult {
                tokens: vec![],
                next_token: 0,
                cancelled: true,
            };
        }

        // Create input tensor
        let device = trainer.device();
        let input = Tensor::from_slice(&self.tokens)
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
                return PromptResult {
                    tokens: self.tokens.clone(),
                    next_token: 0,
                    cancelled: false,
                };
            }
        };

        PromptResult {
            tokens: self.tokens.clone(),
            next_token,
            cancelled: false,
        }
    }
}
