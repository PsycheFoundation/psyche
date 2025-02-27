use crate::{
    auto_config::UseSDPA, rotate_half, yarn_get_mscale, AttentionImplementation, AutoConfig,
    CausalLanguageModel, ColumnParallelLinear, Communicator, CommunicatorId, EosToks,
    LanguageModelConfig, LanguageModelForward, ModelConfig, ModelLoadError, PretrainedSource,
    RMSNorm, RoPECache, RoPEConfig, RoPEType, RowParallelLinear,
};
use std::fmt::Debug;
use std::sync::Arc;
use tch::{
    nn::{
        self,
        init::{FanInOut, NonLinearity, NormalOrUniform},
        Init, Module,
    },
    Device, Kind, Tensor,
};
use torch_sys::IntList;

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub enum ScoringFunc {
    #[serde(rename = "sigmoid")]
    Sigmoid,
    #[serde(rename = "softmax")]
    Softmax,
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize, PartialEq)]
pub enum TopKMethod {
    #[serde(rename = "noaux_tc")]
    NoAuxTC,
    #[serde(rename = "greedy")]
    Greedy,
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct DeepseekConfig {
    pub hidden_size: usize,
    pub intermediate_size: usize,
    pub vocab_size: usize,
    pub num_hidden_layers: usize,
    pub num_attention_heads: usize,
    pub rms_norm_eps: f64,
    pub rope_theta: f32,
    pub max_position_embeddings: usize,
    pub tie_word_embeddings: bool,
    pub bos_token_id: Option<i64>,
    pub eos_token_id: Option<EosToks>,
    pub rope_scaling: Option<RoPEConfig>,
    // MLA
    pub q_lora_rank: Option<usize>,
    pub kv_lora_rank: Option<usize>,
    pub qk_nope_head_dim: Option<usize>,
    pub qk_rope_head_dim: Option<usize>,
    pub v_head_dim: Option<usize>,
    pub attention_bias: Option<bool>,
    // MoE
    pub n_routed_experts: Option<usize>,
    pub num_experts_per_tok: Option<usize>,
    pub moe_intermediate_size: Option<usize>,
    pub routed_scaling_factor: Option<f32>,
    pub n_group: Option<usize>,
    pub topk_group: Option<usize>,
    pub n_shared_experts: Option<usize>,
    pub first_k_dense_replace: Option<usize>,
    pub moe_layer_freq: Option<usize>,
    pub scoring_func: Option<ScoringFunc>,
    pub topk_method: Option<TopKMethod>,
    pub norm_topk_prob: Option<bool>,
}

pub fn apply_rotary_pos_emb(
    q: &Tensor,
    k: &Tensor,
    index_pos: i64,
    cache: &RoPECache,
) -> (Tensor, Tensor) {
    let (_b_sz, _, seq_len, _hidden_size) = q.size4().unwrap();
    let cos = cache.cos.narrow(0, index_pos, seq_len);
    let sin = cache.sin.narrow(0, index_pos, seq_len);
    let cos = Tensor::cat(&[&cos, &cos], -1);
    let sin = Tensor::cat(&[&sin, &sin], -1);
    let cos = cos.unsqueeze(0).unsqueeze(0);
    let sin = sin.unsqueeze(0).unsqueeze(0);

    let (b, h, s, d) = q.size4().unwrap();
    let q = q
        .view([b, h, s, d / 2, 2])
        .transpose(4, 3)
        .reshape([b, h, s, d]);

    let (b, h, s, d) = k.size4().unwrap();
    let k = k
        .view([b, h, s, d / 2, 2])
        .transpose(4, 3)
        .reshape([b, h, s, d]);

    let q_embed = (&q * &cos) + (rotate_half(&q) * &sin);
    let k_embed = (&k * &cos) + (rotate_half(&k) * &sin);

    (q_embed, k_embed)
}

#[derive(Debug)]
struct MLAAttention {
    q_a_proj: Option<ColumnParallelLinear>,
    q_a_layernorm: Option<RMSNorm>,
    q_b_proj: Option<ColumnParallelLinear>,
    q_proj: Option<ColumnParallelLinear>,

