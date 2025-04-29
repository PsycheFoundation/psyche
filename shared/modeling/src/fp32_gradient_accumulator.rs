use crate::{AllReduce, CausalLM, Communicator, ReduceType};

use std::sync::Arc;
use tch::{Kind, Tensor};

pub struct Fp32GradientAccumulator {
    parameters: Vec<(Tensor, (i64, i64))>,
    fp32_grads: Tensor,
}

impl Fp32GradientAccumulator {
    pub fn new(model: &dyn CausalLM) -> Self {
        let _no_grad = tch::no_grad_guard();
        let mut total_numel: i64 = 0;

        let parameters = model
            .variables()
            .filter_map(|parameter| {
                let parameter = parameter.local_tensor();
                match parameter.requires_grad() {
                    true => {
                        let numel = parameter.numel() as i64;
                        let ret = (
                            parameter.shallow_clone(),
                            (total_numel, total_numel + numel),
                        );
                        total_numel += numel;
                        Some(ret)
                    }
                    false => None,
                }
            })
            .collect::<Vec<_>>();

        let fp32_grads = Tensor::zeros([total_numel], (Kind::Float, model.device()));

        Self {
            parameters,
            fp32_grads,
        }
    }

    pub fn accumulate_gradients(&mut self) {
        let _no_grad = tch::no_grad_guard();
        for (param, (start, end)) in &mut self.parameters {
            let grad = param.grad();
            let mut grad_slice = self.fp32_grads.slice(0, *start, *end, 1);
            let _t = grad_slice.g_add_(&grad.to_kind(Kind::Float).view([-1]));
            param.zero_grad();
        }
    }

    pub fn apply_accumulation(&mut self) {
        let _no_grad = tch::no_grad_guard();
        for (param, (start, end)) in &self.parameters {
            let mut grad = param.grad();
            let grad_slice = self.fp32_grads.slice(0, *start, *end, 1);
            grad.copy_(&grad_slice.to_kind(param.kind()).view_as(param));
        }
    }

    pub fn zero_grad(&mut self) {
        let _ = self.fp32_grads.zero_();
    }

    pub fn get_full_grad_buffer(&self) -> &Tensor {
        &self.fp32_grads
    }

    pub fn reduce_gradients(&mut self, comm: Arc<Communicator>) {
        self.fp32_grads.all_reduce(&Some(comm), ReduceType::Mean);
    }
}
