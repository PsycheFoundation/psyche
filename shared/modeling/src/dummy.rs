use crate::{CausalLM, EosToks, StableVarStoreIterator, StableVariableIterator};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::Duration,
};
use tch::{
    nn::{VarStore, Variables},
    Device, Kind, Tensor,
};

#[derive(Debug)]
pub struct DummyModel {
    var_store: VarStore,
    training_delay_secs: Duration,
}

pub fn get_dummy_parameters() -> HashMap<String, Tensor> {
    // These shapes match LlamaConfig::dummy() which has hidden_size=1, vocab_size=1, etc.
    [
        ("model.norm.weight", vec![1]), // [hidden_size] = [1]
        ("model.layers.0.mlp.up_proj.weight", vec![1, 1]), // [intermediate_size, hidden_size] = [1, 1]
        ("model.layers.0.post_attention_layernorm.weight", vec![1]), // [hidden_size] = [1]
        ("model.layers.0.self_attn.q_proj.weight", vec![1, 1]), // [num_heads * head_dim, hidden_size] = [1, 1]
        ("model.embed_tokens.weight", vec![1, 1]), // [vocab_size, hidden_size] = [1, 1]
        ("model.layers.0.self_attn.o_proj.weight", vec![1, 1]), // [hidden_size, num_heads * head_dim] = [1, 1]
        ("model.layers.0.self_attn.v_proj.weight", vec![1, 1]), // [num_kv_heads * head_dim, hidden_size] = [1, 1]
        ("model.layers.0.self_attn.k_proj.weight", vec![1, 1]), // [num_kv_heads * head_dim, hidden_size] = [1, 1]
        ("model.layers.0.mlp.gate_proj.weight", vec![1, 1]), // [intermediate_size, hidden_size] = [1, 1]
        ("model.layers.0.mlp.down_proj.weight", vec![1, 1]), // [hidden_size, intermediate_size] = [1, 1]
        ("lm_head.weight", vec![1, 1]),                      // [vocab_size, hidden_size] = [1, 1]
        ("model.layers.0.input_layernorm.weight", vec![1]),  // [hidden_size] = [1]
    ]
    .into_iter()
    .map(|(name, shape)| {
        (
            name.to_string(),
            Tensor::zeros(shape, (tch::Kind::Float, tch::Device::Cpu)),
        )
    })
    .collect()
}

impl Default for DummyModel {
    fn default() -> Self {
        Self::new(2)
    }
}

impl DummyModel {
    pub fn new(training_delay: u64) -> Self {
        let parameters = get_dummy_parameters();
        let variables = Variables {
            named_variables: parameters,
            shards: HashMap::new(),
            trainable_variables: Vec::new(),
        };
        let mut var_store = VarStore::new(Device::cuda_if_available());
        var_store.variables_ = Arc::new(Mutex::new(variables));
        Self {
            var_store,
            training_delay_secs: Duration::from_secs(training_delay),
        }
    }
}

impl CausalLM for DummyModel {
    fn forward(
        &mut self,
        x: &tch::Tensor,
        _labels: Option<&tch::Tensor>,
        _num_logits_to_keep: Option<i64>,
        loss_scale: Option<f64>,
    ) -> (tch::Tensor, Option<tch::Tensor>) {
        let shapes = [
            (
                vec![500, 4, 16],
                vec![500, 4, 8],
                vec![500, 4, 64, 64],
                4096,
            ),
            (vec![4, 8], vec![4, 8], vec![4, 64], 64),
            (vec![4, 4, 16], vec![4, 4, 8], vec![4, 4, 64, 64], 4096),
            (vec![1, 4, 16], vec![1, 4, 8], vec![1, 4, 32, 64], 2048),
        ];

        let (_, _, xshape, _) = &shapes[0];

        let result = tch::Tensor::zeros(xshape, (Kind::BFloat16, x.device()));
        let loss = tch::Tensor::zeros([1], (Kind::BFloat16, x.device()));
        let loss = loss.set_requires_grad(true);
        let loss = loss.g_add_scalar(1.0);
        let loss = match loss_scale {
            Some(loss_scale) => loss / loss_scale,
            None => loss,
        };

        // sleep some time just to simulate training
        std::thread::sleep(self.training_delay_secs);
        (result, Some(loss))
    }

    fn bos_token_id(&self) -> Option<i64> {
        None
    }

    fn eos_token_ids(&self) -> Option<EosToks> {
        None
    }

    fn device(&self) -> tch::Device {
        Device::cuda_if_available()
    }

    fn variables(&self) -> StableVariableIterator {
        Box::new(StableVarStoreIterator::new(&self.var_store, None))
    }

    fn communicator(&self) -> Option<std::sync::Arc<crate::Communicator>> {
        None
    }

    fn prepare_for_training(&mut self) {}

    fn clip_grad_norm(&mut self, _max_grad_norm: f64) {}

    fn is_dummy_model(&self) -> bool {
        true
    }
}