    kv_a_proj_with_mqa: ColumnParallelLinear,
    kv_a_layernorm: RMSNorm,
    kv_b_proj: ColumnParallelLinear,

    o_proj: RowParallelLinear,

    kv_lora_rank: i64,
    head_v_dim: i64,
    qk_rope_head_dim: i64,
    qk_nope_head_dim: i64,
    softmax_scale: f64,
    device: Device,
    use_sdpa: bool,
    num_local_heads: i64,
}

impl MLAAttention {
    fn new(
        vs: nn::Path,
        config: &DeepseekConfig,
        use_sdpa: bool,
        comm: Option<Arc<Communicator>>,
    ) -> Self {
        let tp_size = comm.as_ref().map(|x| x.size()).unwrap_or(1);
        let num_heads = config.num_attention_heads as i64;
        assert_eq!(
            num_heads % tp_size,
            0,
            "n_head must be divisible by tp_size"
        );
        let num_local_heads = num_heads / tp_size;
        let qk_rope_head_dim = config.qk_rope_head_dim.unwrap() as i64;
        let qk_nope_head_dim = config.qk_nope_head_dim.unwrap() as i64;
        let v_head_dim = config.v_head_dim.unwrap() as i64;
        let q_head_dim = qk_nope_head_dim + qk_rope_head_dim;
        let hidden_size = config.hidden_size as i64;
        let kv_lora_rank = config.kv_lora_rank.unwrap() as i64;
        let attention_bias = config.attention_bias.unwrap();

        let (q_a_proj, q_a_layernorm, q_b_proj, q_proj) = match config.q_lora_rank {
            Some(q_lora_rank) => {
                let q_a_proj = ColumnParallelLinear::new(
                    &vs / "q_a_proj",
                    hidden_size,
                    q_lora_rank as i64,
                    attention_bias,
                    false,
                    None, // explicitly NOT parallel
                );

                let q_a_layernorm = RMSNorm::new(
                    &vs / "q_a_layernorm",
                    q_lora_rank as i64,
                    config.rms_norm_eps,
                );

                let q_b_proj = ColumnParallelLinear::new(
                    &vs / "q_b_proj",
                    q_lora_rank as i64,
                    num_heads * q_head_dim,
                    false,
                    false,
                    comm.clone(),
                );

                (Some(q_a_proj), Some(q_a_layernorm), Some(q_b_proj), None)
            }
            None => {
                let q_proj = ColumnParallelLinear::new(
                    &vs / "q_proj",
                    hidden_size,
                    num_heads * q_head_dim,
                    attention_bias,
                    false,
                    comm.clone(),
                );

                (None, None, None, Some(q_proj))
            }
        };

        let kv_a_proj_with_mqa = ColumnParallelLinear::new(
            &vs / "kv_a_proj_with_mqa",
            hidden_size,
            kv_lora_rank + qk_rope_head_dim,
            attention_bias,
            false,
            None, // explicitly NOT parallel
        );

        let kv_a_layernorm =
            RMSNorm::new(&vs / "kv_a_layernorm", kv_lora_rank, config.rms_norm_eps);

        let kv_b_proj = ColumnParallelLinear::new(
            &vs / "kv_b_proj",
            kv_lora_rank,
            num_heads * (q_head_dim - qk_rope_head_dim + v_head_dim),
            false,
            false,
            comm.clone(),
        );

        let o_proj = RowParallelLinear::new(
            &vs / "o_proj",
            num_heads * v_head_dim,
            hidden_size,
            attention_bias,
            true,
            comm,
        );

        let mut softmax_scale = 1.0 / (q_head_dim as f64).sqrt();
        if let Some(rope_scaling) = &config.rope_scaling {
            if rope_scaling.rope_type == RoPEType::YaRN {
                if let Some(mscale_all_dim) = rope_scaling.mscale_all_dim {
                    let mscale =
                        yarn_get_mscale(rope_scaling.factor.unwrap(), mscale_all_dim) as f64;
                    softmax_scale *= mscale * mscale;
                }
            }
        }

        Self {
            q_a_proj,
            q_a_layernorm,
            q_b_proj,
            q_proj,
            kv_a_proj_with_mqa,
            kv_a_layernorm,
            kv_b_proj,
            o_proj,
            head_v_dim: v_head_dim,
            qk_rope_head_dim,
            qk_nope_head_dim,
            kv_lora_rank,
            softmax_scale,
            device: vs.device(),
            use_sdpa,
            num_local_heads,
        }
    }

