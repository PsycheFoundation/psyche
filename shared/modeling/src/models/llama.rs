use crate::{
    AttentionImplementation, AutoConfig, CausalLanguageModel, CausalSelfAttention,
    ColumnParallelLinear, CommunicatorId, EosToks, LanguageModelConfig, LanguageModelForward,
    ModelLoadError, PretrainedSource, RMSNorm, RoPECache, RoPEConfig, RowParallelLinear,
    default_rope, parallelism::Communicator,
};
use std::sync::Arc;
use tch::{
    Device, Kind, Tensor,
    nn::{self, Module},
};

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct LlamaConfig {
    pub hidden_size: usize,
    pub intermediate_size: usize,
    pub vocab_size: usize,
    pub num_hidden_layers: usize,
    pub num_attention_heads: usize,
    pub num_key_value_heads: Option<usize>,
    pub rms_norm_eps: f64,
    #[serde(default = "default_rope")]
    pub rope_theta: f32,
    pub bos_token_id: Option<i64>,
    pub eos_token_id: Option<EosToks>,
    pub rope_scaling: Option<RoPEConfig>,
    pub max_position_embeddings: usize,
    pub tie_word_embeddings: bool,
    pub attention_bias: Option<bool>,
}

impl LlamaConfig {
    pub fn num_key_value_heads(&self) -> usize {
        self.num_key_value_heads.unwrap_or(self.num_attention_heads)
    }

    pub fn dummy() -> Self {
        Self {
            hidden_size: 1,
            intermediate_size: 1,
            vocab_size: 1,
            num_hidden_layers: 1,
            num_attention_heads: 1,
            num_key_value_heads: Some(1),
            rms_norm_eps: 0.00001,
            rope_theta: 10000.0,
            bos_token_id: Some(1),
            eos_token_id: Some(EosToks::Single(1)),
            rope_scaling: None,
            max_position_embeddings: 2048,
            tie_word_embeddings: false,
            attention_bias: None,
        }
    }
}

#[derive(Debug)]
struct Mlp {
    gate_proj: ColumnParallelLinear,
    up_proj: ColumnParallelLinear,
    down_proj: RowParallelLinear,
}

impl Mlp {
    fn new(vs: nn::Path, n_embd: i64, n_hidden: i64, comm: Option<Arc<Communicator>>) -> Self {
        let tp_size = comm.as_ref().map(|x| x.size()).unwrap_or(1);
        assert_eq!(
            n_hidden % tp_size,
            0,
            "n_hidden must be divisible by tp_size"
        );

        let gate_proj = ColumnParallelLinear::new(
            &vs / "gate_proj",
            n_embd,
            n_hidden,
            false,
            false,
            comm.clone(),
        );
        let up_proj = ColumnParallelLinear::new(
            &vs / "up_proj",
            n_embd,
            n_hidden,
            false,
            false,
            comm.clone(),
        );
        let down_proj =
            RowParallelLinear::new(&vs / "down_proj", n_hidden, n_embd, false, true, comm);
        Self {
            gate_proj,
            up_proj,
            down_proj,
        }
    }
}

impl Module for Mlp {
    fn forward(&self, xs: &Tensor) -> Tensor {
        self.down_proj
            .forward(&(self.gate_proj.forward(xs).silu() * self.up_proj.forward(xs)))
    }
}

#[derive(Debug)]
struct Block {
    rms_1: RMSNorm,
    attn: CausalSelfAttention,
    rms_2: RMSNorm,
    mlp: Mlp,
}

impl Block {
    fn new(
        vs: nn::Path,
        config: &LlamaConfig,
        attn_implementation: AttentionImplementation,
        comm: Option<Arc<Communicator>>,
    ) -> Self {
        let rms_1 = RMSNorm::new(
            &vs / "input_layernorm",
            config.hidden_size as i64,
            config.rms_norm_eps,
        );
        let attn = CausalSelfAttention::new(
            &vs / "self_attn",
            config.num_attention_heads as i64,
            config
                .num_key_value_heads
                .unwrap_or(config.num_attention_heads) as i64,
            config.hidden_size as i64,
            (config.max_position_embeddings + 1) as i64,
            attn_implementation,
            comm.clone(),
        );
        let rms_2 = RMSNorm::new(
            &vs / "post_attention_layernorm",
            config.hidden_size as i64,
            config.rms_norm_eps,
        );
        let mlp = Mlp::new(
            &vs / "mlp",
            config.hidden_size as i64,
            config.intermediate_size as i64,
            comm,
        );
        Self {
            rms_1,
            attn,
            rms_2,
            mlp,
        }
    }

    fn forward(
        &self,
        x: &Tensor,
        position_ids: Option<&Tensor>,
        sequence_lengths: Option<&(Tensor, i32)>,
        cache: &RoPECache,
    ) -> Tensor {
        let x = self.attn.forward(
            &self.rms_1.forward(x),
            position_ids,
            sequence_lengths,
            cache,
        ) + x;
        self.mlp.forward(&self.rms_2.forward(&x)) + x
    }
}

#[allow(dead_code)]
#[derive(Debug)]
pub struct Llama {
    wte: nn::Embedding,
    blocks: Vec<Block>,
    ln_f: RMSNorm,
    attn_implementation: AttentionImplementation,
    rope_cache: RoPECache,
}

