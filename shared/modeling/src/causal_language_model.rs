use crate::{
    AllReduce, AttentionImplementation, Communicator, CommunicatorId, ModelLoadError,
    PretrainedSource, ReduceType, RoPEConfig, StableVarStoreIterator, StableVariableIterator,
};
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::{fmt::Debug, sync::atomic::AtomicBool};
use tch::{
    Device, Kind, Tensor,
    nn::{self, Module},
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
pub trait CausalLM: Send {
    // returns (logits, loss)
    fn forward(
        &self,
        x: &Tensor,
        labels: Option<&Tensor>,
        position_ids: Option<&Tensor>,
        sequence_lengths: Option<&Vec<Vec<i32>>>,
        num_logits_to_keep: Option<i64>,
        loss_scale: Option<f64>,
    ) -> (Option<Tensor>, Option<Tensor>);
    fn bos_token_id(&self) -> Option<i64>;
    fn eos_token_ids(&self) -> Option<EosToks>;
    fn device(&self) -> Device;
    fn max_context_length(&self) -> usize;
    fn variables(&self) -> StableVariableIterator;
    fn communicator(&self) -> Option<Arc<Communicator>>;
    fn prepare_for_training(&self);
    fn clip_grad_norm(&self, max_grad_norm: f64);
    fn shutdown(&self) {}
    fn convert(&self, state_dict: Option<HashMap<String, Tensor>>) -> HashMap<String, Tensor>;
}

pub trait LanguageModelForward: Send + Debug {
    fn forward(
        &self,
        x: &Tensor,
        position_ids: Option<&Tensor>,
        sequence_lengths: Option<&Vec<Vec<i32>>>,
        training: bool,
    ) -> Tensor;
}

pub trait LanguageModelConfig:
    serde::Serialize + Clone + Send + Debug + serde::de::DeserializeOwned
{
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
    pub variables: StableVarStoreIterator,
    pub device: Device,
    pub lm_head: nn::Linear,
    pub comm: Option<Arc<Communicator>>,
    pub training: AtomicBool,
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
        tensor_parallelism_world: Option<(CommunicatorId, usize, usize)>,
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
            #[allow(clippy::arc_with_non_send_sync)]
            // TODO: analyze how we're using Arc here, is this right?
            Some((id, rank, world_size)) => Some(Arc::new(
                CNCCL::new(
                    match id {
                        CommunicatorId::NCCL(cstore) => cstore,
                        _ => return Err(ModelLoadError::CommunicatorMismatch),
                    },
                    rank as i64,
                    world_size as i64,
                    device,
                )
                .map_err(ModelLoadError::TensorParallelismFailedInit)?
                .into(),
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
        let variables = StableVarStoreIterator::new(&variables, comm.clone());
        Ok(Self {
            model,
            config,
            variables,
            device,
            lm_head,
            comm,
            training: AtomicBool::new(false),
        })
    }
}

impl<M: LanguageModelForward, C: LanguageModelConfig> CausalLM for CausalLanguageModel<M, C> {
    fn forward(
        &self,
        x: &Tensor,
        labels: Option<&Tensor>,
        position_ids: Option<&Tensor>,
        sequence_lengths: Option<&Vec<Vec<i32>>>,
        num_logits_to_keep: Option<i64>,
        loss_scale: Option<f64>,
    ) -> (Option<Tensor>, Option<Tensor>) {
        let (_, t) = x.size2().unwrap();
        let mut x = self.model.forward(
            x,
            position_ids,
            sequence_lengths,
            self.training.load(Ordering::Relaxed),
        );
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
                let mut loss = shift_logits.cross_entropy_loss::<Tensor>(
                    &shift_targets,
                    None,
                    tch::Reduction::Mean,
                    -100,
                    0.0,
                );
                if let Some(loss_scale) = loss_scale {
                    loss /= loss_scale;
                }
                Some(loss)
            }
            None => None,
        };
        (Some(logits), loss)
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

    fn max_context_length(&self) -> usize {
        self.config.max_position_embeddings()
    }

    fn variables(&self) -> StableVariableIterator {
        Box::new(self.variables.clone())
    }

    fn communicator(&self) -> Option<Arc<Communicator>> {
        self.comm.clone()
    }

    fn prepare_for_training(&self) {
        self.training.store(true, Ordering::Relaxed);
    }

    /// Clips gradient norm, properly handling tensor-parallel parameters.
    ///
    /// For a model with both sharded and replicated parameters, the true L2 norm is:
    /// sqrt(||w_shared||^2 + ||w_replicated||^2) where:
    /// - w_shared are parameters sharded across ranks (like TP linear layers)
    /// - w_replicated are parameters replicated on all ranks (like layernorms)
    ///
    /// For sharded parameters, since each rank has an orthogonal slice of the full parameter:
    /// ||w_shared||^2 = ||w_shared_1||^2 + ||w_shared_2||^2 + ... + ||w_shared_n||^2
    /// where w_shared_i is the shard on rank i. We compute this via all_reduce_sum of local squared norms.
    ///
    /// For replicated parameters:
    /// ||w_replicated||^2 is identical on all ranks, so we compute it locally.
    ///
    /// The orthogonality of sharded parameters across ranks ensures that:
    /// total_norm = sqrt(all_reduce(||w_shared_local||^2) + ||w_replicated||^2)
    /// gives us the correct global L2 norm as if all parameters were on a single device.
    fn clip_grad_norm(&self, max_norm: f64) {
        let mut sharded_norm_sq = Tensor::zeros([], (Kind::Float, self.device));
        let mut replicated_norm_sq = Tensor::zeros([], (Kind::Float, self.device));

        for var in self.variables() {
            let grad = var.logical_tensor().grad();
            if grad.defined() {
                let local_norm = grad.norm();
                let local_norm_sq = &local_norm * &local_norm;

                if var.is_sharded() {
                    sharded_norm_sq += local_norm_sq
                } else {
                    replicated_norm_sq += local_norm_sq
                }
            }
        }

        sharded_norm_sq.all_reduce(&self.comm, ReduceType::Sum);

        let total_norm: f64 = (sharded_norm_sq + replicated_norm_sq)
            .sqrt()
            .try_into()
            .unwrap();

        if total_norm > max_norm {
            let scale = max_norm / (total_norm + 1e-6);
            for var in self.variables() {
                let mut grad = var.logical_tensor().grad();
                if grad.defined() {
                    let _t = grad.g_mul_scalar_(scale);
                }
            }
        }
    }

    fn convert(&self, state_dict: Option<HashMap<String, Tensor>>) -> HashMap<String, Tensor> {
        state_dict.unwrap_or_else(|| {
            self.variables()
                .map(|x| (x.name().to_string(), x.logical_tensor()))
                .collect()
        })
    }
}

impl EosToks {
    pub fn contains(&self, token: i64) -> bool {
        match self {
            EosToks::Single(x) => *x == token,
            EosToks::Multiple(items) => items.contains(&token),
        }
    }
}
