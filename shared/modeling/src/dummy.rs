use crate::{CausalLM, EosToks, StableVarStoreIterator, StableVariableIterator};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::Duration,
};
use tch::{
    Device, Kind, Tensor,
    nn::{VarStore, Variables},
};

#[derive(Debug)]
pub struct DummyModel {
    var_store: VarStore,
    training_delay_secs: Duration,
}

pub fn get_dummy_parameters() -> HashMap<String, Tensor> {
    // You may tweak these numbers if you want less/more dummy training and p2p blob size
    [
        ("model.norm.weight", vec![512]),
        ("model.layers.0.mlp.up_proj.weight", vec![512, 512]),
        ("model.layers.0.post_attention_layernorm.weight", vec![512]),
        ("model.layers.0.self_attn.q_proj.weight", vec![512, 512]),
        ("model.embed_tokens.weight", vec![512, 512]),
        ("model.layers.0.self_attn.o_proj.weight", vec![512, 512]),
        ("model.layers.0.self_attn.v_proj.weight", vec![512, 512]),
        ("model.layers.0.self_attn.k_proj.weight", vec![512, 512]),
        ("model.layers.0.mlp.gate_proj.weight", vec![512, 512]),
        ("model.layers.0.mlp.down_proj.weight", vec![512, 512]),
        ("lm_head.weight", vec![512, 512]),
        ("model.layers.0.input_layernorm.weight", vec![512]),
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
        Self::new(500)
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
        let mut var_store = VarStore::new(Device::Cpu);
        var_store.variables_ = Arc::new(Mutex::new(variables));
        Self {
            var_store,
            training_delay_secs: Duration::from_millis(training_delay),
        }
    }
}

impl CausalLM for DummyModel {
    fn forward(
        &mut self,
        _x: &tch::Tensor,
        _labels: Option<&tch::Tensor>,
        _position_ids: Option<&tch::Tensor>,
        _sequence_lengths: Option<&Vec<Vec<i32>>>,
        _num_logits_to_keep: Option<i64>,
        loss_scale: Option<f64>,
    ) -> (tch::Tensor, Option<tch::Tensor>) {
        let shape = vec![1, 1, 1];
        let cpu_device = tch::Device::Cpu;

        let result = tch::Tensor::zeros(&shape, (Kind::BFloat16, cpu_device));
        let loss = tch::Tensor::zeros([1], (Kind::BFloat16, cpu_device));
        let loss = loss.set_requires_grad(true);
        let loss = loss.g_add_scalar(1.0);
        let loss = match loss_scale {
            Some(loss_scale) => loss / loss_scale,
            None => loss,
        };

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
        Device::Cpu
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
