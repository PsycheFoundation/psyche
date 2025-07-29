use crate::{
    AttentionImplementation, ColumnParallelLinear, Communicator, RoPECache, RowParallelLinear,
};
use std::sync::Arc;
use tch::{Device, Tensor, flash_attention_forward, nn::Module};

fn repeat_kv(hidden_states: &Tensor, n_rep: i64) -> Tensor {
    let (batch, num_key_value_heads, slen, head_dim) = hidden_states.size4().unwrap();

    if n_rep == 1 {
        return hidden_states.shallow_clone();
    }

    let hidden_states = hidden_states
        .unsqueeze(2)
        .expand([batch, num_key_value_heads, n_rep, slen, head_dim], false);

    hidden_states.reshape([batch, num_key_value_heads * n_rep, slen, head_dim])
}

pub fn create_cu_seqlens(lengths: &Vec<Vec<i32>>, device: Device) -> (Tensor, i32) {
    let mut seq_lens: Vec<i32> = Vec::new();
    for batch in lengths.iter() {
        for &len in batch.iter() {
            if len > 0 {
                seq_lens.push(len);
            }
        }
    }

    let mut cum: Vec<i32> = vec![0];
    let mut current: i32 = 0;
    let mut max = 0;
    for &len in seq_lens.iter() {
        if len > max {
            max = len;
        }
        current += len;
        cum.push(current);
    }

    (Tensor::from_slice(&cum).to(device), max)

    // let max_inner_len = lengths.iter().map(|v| v.len()).max().unwrap_or(0);
    // let mut max_len = 0;
    // let mut padded = Vec::with_capacity(lengths.len());
    // for mut v in lengths.clone() {
    //     v.resize(max_inner_len, 0);
    //     for x in &v {
    //         if *x > max_len {
    //             max_len = *x;
    //         }
    //     }
    //     padded.push(v);
    // }
    // println!("padded: {:?}: max_len: {}", padded, max_len);
    // (
    //     Tensor::from_slice2(&padded)
    //         .to(device)
    //         .cumsum(0, None).pad([1,0], "constant", None)
    //         .to_kind(tch::Kind::Int),
    //     max_len,
    // )
}

#[allow(dead_code)]
#[derive(Debug)]
pub struct CausalSelfAttention {
    q_proj: ColumnParallelLinear,
    k_proj: ColumnParallelLinear,
    v_proj: ColumnParallelLinear,
    o_proj: RowParallelLinear,
    n_head: i64,
    n_kvhead: i64,
    n_embd: i64,
    n_max_seq_len: i64,
    head_dim: i64,
    device: Device,
    attn_implementation: AttentionImplementation,
    tp_size: i64,
}

impl CausalSelfAttention {
    pub fn new(
        vs: tch::nn::Path,
        n_head: i64,
        n_kvheads: i64,
        n_embd: i64,
        n_max_seq_len: i64,
        attn_implementation: AttentionImplementation,
        comm: Option<Arc<Communicator>>,
    ) -> Self {
        let tp_size = comm.as_ref().map(|x| x.size()).unwrap_or(1);
        assert_eq!(n_head % tp_size, 0, "n_head must be divisible by tp_size");
        assert_eq!(
            n_kvheads % tp_size,
            0,
            "n_kvheads must be divisible by tp_size"
        );

        let head_dim = n_embd / n_head;
        let size_q = head_dim * n_head;
        let size_kv = head_dim * n_kvheads;

        let q_proj =
            ColumnParallelLinear::new(&vs / "q_proj", n_embd, size_q, false, false, comm.clone());
        let k_proj =
            ColumnParallelLinear::new(&vs / "k_proj", n_embd, size_kv, false, false, comm.clone());
        let v_proj =
            ColumnParallelLinear::new(&vs / "v_proj", n_embd, size_kv, false, false, comm.clone());
        let o_proj = RowParallelLinear::new(&vs / "o_proj", size_q, n_embd, false, true, comm);

        Self {
            q_proj,
            k_proj,
            v_proj,
            o_proj,
            n_head,
            n_kvhead: n_kvheads,
            n_embd,
            n_max_seq_len,
            head_dim,
            device: vs.device(),
            attn_implementation,
            tp_size,
        }
    }