    fn split_with_sizes_2(tensor: Tensor, split_sizes: impl IntList, dim: i64) -> (Tensor, Tensor) {
        let mut tensors = tensor.split_with_sizes(split_sizes, dim);
        let b = tensors.pop().unwrap();
        let a = tensors.pop().unwrap();
        (a, b)
    }

    fn forward(&self, x: &Tensor, index_pos: i64, cache: &RoPECache) -> Tensor {
        let (b, t, _) = x.size3().unwrap();
        let kind = x.kind();

        let q = match (
            &self.q_a_proj,
            &self.q_a_layernorm,
            &self.q_b_proj,
            &self.q_proj,
        ) {
            (Some(q_a_proj), Some(q_a_layernorm), Some(q_b_proj), None) => {
                let q_compressed = q_a_proj.forward(x);
                let q_compressed = q_a_layernorm.forward(&q_compressed);
                q_b_proj.forward(&q_compressed)
            }
            (None, None, None, Some(q_proj)) => q_proj.forward(x),
            _ => panic!("Unexpected MLA proj combination"),
        };

        let q = q.view([b, t, self.num_local_heads, -1]).transpose(1, 2);
        let (q_nope, q_pe) =
            Self::split_with_sizes_2(q, [self.qk_nope_head_dim, self.qk_rope_head_dim], -1);

        let compressed_kv = self.kv_a_proj_with_mqa.forward(x);
        let (compressed_kv, k_pe) = Self::split_with_sizes_2(
            compressed_kv,
            [self.kv_lora_rank, self.qk_rope_head_dim],
            -1,
        );
        let compressed_kv = self.kv_a_layernorm.forward(&compressed_kv);
        let kv = self
            .kv_b_proj
            .forward(&compressed_kv)
            .view([
                b,
                t,
                self.num_local_heads,
                self.qk_nope_head_dim + self.head_v_dim,
            ])
            .transpose(1, 2);

        let (k_nope, value_states) =
            Self::split_with_sizes_2(kv, [self.qk_nope_head_dim, self.head_v_dim], -1);

        let k_pe = k_pe
            .view([b, t, 1, self.qk_rope_head_dim])
            // matches the expansion
            //    query_states[:, :, :, : self.qk_nope_head_dim] = q_nope
            //    query_states[:, :, :, self.qk_nope_head_dim :] = q_pe
            .expand([b, t, self.num_local_heads, self.qk_rope_head_dim], false)
            .transpose(1, 2);

        let (q_pe, k_pe) = apply_rotary_pos_emb(&q_pe, &k_pe, index_pos, cache);

        let query_states = Tensor::cat(&[&q_nope, &q_pe], -1);
        let key_states = Tensor::cat(&[&k_nope, &k_pe], -1);

        let y = if self.use_sdpa {
            Tensor::scaled_dot_product_attention::<Tensor>(
                &query_states,
                &key_states,
                &value_states,
                None,
                0.0,
                t > 1,
                Some(self.softmax_scale),
                false,
            )
        } else {
            let att = query_states.matmul(&key_states.transpose(-2, -1)) * self.softmax_scale;
            let mask = Tensor::ones([t, t], (kind, self.device))
                .tril(0)
                .reshape([1, 1, t, t]);
            let att = att.masked_fill(&mask.eq(0.), f64::NEG_INFINITY);
            att.softmax(-1, kind).matmul(&value_states)
        };

        // Project back to hidden size
        let y =
            y.transpose(1, 2)
                .contiguous()
                .reshape([b, t, self.num_local_heads * self.head_v_dim]);

        self.o_proj.forward(&y)
    }
}

#[derive(Debug)]
#[allow(clippy::upper_case_acronyms)]
struct MLP {
    gate_proj: ColumnParallelLinear,
    up_proj: ColumnParallelLinear,
    down_proj: RowParallelLinear,
}

impl MLP {
    fn new(vs: nn::Path, config: &DeepseekConfig, comm: Option<Arc<Communicator>>) -> Self {
        let hidden_size = config.hidden_size as i64;
        let intermediate_size = config.intermediate_size as i64;

        let gate_proj = ColumnParallelLinear::new(
            &vs / "gate_proj",
            hidden_size,
            intermediate_size,
            false,
            false,
            comm.clone(),
        );
        let up_proj = ColumnParallelLinear::new(
            &vs / "up_proj",
            hidden_size,
            intermediate_size,
            false,
            false,
            comm.clone(),
        );
        let down_proj = RowParallelLinear::new(
            &vs / "down_proj",
            intermediate_size,
            hidden_size,
            false,
            true,
            comm,
        );
        Self {
            gate_proj,
            up_proj,
            down_proj,
        }
    }

