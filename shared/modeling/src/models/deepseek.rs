#![allow(clippy::manual_is_multiple_of)]

use crate::{
    AttentionImplementation, AutoConfig, CausalLanguageModel, ColumnParallelLinear, Communicator,
    CommunicatorId, EosToks, LanguageModelConfig, LanguageModelForward, ModelLoadError,
    ParallelExpandHeads, PretrainedSource, RMSNorm, RoPECache, RoPEConfig, RoPEType,
    RowParallelLinear, rotate_half, yarn_get_mscale,
};
use std::fmt::Debug;
use std::sync::Arc;
use tch::{
    Device, Kind, Tensor,
    nn::{
        self, Init, Module,
        init::{FanInOut, NonLinearity, NormalOrUniform},
    },
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
    cache: &RoPECache,
    q: &Tensor,
    k: &Tensor,
    position_ids: Option<&Tensor>,
) -> (Tensor, Tensor) {
    let (b, h, s, d) = q.size4().unwrap();
    let position_ids = match position_ids {
        Some(ids) => ids,
        None => &Tensor::arange(s, (Kind::Int64, q.device()))
            .unsqueeze(0)
            .expand([b, s], false),
    };
    let pos_shape = position_ids.size();
    assert_eq!(
        pos_shape.len(),
        2,
        "position_ids must be 2D [batch, seq_len]"
    );
    let pos_b = pos_shape[0];
    let pos_s = pos_shape[1];
    assert_eq!(
        pos_s, s,
        "sequence length mismatch between q and position_ids"
    );
    assert!(
        pos_b == 1 || pos_b == b,
        "batch size mismatch between position_ids and q"
    );

    let head_dim_2 = cache.inv_freq.size()[0];
    let inv_freq_expanded = cache
        .inv_freq
        .to_kind(Kind::Float)
        .unsqueeze(0)
        .unsqueeze(-1)
        .expand([pos_b, head_dim_2, 1], true);
    let position_ids_expanded = position_ids.to_kind(Kind::Float).unsqueeze(1); // [pos_b, 1, seq_len]

    let freqs = inv_freq_expanded.matmul(&position_ids_expanded); // [pos_b, head_dim_2, seq_len]
    let freqs = freqs.transpose(1, 2); // [pos_b, seq_len, head_dim_2]

    let emb = Tensor::cat(&[&freqs, &freqs], -1); // [pos_b, seq_len, head_dim]

    let mut cos = emb.cos();
    let mut sin = emb.sin();

    if cache.mscale != 1.0 {
        let _ = cos.g_mul_scalar_(cache.mscale);
        let _ = sin.g_mul_scalar_(cache.mscale);
    }

    let cos = cos.unsqueeze(1); // [pos_b, 1, seq_len, head_dim]
    let sin = sin.unsqueeze(1); // [pos_b, 1, seq_len, head_dim]

    let kind = q.kind();
    let cos = cos.to_kind(kind);
    let sin = sin.to_kind(kind);

    let q = q
        .view([b, h, s, d / 2, 2])
        .transpose(4, 3)
        .reshape([b, h, s, d]);

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
    attn_implementation: AttentionImplementation,
    num_heads: i64,
    num_local_heads: i64,

    comm: Option<Arc<Communicator>>,
}

unsafe impl Send for MLAAttention {}

