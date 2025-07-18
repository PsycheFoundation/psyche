use crate::{CausalLM, EosToks, ModelConfig, StableVarStoreIterator, StableVariableIterator};
use serde::{Deserialize, Serialize};
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

const DUMMY_PARAMS: &[(&str, usize)] = &[
    ("model.norm.weight", 1),
    ("model.layers.0.mlp.up_proj.weight", 2),
    ("model.layers.0.post_attention_layernorm.weight", 1),
    ("model.layers.0.self_attn.q_proj.weight", 2),
    ("model.embed_tokens.weight", 2),
    ("model.layers.0.self_attn.o_proj.weight", 2),
    ("model.layers.0.self_attn.v_proj.weight", 2),
    ("model.layers.0.self_attn.k_proj.weight", 2),
    ("model.layers.0.mlp.gate_proj.weight", 2),
    ("model.layers.0.mlp.down_proj.weight", 2),
    ("lm_head.weight", 2),
    ("model.layers.0.input_layernorm.weight", 1),
];

/// The dummy size is (9*x^2 + 3x) * f32 size bytes - so a size of `50` is ~90kb, a size of `512` is ~9mb, a size of `4096` is ~600 mb, etc.
/// You may tweak these numbers if you want less/more dummy training and p2p blob size
pub fn get_dummy_parameters(size: i64) -> HashMap<String, Tensor> {
    DUMMY_PARAMS
        .iter()
        .map(|(name, dims)| {
            let shape: Vec<i64> = vec![size; *dims];
            (
                name.to_string(),
                Tensor::zeros(shape, (tch::Kind::Float, tch::Device::Cpu)),
            )
        })
        .collect()
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct DummyConfig {
    pub size: i64,
}

impl Default for DummyModel {
    fn default() -> Self {
        Self::new(512, None)
    }
}

impl ModelConfig for DummyConfig {
    fn get_parameter_names(&self) -> Vec<String> {
        DUMMY_PARAMS.iter().map(|p| p.0.to_string()).collect()
    }
}

impl DummyModel {
    pub fn new(size: i64, training_time_secs: Option<f64>) -> Self {
        let parameters = get_dummy_parameters(size);
        let variables = Variables {
            named_variables: parameters,
            shards: HashMap::new(),
            trainable_variables: Vec::new(),
        };
        let mut var_store = VarStore::new(Device::Cpu);
        var_store.variables_ = Arc::new(Mutex::new(variables));
        Self {
            var_store,
            training_delay_secs: Duration::from_secs_f64(training_time_secs.unwrap_or(5.0)),
        }
    }
}

impl CausalLM for DummyModel {
    fn forward(
        &mut self,
        _x: &tch::Tensor,
        _labels: Option<&tch::Tensor>,
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