impl Llama {
    pub fn new(
        vs: nn::Path,
        config: &LlamaConfig,
        attn_implementation: AttentionImplementation,
        comm: Option<Arc<Communicator>>,
    ) -> Self {
        let wte = nn::embedding(
            &vs / "model" / "embed_tokens",
            config.vocab_size as i64,
            config.hidden_size as i64,
            Default::default(),
        );
        let ln_f = RMSNorm::new(
            &vs / "model" / "norm",
            config.hidden_size as i64,
            config.rms_norm_eps,
        );
        let blocks = (0..config.num_hidden_layers)
            .map(|i| {
                Block::new(
                    &vs / "model" / "layers" / i,
                    config,
                    attn_implementation,
                    comm.clone(),
                )
            })
            .collect::<Vec<_>>();
        let rope_cache = RoPECache::new(
            &config.rope_config(),
            config.hidden_size() / config.num_attention_heads(),
            config.rope_theta(),
            &vs.device(),
        );
        Self {
            wte,
            blocks,
            ln_f,
            attn_implementation,
            rope_cache,
        }
    }
}

impl LanguageModelForward for Llama {
    #[allow(unused_variables)]
    fn forward(
        &self,
        x: &Tensor,
        position_ids: Option<&Tensor>,
        sequence_lengths: Option<&Vec<Vec<i32>>>,
        _training: bool,
    ) -> Tensor {
        let sequence_lengths = sequence_lengths.map(|sequence_lengths| {
            #[cfg(feature = "parallelism")]
            {
                if self.attn_implementation == AttentionImplementation::FlashAttention2 {
                    crate::attention::create_cu_seqlens(sequence_lengths, x.device())
                } else {
                    panic!("`sequence_lengths` only supported for FlashAttention2");
                }
            }

            #[cfg(not(feature = "parallelism"))]
            {
                panic!("`sequence_lengths` only supported for FlashAttention2");
            }
        });

        let mut x = self.wte.forward(x);
        for block in &self.blocks {
            x = block.forward(
                &x,
                position_ids,
                sequence_lengths.as_ref(),
                &self.rope_cache,
            );
        }
        self.ln_f.forward(&x)
    }
}

pub type LlamaForCausalLM = CausalLanguageModel<Llama, LlamaConfig>;

impl LlamaForCausalLM {
    fn builder(
        vs: nn::Path,
        config: &LlamaConfig,
        attn_implementation: Option<AttentionImplementation>,
        comm: Option<Arc<Communicator>>,
    ) -> Result<Llama, ModelLoadError> {
        Ok(Llama::new(
            vs,
            config,
            attn_implementation.unwrap_or_default(),
            comm,
        ))
    }

    pub fn from_pretrained(
        source: &PretrainedSource<LlamaConfig>,
        kind: Option<Kind>,
        attn_implementation: Option<AttentionImplementation>,
        device: Option<Device>,
        tensor_parallelism_world: Option<(CommunicatorId, usize, usize)>,
        override_max_position_embeddings: Option<usize>,
    ) -> Result<Self, ModelLoadError> {
        Self::from_builder(
            Self::builder,
            source,
            kind,
            attn_implementation,
            device,
            tensor_parallelism_world,
            override_max_position_embeddings,
        )
    }
}

impl TryFrom<AutoConfig> for LlamaConfig {
    type Error = ModelLoadError;

    fn try_from(value: AutoConfig) -> Result<Self, Self::Error> {
        match value {
            AutoConfig::Llama(llama_config) => Ok(llama_config),
            _ => Err(ModelLoadError::WrongConfigType),
        }
    }
}

impl TryFrom<PretrainedSource<AutoConfig>> for PretrainedSource<LlamaConfig> {
    type Error = ModelLoadError;

    fn try_from(value: PretrainedSource<AutoConfig>) -> Result<Self, Self::Error> {
        match value {
            PretrainedSource::RepoFiles(path_bufs) => Ok(PretrainedSource::RepoFiles(path_bufs)),
            PretrainedSource::ConfigAndTensors(AutoConfig::Llama(config), hash_map) => {
                Ok(PretrainedSource::ConfigAndTensors(config, hash_map))
            }
            _ => Err(ModelLoadError::WrongConfigType),
        }
    }
}

impl LanguageModelConfig for LlamaConfig {
    fn tie_word_embeddings(&self) -> bool {
        self.tie_word_embeddings
    }

    fn set_max_position_embeddings(&mut self, set: usize) {
        self.max_position_embeddings = set;
    }

    fn hidden_size(&self) -> usize {
        self.hidden_size
    }

    fn vocab_size(&self) -> usize {
        self.vocab_size
    }

    fn rope_config(&self) -> Option<RoPEConfig> {
        self.rope_scaling.clone()
    }

    fn num_attention_heads(&self) -> usize {
        self.num_attention_heads
    }

    fn rope_theta(&self) -> f32 {
        self.rope_theta
    }

    fn max_position_embeddings(&self) -> usize {
        self.max_position_embeddings
    }

    fn bos_token_id(&self) -> Option<i64> {
        self.bos_token_id
    }

    fn eos_token_ids(&self) -> Option<EosToks> {
        self.eos_token_id.clone()
    }
}
