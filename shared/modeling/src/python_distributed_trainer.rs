use crate::{
    trainer::DistroResults, ApplyDistroResultError, Batch, BatchData, CausalLM, Communicator, EosToks, LocalTrainer, PythonDistributedCausalLM, StableVariableIterator, TorchDistributedCommunicator, TrainOutput, Trainer, TrainerThreadCommunicationError
};

use psyche_core::{LearningRateSchedule, OptimizerDefinition};
use pyo3::{PyErr, PyResult};
use std::{collections::HashMap, sync::Arc};
use tch::{Device, Tensor};
use thiserror::Error;
use tokio_util::sync::CancellationToken;

#[derive(Debug)]
pub struct PythonDistributedTrainer {
    local: Box<LocalTrainer>,
    comm: TorchDistributedCommunicator,
    iteration: usize,
    device: Device,
}

#[derive(Debug, Error)]
pub enum PythonDistributedTrainerError {
    #[error("No communicator")]
    NoCommunicator,

    #[error("Communicator not a TorchDistributedCommunicator")]
    WrongCommunicator,

    #[error("Python error: {0}")]
    PythonError(#[from] PyErr),

    #[error("Serialization error: {0}")]
    SerializationError(#[from] serde_json::Error),
}

impl PythonDistributedTrainer {
    pub fn new(
        model: PythonDistributedCausalLM,
        lr_scheduler: LearningRateSchedule,
        optimizer: OptimizerDefinition,
        micro_batch_size: usize,
        stats: Option<u32>,
        grad_accum_in_fp32: bool,
    ) -> Result<Self, PythonDistributedTrainerError> {
        let comm = match model.communicator() {
            Some(comm) => match comm.as_ref() {
                Communicator::TorchDistributed(torch) => torch.clone(),
                _ => return Err(PythonDistributedTrainerError::NoCommunicator),
            },
            None => return Err(PythonDistributedTrainerError::WrongCommunicator),
        };

        comm.set("lr_scheduler_json", &serde_json::to_string(&lr_scheduler)?)?;
        comm.set("optimizer_json", &serde_json::to_string(&optimizer)?)?;
        comm.set("micro_batch_size", &micro_batch_size.to_string())?;
        comm.set(
            "grad_accum_in_fp32",
            if grad_accum_in_fp32 { "1" } else { "0" },
        )?;

        let device = model.device();
        let local = Box::new(
            LocalTrainer::new(
                vec![Box::new(model) as Box<dyn CausalLM>],
                lr_scheduler,
                optimizer,
                micro_batch_size,
                stats,
                grad_accum_in_fp32,
                None,
            )
            .into(),
        );

        Ok(Self {
            local,
            comm,
            device,
            iteration: 0,
        })
    }

    pub fn train(
        self,
        step: u32,
        data: Batch,
        warmup_lr_between: Option<(u32, u32)>,
        zero_optim: bool,
        rollback: Vec<(u32, Vec<DistroResults>)>,
        prev_self_distro_results: Option<Vec<DistroResults>>,
        cancel_training: CancellationToken,
    ) -> Result<TrainOutput, TrainerThreadCommunicationError> {
        let data = data.gpu(self.device);
        let tensor = match &data.data {
            BatchData::GPU(tensor) => tensor,
            _ => unreachable!(),
        };
        self.comm.set("step", &step.to_string())?;
        self.comm
            .set("batch-id-start", &data.id.0.start.to_string())?;
        self.comm.set("batch-id-end", &data.id.0.end.to_string())?;
        let batch_shape = tensor
            .size()
            .into_iter()
            .map(|y| y.to_string())
            .collect::<Vec<_>>()
            .join(",");
        self.comm.set("batch-shape", &batch_shape)?;
        self.comm.set(
            "warmup-lr-between",
            &match warmup_lr_between {
                Some((a, b)) => format!("{a},{b}"),
                None => String::new(),
            },
        )?;

        let results_len = match &prev_self_distro_results {
            // we assume (as we do else where) that each result is identically shaped
            Some(distro_results) => distro_results.len(),
            None => 0,
        };
        self.comm.set("results-len", &results_len.to_string())?;
        self.comm.set(&self.iteration.to_string(), "train")?;
        if results_len > 0 {
            self.broadcast_distro_results(prev_self_distro_results.as_ref().unwrap())?;
        }

        self.comm.broadcast(tensor)?;

        self.local
            .train(
                step,
                data,
                warmup_lr_between,
                zero_optim,
                rollback,
                prev_self_distro_results,
                cancel_training,
            )
            .map(|x| TrainOutput {
                trainer: Self {
                    local: match x.trainer {
                        Trainer::Local(local_trainer) => Box::new(local_trainer),
                        Trainer::PythonDistributed(_) => unreachable!()
                    },
                    comm: self.comm,
                    device: self.device,
                    iteration: self.iteration + 1,
                }
                .into(),
                ..x
            })
    }

    pub fn optimize(
        self,
        step: u32,
        warmup_lr_between: Option<(u32, u32)>,
        distro_results: Option<Vec<DistroResults>>,
    ) -> Result<Self, ApplyDistroResultError> {
        let _no_grad = tch::no_grad_guard();

        self.comm.set("step", &step.to_string())?;

        let results_len = match &distro_results {
            // we assume (as we do else where) that each result is identically shaped
            Some(distro_results) => distro_results.len(),
            None => 0,
        };
        self.comm.set("results-len", &results_len.to_string())?;
        self.comm.set(&self.iteration.to_string(), "optimize")?;
        if results_len > 0 {
            self.broadcast_distro_results(distro_results.as_ref().unwrap())?;
        }

        self.local
            .optimize(step, warmup_lr_between, distro_results)
            .map(|x| Self {
                local: Box::new(x),
                comm: self.comm,
                iteration: self.iteration + 1,
                device: self.device,
            })
    }

    pub fn extract(&mut self) -> Result<HashMap<String, Tensor>, TrainerThreadCommunicationError> {
        self.comm.set(&self.iteration.to_string(), "extract")?;
        self.iteration += 1;
        self.local.extract()
    }

    fn broadcast_distro_results(&self, distro_results: &Vec<DistroResults>) -> PyResult<()> {
        let first = distro_results.first().unwrap();
        let params = first.len();
        for param_index in 0..params {
            let sparse_idx = distro_results
                .iter()
                .map(|x| &x[param_index].sparse_idx)
                .collect::<Vec<_>>();
            let sparse_val = distro_results
                .iter()
                .map(|x| &x[param_index].sparse_val)
                .collect::<Vec<_>>();
            self.comm
                .broadcast(&Tensor::stack(&sparse_idx, 0).to(self.device))?;
            self.comm
                .broadcast(&Tensor::stack(&sparse_val, 0).to(self.device))?;
        }
        Ok(())
    }
}

impl From<PythonDistributedTrainer> for Trainer {
    fn from(value: PythonDistributedTrainer) -> Self {
        Self::PythonDistributed(value)
    }
}

impl CausalLM for PythonDistributedTrainer {
    fn forward(
        &mut self,
        x: &Tensor,
        labels: Option<&Tensor>,
        num_logits_to_keep: Option<i64>,
        loss_scale: Option<f64>,
    ) -> (Tensor, Option<Tensor>) {
        self.local.forward(x, labels, num_logits_to_keep, loss_scale)
    }

    fn bos_token_id(&self) -> Option<i64> {
        self.local.bos_token_id()
    }

    fn eos_token_ids(&self) -> Option<EosToks> {
        self.local.eos_token_ids()
    }

    fn device(&self) -> Device {
        self.device
    }

    fn variables(&self) -> StableVariableIterator {
        self.local.variables()
    }

    fn communicator(&self) -> Option<Arc<Communicator>> {
        self.local.communicator()
    }

    fn prepare_for_training(&mut self) {
        self.local.prepare_for_training();
    }

    fn clip_grad_norm(&mut self, max_grad_norm: f64) {
        self.local.clip_grad_norm(max_grad_norm);
    }
}