    pub fn forward(
        &self,
        x: &Tensor,
        position_ids: Option<&Tensor>,
        sequence_lengths: Option<&(Tensor, i32)>,
        cache: &RoPECache,
    ) -> Tensor {
        let (b, t, c) = x.size3().unwrap();
        assert_eq!(c, self.n_embd, "Input hidden size mismatch");
        let kind = x.kind();

        let q = self.q_proj.forward(x);
        let k = self.k_proj.forward(x);
        let v = self.v_proj.forward(x);

        let local_n_head = self.n_head / self.tp_size;
        let local_n_kvhead = self.n_kvhead / self.tp_size;

        let q = q
            .contiguous()
            .reshape([b, t, local_n_head, self.head_dim])
            .transpose(1, 2);
        let k = k
            .contiguous()
            .reshape([b, t, local_n_kvhead, self.head_dim])
            .transpose(1, 2);
        let v = v
            .contiguous()
            .reshape([b, t, local_n_kvhead, self.head_dim])
            .transpose(1, 2);

        let mut q = cache.apply_rotary_emb(&q, position_ids).to_kind(kind);
        let k = cache.apply_rotary_emb(&k, position_ids).to_kind(kind);

        let mut k = repeat_kv(&k, local_n_head / local_n_kvhead);
        let mut v = repeat_kv(&v, local_n_head / local_n_kvhead);

        let scale = 1.0 / (self.head_dim as f64).sqrt();

        let y = match self.attn_implementation {
            AttentionImplementation::FlashAttention2 => {
                let (cum_seq, max_len) = match sequence_lengths {
                    Some((cum_seq, max_len)) => (Some(cum_seq), *max_len as i64),
                    None => (None, t),
                };

                let _ = q.transpose_(1, 2);
                let _ = k.transpose_(1, 2);
                let _ = v.transpose_(1, 2);

                if cum_seq.is_some() {
                    // reshape to 3D packed format for FA varlen
                    q = q.reshape([b * t, local_n_head, self.head_dim]);
                    k = k.reshape([b * t, local_n_head, self.head_dim]);
                    v = v.reshape([b * t, local_n_head, self.head_dim]);
                }

                let (att, _, _, _, _) = flash_attention_forward(
                    &q,
                    &k,
                    &v,
                    cum_seq,
                    cum_seq,
                    max_len,
                    max_len,
                    0.0,
                    t > 1,
                    false,
                    Some(scale),
                    None,
                    None,
                    None,
                    None,
                )
                .unwrap();
                att.contiguous()
                    .reshape([b, t, local_n_head * self.head_dim])
            }
            AttentionImplementation::Sdpa => {
                let att = Tensor::scaled_dot_product_attention::<Tensor>(
                    &q,
                    &k,
                    &v,
                    None,
                    0.0,
                    t > 1,
                    Some(scale),
                    false,
                );
                att.transpose(1, 2)
                    .contiguous()
                    .reshape([b, t, local_n_head * self.head_dim])
            }
            AttentionImplementation::Eager => {
                let att = q.matmul(&k.transpose(-2, -1)) * scale;
                let mask = Tensor::ones([t, t], (kind, self.device))
                    .tril(0)
                    .reshape([1, 1, t, t]);
                let att = att.masked_fill(&mask.eq(0.), f64::NEG_INFINITY);
                let y = att.softmax(-1, kind).matmul(&v);
                y.transpose(1, 2)
                    .contiguous()
                    .reshape([b, t, local_n_head * self.head_dim])
            }
        };

        self.o_proj.forward(&y)
    }
}