impl MLAAttention {
    fn new(
        vs: nn::Path,
        config: &DeepseekConfig,
        attn_implementation: AttentionImplementation,
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
            comm.clone(),
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
            attn_implementation,
            num_heads,
            num_local_heads,
            comm,
        }
    }

    fn split_with_sizes_2(tensor: Tensor, split_sizes: impl IntList, dim: i64) -> (Tensor, Tensor) {
        let mut tensors = tensor.split_with_sizes(split_sizes, dim);
        let b = tensors.pop().unwrap();
        let a = tensors.pop().unwrap();
        (a, b)
    }

    #[allow(unused_mut)]
    #[allow(unused_variables)]
    fn forward(
        &self,
        x: &Tensor,
        position_ids: Option<&Tensor>,
        sequence_lengths: Option<&(Tensor, i32)>,
        cache: &RoPECache,
    ) -> Tensor {
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
            .parallel_expand_heads(&self.comm, [b, t, self.num_heads, self.qk_rope_head_dim])
            .transpose(1, 2);

        let (q_pe, k_pe) = apply_rotary_pos_emb(cache, &q_pe, &k_pe, position_ids);

        let mut query_states = Tensor::cat(&[&q_nope, &q_pe], -1);
        let mut key_states = Tensor::cat(&[&k_nope, &k_pe], -1);

        let y = match self.attn_implementation {
            #[cfg(feature = "parallelism")]
            AttentionImplementation::FlashAttention2 => {
                let mut padded_value_states = value_states.shallow_clone();
                let full_head_dim = self.qk_nope_head_dim + self.qk_rope_head_dim;
                if full_head_dim != self.head_v_dim {
                    let pad_size = full_head_dim - self.head_v_dim;
                    padded_value_states = Tensor::cat(
                        &[
                            &value_states,
                            &Tensor::zeros(
                                [b, self.num_local_heads, t, pad_size],
                                (kind, self.device),
                            ),
                        ],
                        -1,
                    );
                }

                let (cum_seq, max_len) = match sequence_lengths {
                    Some((cum_seq, max_len)) => (Some(cum_seq), *max_len as i64),
                    None => (None, t),
                };

                let _ = query_states.transpose_(1, 2);
                let _ = key_states.transpose_(1, 2);
                let _ = padded_value_states.transpose_(1, 2);

                if cum_seq.is_some() {
                    // reshape to 3D packed format for FA varlen
                    query_states =
                        query_states.reshape([b * t, self.num_local_heads, full_head_dim]);
                    key_states = key_states.reshape([b * t, self.num_local_heads, full_head_dim]);
                    padded_value_states =
                        padded_value_states.reshape([b * t, self.num_local_heads, full_head_dim]);
                }

                let (att, _, _, _, _) = tch::flash_attention_forward(
                    &query_states,
                    &key_states,
                    &padded_value_states,
                    cum_seq,
                    cum_seq,
                    max_len,
                    max_len,
                    0.0,
                    t > 1,
                    false,
                    Some(self.softmax_scale),
                    None,
                    None,
                    None,
                    None,
                )
                .unwrap();

                if full_head_dim != self.head_v_dim {
                    att.narrow(-1, 0, self.head_v_dim)
                } else {
                    att
                }
            }
            AttentionImplementation::Sdpa => {
                // Flash Attention only supports equal dimensions for query, keys, and values.
                // It's actually more efficient to pad to these dimensions so that SDPA will use FA
                // rather than doing vanilla math attention on the smaller value dimension

                let mut padded_value_states = value_states.shallow_clone();
                if self.qk_nope_head_dim + self.qk_rope_head_dim != self.head_v_dim {
                    let pad_size = self.qk_nope_head_dim + self.qk_rope_head_dim - self.head_v_dim;
                    padded_value_states = Tensor::cat(
                        &[
                            &value_states,
                            &Tensor::zeros(
                                [b, self.num_local_heads, t, pad_size],
                                (kind, self.device),
                            ),
                        ],
                        -1,
                    );
                }

                let att = Tensor::scaled_dot_product_attention::<Tensor>(
                    &query_states,
                    &key_states,
                    &padded_value_states,
                    None,
                    0.0,
                    true,
                    Some(self.softmax_scale),
                    false,
                );

                if self.qk_nope_head_dim + self.qk_rope_head_dim != self.head_v_dim {
                    att.narrow(-1, 0, self.head_v_dim)
                } else {
                    att
                }
                .transpose(1, 2)
            }
            AttentionImplementation::Eager => {
                let att = query_states.matmul(&key_states.transpose(-2, -1)) * self.softmax_scale;
                let mask = Tensor::ones([t, t], (kind, self.device))
                    .tril(0)
                    .reshape([1, 1, t, t]);
                let att = att.masked_fill(&mask.eq(0.), f64::NEG_INFINITY);
                att.softmax(-1, kind).matmul(&value_states).transpose(1, 2)
            }
        };

        // Project back to hidden size
        let y = y
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

        // TODO (if needed): DeepseekV2 aux loss

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
        attn_implementation: AttentionImplementation,
        comm: Option<Arc<Communicator>>,
    ) -> Self {
        let mla = MLAAttention::new(&vs / "self_attn", config, attn_implementation, comm.clone());

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

    fn forward(
        &self,
        x: &Tensor,
        position_ids: Option<&Tensor>,
        sequence_lengths: Option<&(Tensor, i32)>,
        cache: &RoPECache,
    ) -> Tensor {
        let residual = x;
        let x = self.mla.forward(
            &self.input_layernorm.forward(x),
            position_ids,
            sequence_lengths,
            cache,
        );
        let x = &x + residual;

        let residual = &x;
        let x = self
            .network
            .forward(&self.post_attention_layernorm.forward(&x));
        x + residual
    }
}

#[allow(dead_code)]
#[derive(Debug)]
pub struct Deepseek {
    embed_tokens: nn::Embedding,
    blocks: Vec<DeepseekBlock>,
    norm: RMSNorm,
    attn_implementation: AttentionImplementation,
    rope_cache: RoPECache,
}

impl Deepseek {
    pub fn new(
        vs: nn::Path,
        config: &DeepseekConfig,
        attn_implementation: AttentionImplementation,
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
                    attn_implementation,
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
            &config.rope_config(),
            config.qk_rope_head_dim.unwrap(),
            config.rope_theta(),
            &vs.device(),
        );

        Self {
            embed_tokens,
            blocks,
            norm,
            attn_implementation,
            rope_cache,
        }
    }
}

impl LanguageModelForward for Deepseek {
    #[allow(unused_variables)]
    fn forward(
        &self,
        x: &Tensor,
        position_ids: Option<&Tensor>,
        sequence_lengths: Option<&Vec<Vec<i32>>>,
        training: bool,
    ) -> Tensor {
        if let NetworkBlock::MoE(_) = &self.blocks[0].network {
            assert!(!training, "DeepseekMoE training not yet supported");
        }

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

        let mut hidden_states = self.embed_tokens.forward(x);

        for block in &self.blocks {
            hidden_states = block.forward(
                &hidden_states,
                position_ids,
                sequence_lengths.as_ref(),
                &self.rope_cache,
            );
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
            attn_implementation.unwrap_or_default(),
            comm,
        ))
    }

    pub fn from_pretrained(
        source: &PretrainedSource<DeepseekConfig>,
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
