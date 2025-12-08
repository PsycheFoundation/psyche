use crate::{CausalLM, Distro};
use psyche_core::OptimizerDefinition;
use tch::COptimizer;

pub enum Optimizer {
    Torch {
        optimizer: COptimizer,
        clip_grad_norm: Option<f32>,
    },
    Distro {
        optimizer: Box<Distro>,
        clip_grad_norm: Option<f32>,
        quantize_1bit: bool,
    },
    Null,
}

impl Optimizer {
    pub fn new(definition: OptimizerDefinition, model: &dyn CausalLM) -> Self {
        match definition {
            OptimizerDefinition::AdamW {
                betas,
                weight_decay,
                eps,
                clip_grad_norm,
            } => Self::Torch {
                optimizer: {
                    let mut adamw = COptimizer::adamw(
                        1.0e-1,
                        betas[0] as f64,
                        betas[1] as f64,
                        weight_decay as f64,
                        eps as f64,
                        false,
                    )
                    .unwrap();
                    for var in model.variables() {
                        let tensor = var.logical_tensor();
                        adamw.add_parameters(&tensor, 0).unwrap();
                    }
                    adamw
                },
                clip_grad_norm,
            },
            OptimizerDefinition::Distro {
                clip_grad_norm,
                weight_decay,
                compression_decay,
                compression_topk,
                compression_chunk,
                quantize_1bit,
            } => Self::Distro {
                optimizer: Distro::new(
                    model,
                    compression_decay as f64,
                    compression_chunk as i64,
                    compression_topk as i64,
                    weight_decay.unwrap_or(0.0) as f64,
                )
                .into(),
                clip_grad_norm,
                quantize_1bit,
            },
            OptimizerDefinition::Dummy => Self::Null,
        }
    }
}
