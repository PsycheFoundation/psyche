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
    [
        "model.norm.weight",
        "model.layers.0.mlp.up_proj.weight",
        "model.layers.0.post_attention_layernorm.weight",
        "model.layers.0.self_attn.q_proj.weight",
        "model.embed_tokens.weight",
        "model.layers.0.self_attn.o_proj.weight",
        "model.layers.0.self_attn.v_proj.weight",
        "model.layers.0.self_attn.k_proj.weight",
        "model.layers.0.mlp.gate_proj.weight",
        "model.layers.0.mlp.down_proj.weight",
        "lm_head.weight",
        "model.layers.0.input_layernorm.weight",
    ]
    .into_iter()
    .map(|p| (p.to_string(), Tensor::zeros([1], tch::kind::FLOAT_CPU)))
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
        &self,
        x: &tch::Tensor,
        _labels: Option<&tch::Tensor>,
        _position_ids: Option<&tch::Tensor>,
        _sequence_lengths: Option<&Vec<Vec<i32>>>,
        _num_logits_to_keep: Option<i64>,
        loss_scale: Option<f64>,
    ) -> (Option<tch::Tensor>, Option<tch::Tensor>) {
        let result = tch::Tensor::zeros([1], (Kind::BFloat16, x.device()));
        let loss = tch::Tensor::zeros([1], (Kind::BFloat16, x.device()));
        let loss = loss.set_requires_grad(true);
        let loss = loss.g_add_scalar(1.0);
        let loss = match loss_scale {
            Some(loss_scale) => loss / loss_scale,
            None => loss,
        };

        // sleep some time just to simulate training
        std::thread::sleep(self.training_delay_secs);
        (Some(result), Some(loss))
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

    fn max_context_length(&self) -> usize {
        2048
    }

    fn variables(&self) -> StableVariableIterator {
        Box::new(StableVarStoreIterator::new(&self.var_store, None))
    }

    fn communicator(&self) -> Option<std::sync::Arc<crate::Communicator>> {
        None
    }

    fn prepare_for_training(&self) {}

    fn clip_grad_norm(&self, _max_grad_norm: f64) {}

    fn convert(&self, state_dict: Option<HashMap<String, Tensor>>) -> HashMap<String, Tensor> {
        state_dict.unwrap_or_default()
    }
}
