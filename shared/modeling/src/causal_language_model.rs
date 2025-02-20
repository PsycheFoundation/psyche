use crate::{
    AttentionImplementation, Communicator, CommunicatorId, ModelConfig, ModelLoadError,
    PretrainedSource, RoPEConfig,
};
use std::fmt::Debug;
use std::sync::Arc;
use tch::{
    nn::{self, Module, VarStore},
    Device, Kind, Tensor,
};

#[cfg(feature = "parallelism")]
use tch::CNCCL;

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
#[serde(untagged)]
pub enum EosToks {
    Single(i64),
    Multiple(Vec<i64>),
}

/// This trait is for any Causal Language Model that can be inferred,
/// and thus can have backprop run on it.
/// Its internal implementation is completely hidden, so this can be impl'd
/// for a wrapper struct that does something like data parallelism.
pub trait CausalLM: Send + std::fmt::Debug {
    fn forward(
        &mut self,
        x: &Tensor,
        labels: Option<&Tensor>,
        num_logits_to_keep: Option<i64>,
    ) -> (Tensor, Option<Tensor>);
    fn bos_token_id(&self) -> Option<i64>;
    fn eos_token_ids(&self) -> Option<EosToks>;
    fn device(&self) -> Device;
    fn variables(&self) -> &VarStore;
    fn communicator(&self) -> Option<Arc<Communicator>>;
}

pub trait LanguageModelForward: Send + Debug {
    fn forward(&self, x: &Tensor, index_pos: i64) -> Tensor;
}

pub trait LanguageModelConfig: ModelConfig + Send + Debug + serde::de::DeserializeOwned {
    fn tie_word_embeddings(&self) -> bool;
    fn set_max_position_embeddings(&mut self, set: usize);
    fn hidden_size(&self) -> usize;
    fn vocab_size(&self) -> usize;

    fn rope_config(&self) -> Option<RoPEConfig>;
    fn num_attention_heads(&self) -> usize;
    fn rope_theta(&self) -> f32;
    fn max_position_embeddings(&self) -> usize;
    fn bos_token_id(&self) -> Option<i64>;
    fn eos_token_ids(&self) -> Option<EosToks>;
}

#[derive(Debug)]
pub struct CausalLanguageModel<M: LanguageModelForward, C: LanguageModelConfig> {
    pub model: M,
    pub config: C,
    pub variables: VarStore,
    pub device: Device,
    pub lm_head: nn::Linear,
    pub comm: Option<Arc<Communicator>>,
}

// this is absolutely unsafe, if you use it across threads with NCCL you will have a bad day
unsafe impl<M: LanguageModelForward, C: LanguageModelConfig> Send for CausalLanguageModel<M, C> {}

pub type LanguageModelBuilder<M, C> = fn(
    vs: nn::Path,
    config: &C,
    attn_implementation: Option<AttentionImplementation>,
    comm: Option<Arc<Communicator>>,
) -> Result<M, ModelLoadError>;

impl<M: LanguageModelForward, C: LanguageModelConfig> CausalLanguageModel<M, C> {
    pub fn from_builder(
        builder: LanguageModelBuilder<M, C>,
        source: &PretrainedSource<C>,
        kind: Option<Kind>,
        attn_implementation: Option<AttentionImplementation>,
        device: Option<Device>,
        tensor_parallelism_world: Option<(Arc<CommunicatorId>, usize, usize)>,
        override_max_position_embeddings: Option<usize>,
    ) -> Result<Self, ModelLoadError> {
        let mut config = source.get_config()?;

        if config.tie_word_embeddings() {
            return Err(ModelLoadError::ModelHasTiedEmbeddings);
        }

        if let Some(override_max_position_embeddings) = override_max_position_embeddings {
            config.set_max_position_embeddings(override_max_position_embeddings);
        }

        let device = device.unwrap_or(Device::cuda_if_available());
        #[cfg(feature = "parallelism")]
        let comm = match tensor_parallelism_world {
            // TODO: CNCCL is not Sync, though it is Send.
            // since we can't safely use it on two threads at once,
            // we should either wrap it in a Mutex, or just switch to Rc if we don't need mutability.
            #[allow(clippy::arc_with_non_send_sync)]
            Some((id, rank, world_size)) => Some(Arc::new(
                CNCCL::new(id, rank as i64, world_size as i64, device)
                    .map_err(ModelLoadError::TensorParallelismFailedInit)?,
            )),
            None => None,
        };

        #[cfg(not(feature = "parallelism"))]
        let comm = match tensor_parallelism_world {
            Some(_) => return Err(ModelLoadError::TensorParallelismNotEnabled),
            None => None,
        };
        let mut variables: nn::VarStore = nn::VarStore::new(device);
        if let Some(kind) = kind {
            variables.set_kind(kind);
        }
        let (model, lm_head) = {
            let _no_grad = tch::no_grad_guard();
            let model = builder(variables.root(), &config, attn_implementation, comm.clone())?;
            let c = nn::LinearConfig {
                bias: false,
                ..Default::default()
            };
            let lm_head = nn::linear(
                &variables.root() / "lm_head",
                config.hidden_size() as i64,
                config.vocab_size() as i64,
                c,
            );

            source.load(&mut variables)?;

            (model, lm_head)
        };
        Ok(Self {
            model,
            config,
            variables,
            device,
            lm_head,
            comm,
        })
    }
}

impl<M: LanguageModelForward, C: LanguageModelConfig> CausalLM for CausalLanguageModel<M, C> {
    fn forward(
        &mut self,
        x: &Tensor,
        labels: Option<&Tensor>,
        num_logits_to_keep: Option<i64>,
    ) -> (Tensor, Option<Tensor>) {
        let (_, t) = x.size2().unwrap();
        let mut x = self.model.forward(x, 0);
        if let Some(num_logits_to_keep) = num_logits_to_keep {
            // Only compute necessary logits, and do not upcast them to float if we are not computing the loss
            x = x.slice(1, t - num_logits_to_keep, t, 1);
        }
        let mut logits = self.lm_head.forward(&x);
        let loss = match labels {
            Some(labels) => {
                // Upcast to float if we need to compute the loss to avoid potential precision issues
                logits = logits.to_kind(Kind::Float);
                // Shift so that tokens < n predict n
                let shift_logits = logits.slice(1, 0, -1, 1).contiguous();
                let shift_labels = labels.slice(1, 1, None, 1).contiguous();
                let shift_logits = shift_logits.view([-1i64, self.config.vocab_size() as i64]);
                let shift_targets = shift_labels.view(-1).to_kind(Kind::Int64);
                let loss = shift_logits.cross_entropy_loss::<Tensor>(
                    &shift_targets,
                    None,
                    tch::Reduction::Mean,
                    -100,
                    0.0,
                );
                Some(loss)
            }
            None => None,
        };
        (logits, loss)
    }

    fn bos_token_id(&self) -> Option<i64> {
        self.config.bos_token_id()
    }

    fn eos_token_ids(&self) -> Option<EosToks> {
        self.config.eos_token_ids()
    }

    fn device(&self) -> Device {
        self.device
    }

    fn variables(&self) -> &VarStore {
        &self.variables
    }

    fn communicator(&self) -> Option<Arc<Communicator>> {
        self.comm.clone()
    }
}