    fn forward(&self, x: &Tensor) -> Tensor {
        self.down_proj
            .forward(&(self.gate_proj.forward(x).silu() * self.up_proj.forward(x)))
    }
}

#[derive(Debug)]
struct MoEGate {
    weight: Tensor,
    top_k: i64,
    norm_topk_prob: bool,
    n_routed_experts: i64,
    routed_scaling_factor: f64,
    n_group: i64,
    topk_group: i64,
    #[allow(dead_code)]
    e_score_correction_bias: Option<Tensor>,
    scoring_func: ScoringFunc,
    topk_method: TopKMethod,
}

impl MoEGate {
    fn new(vs: nn::Path, config: &DeepseekConfig) -> Self {
        let weight = vs.var(
            "weight",
            &[
                config.n_routed_experts.unwrap() as i64,
                config.hidden_size as i64,
            ],
            Init::Kaiming {
                dist: NormalOrUniform::Uniform,
                fan: FanInOut::FanIn,
                non_linearity: NonLinearity::ReLU,
            },
        );

        let e_score_correction_bias = if Some(TopKMethod::NoAuxTC) == config.topk_method {
            Some(vs.var(
                "e_score_correction_bias",
                &[config.n_routed_experts.unwrap() as i64],
                Init::Const(0.),
            ))
        } else {
            None
        };

        Self {
            weight,
            top_k: config.num_experts_per_tok.unwrap() as i64,
            norm_topk_prob: config.norm_topk_prob.unwrap(),
            n_routed_experts: config.n_routed_experts.unwrap() as i64,
            routed_scaling_factor: config.routed_scaling_factor.unwrap() as f64,
            n_group: config.n_group.unwrap() as i64,
            topk_group: config.topk_group.unwrap() as i64,
            e_score_correction_bias,
            scoring_func: config.scoring_func.clone().unwrap(),
            topk_method: config.topk_method.clone().unwrap(),
        }
    }

