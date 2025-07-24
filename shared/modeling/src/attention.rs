use crate::{ColumnParallelLinear, Communicator, RoPECache, RowParallelLinear};
use std::sync::Arc;
use tch::{Device, Tensor, nn::Module};

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
    use_sdpa: bool,
    tp_size: i64,
}

impl CausalSelfAttention {
    pub fn new(
        vs: tch::nn::Path,
        n_head: i64,
        n_kvheads: i64,
        n_embd: i64,
        n_max_seq_len: i64,
        use_sdpa: bool,
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
            use_sdpa,
            tp_size,
        }
    }

    pub fn forward(&self, x: &Tensor, position_ids: Option<&Tensor>, cache: &RoPECache) -> Tensor {
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

        let q = cache.apply_rotary_emb(&q, position_ids).to_kind(kind);
        let k = cache.apply_rotary_emb(&k, position_ids).to_kind(kind);

        let k = repeat_kv(&k, local_n_head / local_n_kvhead);
        let v = repeat_kv(&v, local_n_head / local_n_kvhead);

        let scale = 1.0 / (self.head_dim as f64).sqrt();

        let y = if self.use_sdpa {
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
        } else {
            let att = q.matmul(&k.transpose(-2, -1)) * scale;
            let mask = Tensor::ones([t, t], (kind, self.device))
                .tril(0)
                .reshape([1, 1, t, t]);
            let att = att.masked_fill(&mask.eq(0.), f64::NEG_INFINITY);
            let y = att.softmax(-1, kind).matmul(&v);
            y.transpose(1, 2)
                .contiguous()
                .reshape([b, t, local_n_head * self.head_dim])
        };

        self.o_proj.forward(&y)
    }
}