    fn forward(&self, hidden_states: &Tensor) -> (Tensor, Tensor) {
        let (bsz, seq_len, _) = hidden_states.size3().unwrap();

        let hidden_states = hidden_states.view([-1, hidden_states.size()[2]]);
        let logits = hidden_states.matmul(&self.weight.transpose(-2, -1));
        let scores = match self.scoring_func {
            ScoringFunc::Sigmoid => logits.sigmoid(),
            ScoringFunc::Softmax => logits.softmax(-1, Kind::Float),
        };

        let (topk_idx, topk_weight) = match self.topk_method {
            TopKMethod::NoAuxTC => {
                // assert not training
                let scores_for_choice = if let Some(bias) = &self.e_score_correction_bias {
                    scores.view([bsz * seq_len, -1]) + bias.unsqueeze(0)
                } else {
                    scores.view([bsz * seq_len, -1])
                };

                let group_scores = scores_for_choice
                    .view([bsz * seq_len, self.n_group, -1])
                    .topk(2, -1, true, true)
                    .0 // values
                    .sum_dim_intlist(-1, false, Kind::Float);

                let group_idx = group_scores.topk(self.topk_group, -1, true, false).1;

                let mut group_mask = Tensor::zeros_like(&group_scores);
                let _ = group_mask.scatter_(-1, &group_idx, &Tensor::ones_like(&group_idx));

                let score_mask = group_mask
                    .unsqueeze(-1)
                    .expand(
                        [
                            bsz * seq_len,
                            self.n_group,
                            self.n_routed_experts / self.n_group,
                        ],
                        true,
                    )
                    .reshape([bsz * seq_len, -1]);

                let tmp_scores = scores_for_choice.masked_fill(&score_mask.eq(0.), 0.);
                let (_, topk_idx) = tmp_scores.topk(self.top_k, -1, true, false);
                let topk_weight = scores.gather(1, &topk_idx, false);
                (topk_idx, topk_weight)
            }
            TopKMethod::Greedy => {
                let (topk_weight, topk_idx) = scores.topk(self.top_k, -1, true, false);
                (topk_idx, topk_weight)
            }
        };

        let topk_weight = if self.top_k > 1 && self.norm_topk_prob {
            let denominator = topk_weight.sum_dim_intlist(-1, true, Kind::Float) + 1e-20;
            topk_weight / denominator
        } else {
            topk_weight
        } * self.routed_scaling_factor;

        (topk_idx, topk_weight)
    }
}

#[derive(Debug)]
struct DeepseekMoE {
    experts: Vec<Option<MLP>>,
    gate: MoEGate,
    shared_experts: Option<MLP>,
    #[allow(unused)]
    ep_size: i64,
    #[allow(unused)]
    experts_per_rank: i64,
    #[allow(unused)]
    ep_rank: i64,
}

impl DeepseekMoE {
    fn new(vs: nn::Path, config: &DeepseekConfig, comm: Option<Arc<Communicator>>) -> Self {
        // let (ep_size, ep_rank) = comm
        //     .as_ref()
        //     .map(|c| (c.size(), c.rank()))
        //     .unwrap_or((1, 0));
        // TODO: EP
        let (ep_size, ep_rank) = (1, 0);

        let experts_per_rank = config.n_routed_experts.unwrap() as i64 / ep_size;

        let experts = (0..config.n_routed_experts.unwrap() as i64)
            .map(|i| {
                if i >= ep_rank * experts_per_rank && i < (ep_rank + 1) * experts_per_rank {
                    Some(MLP::new(
                        &vs / "experts" / i,
                        &DeepseekConfig {
                            intermediate_size: config.moe_intermediate_size.unwrap(),
                            ..config.clone()
                        },
                        comm.clone(),
                    ))
                } else {
                    None
                }
            })
            .collect();

        let shared_experts = config.n_shared_experts.map(|n| {
            MLP::new(
                &vs / "shared_experts",
                &DeepseekConfig {
                    intermediate_size: config.moe_intermediate_size.unwrap() * n,
                    ..config.clone()
                },
                comm.clone(),
            )
        });

        let gate = MoEGate::new(&vs / "gate", config);

        Self {
            experts,
            gate,
            shared_experts,
            ep_size,
            experts_per_rank,
            ep_rank,
        }
    }

    fn forward(&self, hidden_states: &Tensor) -> Tensor {
        let identity = hidden_states;
        let orig_shape = hidden_states.size();

        let (topk_idx, topk_weight) = self.gate.forward(hidden_states);
        let hidden_states = hidden_states.view([-1, hidden_states.size()[2]]);

        let mut cnts = Tensor::zeros(
            [topk_idx.size()[0], self.experts.len() as i64],
            (topk_idx.kind(), topk_idx.device()),
        );
        let _ = cnts.scatter_add_(1, &topk_idx, &Tensor::ones_like(&topk_idx));
        let tokens_per_expert = cnts.sum_dim_intlist(0, false, Kind::Int64);

        let idxs: Tensor = topk_idx.view(-1).argsort(0, false);
        let sorted_tokens =
            hidden_states.index_select(0, &(idxs.divide_scalar_mode(topk_idx.size()[1], "floor")));

        #[cfg(feature = "parallelism")]
        let y = if self.ep_size > 1 {
            self.parallel_expert_computation(&sorted_tokens, &tokens_per_expert)
        } else {
            self.local_expert_computation(&sorted_tokens, &tokens_per_expert)
        };

        #[cfg(not(feature = "parallelism"))]
        let y = self.local_expert_computation(&sorted_tokens, &tokens_per_expert);

        let mut new_x = Tensor::empty_like(&y);
        let _ = new_x.index_copy_(0, &idxs, &y);

        let final_out = new_x
            .view([topk_idx.size()[0], topk_idx.size()[1], orig_shape[2]])
            .to_kind(topk_weight.kind())
            .f_mul(&topk_weight.unsqueeze(-1))
            .unwrap()
            .sum_dim_intlist(1, false, Kind::Float)
            .to_kind(new_x.kind())
            .view(&orig_shape[..]);

        if let Some(shared_experts) = &self.shared_experts {
            final_out + shared_experts.forward(identity)
        } else {
            final_out
        }
    }

    #[cfg(feature = "parallelism")]
    fn parallel_expert_computation(
        &self,
        _sorted_tokens: &Tensor,
        _tokens_per_expert: &Tensor,
    ) -> Tensor {
        unimplemented!("Implement expert-parallel expert computation")
    }

    fn local_expert_computation(
        &self,
        sorted_tokens: &Tensor,
        tokens_per_expert: &Tensor,
    ) -> Tensor {
        let tokens_per_expert: Vec<i64> = tokens_per_expert.try_into().unwrap();

        let mut outputs = Vec::new();
        let mut start_idx = 0;

        for (i, num_tokens) in tokens_per_expert.iter().enumerate() {
            if *num_tokens == 0 {
                continue;
            }

            if let Some(expert) = &self.experts[i] {
                let end_idx = start_idx + num_tokens;
                let tokens = sorted_tokens.narrow(0, start_idx, *num_tokens);
                outputs.push(expert.forward(&tokens));
                start_idx = end_idx;
            }
        }

        Tensor::cat(&outputs, 0)
    }
}

#[derive(Debug)]
enum NetworkBlock {
    #[allow(clippy::upper_case_acronyms)]
    MLP(MLP),
    MoE(DeepseekMoE),
}

impl NetworkBlock {
    fn forward(&self, x: &Tensor) -> Tensor {
        match self {
            NetworkBlock::MLP(mlp) => mlp.forward(x),
            NetworkBlock::MoE(moe) => moe.forward(x),
        }
    }
}

#[derive(Debug)]
struct DeepseekBlock {
    mla: MLAAttention,
    network: NetworkBlock,
    input_layernorm: RMSNorm,
    post_attention_layernorm: RMSNorm,
}

impl DeepseekBlock {
    fn new(
        vs: nn::Path,
        config: &DeepseekConfig,
        layer_idx: usize,
        use_sdpa: bool,
        comm: Option<Arc<Communicator>>,
    ) -> Self {
        let mla = MLAAttention::new(&vs / "self_attn", config, use_sdpa, comm.clone());

        let network = if config.n_routed_experts.is_some()
            && layer_idx >= config.first_k_dense_replace.unwrap()
            && layer_idx % config.moe_layer_freq.unwrap() == 0
        {
            NetworkBlock::MoE(DeepseekMoE::new(&vs / "mlp", config, comm.clone()))
        } else {
            NetworkBlock::MLP(MLP::new(&vs / "mlp", config, comm.clone()))
        };

        let input_layernorm = RMSNorm::new(
            &vs / "input_layernorm",
            config.hidden_size as i64,
            config.rms_norm_eps,
        );
        let post_attention_layernorm = RMSNorm::new(
            &vs / "post_attention_layernorm",
            config.hidden_size as i64,
            config.rms_norm_eps,
        );

        Self {
            mla,
            network,
            input_layernorm,
            post_attention_layernorm,
        }
    }

    fn forward(&self, x: &Tensor, index_pos: i64, cache: &RoPECache) -> Tensor {
        let residual = x;
        let x = self
            .mla
            .forward(&self.input_layernorm.forward(x), index_pos, cache);
        let x = &x + residual;

        let residual = &x;
        let x = self
            .network
            .forward(&self.post_attention_layernorm.forward(&x));
        x + residual
    }
}

#[derive(Debug)]
pub struct Deepseek {
    embed_tokens: nn::Embedding,
    blocks: Vec<DeepseekBlock>,
    norm: RMSNorm,
    rope_cache: RoPECache,
}

impl Deepseek {
    pub fn new(
        vs: nn::Path,
        config: &DeepseekConfig,
        use_sdpa: bool,
        comm: Option<Arc<Communicator>>,
    ) -> Self {
        let embed_tokens = nn::embedding(
            &vs / "model" / "embed_tokens",
            config.vocab_size as i64,
            config.hidden_size as i64,
            Default::default(),
        );

        let blocks = (0..config.num_hidden_layers)
            .map(|i| {
                DeepseekBlock::new(
                    &vs / "model" / "layers" / i,
                    config,
                    i,
                    use_sdpa,
                    comm.clone(),
                )
            })
            .collect();

        let norm = RMSNorm::new(
            &vs / "model" / "norm",
            config.hidden_size as i64,
            config.rms_norm_eps,
        );

        let rope_cache = RoPECache::new(
            vs.kind(),
            &config.rope_config(),
            config.qk_rope_head_dim.unwrap(),
            config.rope_theta(),
            config.max_position_embeddings(),
            &vs.device(),
        );

        Self {
            embed_tokens,
            blocks,
            norm,
            rope_cache,
        }
    }
}

impl LanguageModelForward for Deepseek {
    fn forward(&self, x: &Tensor, index_pos: i64, training: bool) -> Tensor {
        if let NetworkBlock::MoE(_) = &self.blocks[0].network {
            assert!(!training, "DeepseekMoE training not yet supported");
        }
        let mut hidden_states = self.embed_tokens.forward(x);

        for block in &self.blocks {
            hidden_states = block.forward(&hidden_states, index_pos, &self.rope_cache);
        }

        self.norm.forward(&hidden_states)
    }
}

pub type DeepseekForCausalLM = CausalLanguageModel<Deepseek, DeepseekConfig>;

impl DeepseekForCausalLM {
    fn builder(
        vs: nn::Path,
        config: &DeepseekConfig,
        attn_implementation: Option<AttentionImplementation>,
        comm: Option<Arc<Communicator>>,
    ) -> Result<Deepseek, ModelLoadError> {
        Ok(Deepseek::new(
            vs,
            config,
            attn_implementation.use_sdpa()?,
            comm,
        ))
    }

    pub fn from_pretrained(
        source: &PretrainedSource<DeepseekConfig>,
        kind: Option<Kind>,
        attn_implementation: Option<AttentionImplementation>,
        device: Option<Device>,
        tensor_parallelism_world: Option<(Arc<CommunicatorId>, usize, usize)>,
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

impl ModelConfig for DeepseekConfig {
    // TODO: This is just a hacky solution to get the parameter names from the config
    // but it is probably overkill. We should think about a better way to get them
    // to make the p2p requests.
    fn get_parameter_names(&self) -> Vec<String> {
        let mut variables: nn::VarStore = nn::VarStore::new(Device::Cpu);
        variables.set_kind(Kind::BFloat16);
        let _model = Deepseek::new(variables.root(), self, false, None);
        let c = nn::LinearConfig {
            bias: false,
            ..Default::default()
        };

        let _lm_head = nn::linear(
            &variables.root() / "embed_tokens",
            self.hidden_size as i64,
            self.vocab_size as i64,
            c,
        );

        let variables_lock = variables.variables_.lock().unwrap();
        variables_lock.named_variables.keys().cloned().collect()
    }
}

impl TryFrom<AutoConfig> for DeepseekConfig {
    type Error = ModelLoadError;

    fn try_from(value: AutoConfig) -> Result<Self, Self::Error> {
        match value {
            AutoConfig::Deepseek(config) => Ok(config),
            _ => Err(ModelLoadError::WrongConfigType),
        }
    }
}

impl TryFrom<PretrainedSource<AutoConfig>> for PretrainedSource<DeepseekConfig> {
    type Error = ModelLoadError;

    fn try_from(value: PretrainedSource<AutoConfig>) -> Result<Self, Self::Error> {
        match value {
            PretrainedSource::RepoFiles(path_bufs) => Ok(PretrainedSource::RepoFiles(path_bufs)),
            PretrainedSource::ConfigAndTensors(AutoConfig::Deepseek(config), hash_map) => {
                Ok(PretrainedSource::ConfigAndTensors(config, hash_map))
            }
            _ => Err(ModelLoadError::WrongConfigType),
        }
    }
}

impl LanguageModelConfig for DeepseekConfig {
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

    fn eos_token_ids(&self) -> Option<crate::EosToks> {
        self.eos_token_id.clone()
    }
}
