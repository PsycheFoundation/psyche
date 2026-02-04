use crate::{
    ApplyDistroResultError, Batch, BatchData, CausalLM, Communicator, EosToks, LocalTrainer,
    ParallelModels, PythonDistributedCausalLM, ReduceType, StableVariableIterator,
    TorchDistributedCommunicator, TrainOutput, Trainer, TrainerThreadCommunicationError,
    python_causal_lm::WrappedPythonCausalLM, trainer::DistroResults,
};

use psyche_core::{Barrier, CancelledBarrier, LearningRateSchedule, OptimizerDefinition};
use pyo3::{PyErr, PyResult};
use std::{
    collections::HashMap,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};
use tch::{Device, Kind, Tensor};
use thiserror::Error;
use tokio_util::sync::CancellationToken;
use tracing::{debug, trace};

#[derive(Debug)]
pub struct PythonDistributedTrainer {
    model: PythonDistributedCausalLM,
    local: Box<LocalTrainer>,
    comm: TorchDistributedCommunicator,
    iteration: Arc<AtomicUsize>,
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

#[derive(Debug)]
pub struct NopBarrier;

impl Barrier for NopBarrier {
    fn wait(&self) -> Result<(), CancelledBarrier> {
        Ok(())
    }

    fn cancel(&self) {}

    fn reset(&self) {}

    fn is_cancelled(&self) -> bool {
        false
    }
}

impl Default for NopBarrier {
    fn default() -> Self {
        Self
    }
}

impl PythonDistributedTrainer {
    pub fn new(
        model: PythonDistributedCausalLM,
        lr_scheduler: LearningRateSchedule,
        optimizer: OptimizerDefinition,
        mut micro_batch_size: usize,
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

        if model.parallelism.dp > 1 {
            debug!(
                "Increasing micro batch size from {} to {} to account for FSDP sharding size of {}",
                micro_batch_size,
                micro_batch_size * model.parallelism.dp,
                model.parallelism.dp
            );

            micro_batch_size *= model.parallelism.dp;
        }

        let hyperparameters = serde_json::json!({
            "operation": "hyperparameters",
            "lr_scheduler": lr_scheduler,
            "optimizer": optimizer,
            "micro_batch_size": micro_batch_size,
            "grad_accum_in_fp32": grad_accum_in_fp32
        });

        let iteration = model.iteration().fetch_add(1, Ordering::Relaxed);
        let device = model.device();

        trace!(
            "Sending hyperparameters operation to Python clients, iteration = {}",
            iteration
        );
        comm.set(&iteration.to_string(), &hyperparameters.to_string())?;

        // barrier to ensure everyone has seen the broadcast
        let dummy = Tensor::zeros([], (Kind::Float, device));
        comm.all_reduce(&dummy, ReduceType::Sum)?;

        let it = model.iteration();
        let local: WrappedPythonCausalLM = model.local.clone();
        let local = Box::new(LocalTrainer::new(
            ParallelModels {
                models: vec![Box::new(local) as Box<dyn CausalLM>],
                barrier: Arc::new(NopBarrier) as Arc<dyn Barrier>,
                data_parallel: None,
            },
            lr_scheduler,
            optimizer,
            micro_batch_size,
            stats,
            grad_accum_in_fp32,
        ));

        comm.delete(&iteration.to_string())?;

        Ok(Self {
            model,
            local,
            comm,
            device,
            iteration: it,
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub fn train(
        self,
        step: u32,
        mut data: Batch,
        warmup_lr_between: Option<(u32, u32)>,
        zero_optim: bool,
        rollback: Vec<(u32, Vec<DistroResults>)>,
        prev_self_distro_results: Option<Vec<DistroResults>>,
        cancel_training: CancellationToken,
    ) -> Result<TrainOutput, TrainerThreadCommunicationError> {
        let world_size = self.comm.size();
        let original_batch_size = data.data.size();

        // Pad the batch if necessary for FSDP
        if world_size > 1 {
            trace!(
                "Checking batch padding: original batch size = {}, world_size = {}",
                original_batch_size, world_size
            );

            data.pad(world_size);

            let new_size = data.data.size();
            if new_size != original_batch_size {
                debug!(
                    "FSDP: Padded batch from {} to {} samples (world_size={})",
                    original_batch_size, new_size, world_size
                );
            }
        }

        let data = data.gpu(self.device);
        debug!("Training on device: {:?}", self.device);
        let batch_data = match &data.data {
            BatchData::GPU(batch_data) => batch_data,
            _ => unreachable!(),
        };

        let padded_bs = batch_data.input_ids.size()[0] as f64;

        let results_len = match &prev_self_distro_results {
            // we assume (as we do else where) that each result is identically shaped
            Some(distro_results) => distro_results.len(),
            None => 0,
        };

        let operation = serde_json::json!({
            "operation": "train",
            "step": step,
            "batch_id": (data.id.0.start, data.id.0.end),
            "batch_shape": batch_data.input_ids.size(),
            "batch_has_labels": batch_data.labels.is_some(),
            "batch_has_position_ids": batch_data.position_ids.is_some(),
            "batch_sequence_lengths": batch_data.sequence_lengths,
            "warmup_lr_between": warmup_lr_between,
            "zero_optim": zero_optim,
            "results_len": results_len,
            "results_metadata": prev_self_distro_results.as_ref().map(|r| Self::distro_results_metadata(r)),
        });

        let iteration = self.iteration.fetch_add(1, Ordering::Relaxed);
        trace!(
            "Sending train operation to Python clients, iteration = {}",
            iteration
        );

        self.comm
            .set(&iteration.to_string(), &operation.to_string())?;

        // barrier to ensure everyone has seen the broadcast
        let dummy = Tensor::zeros([], (Kind::Float, self.device));
        self.comm.all_reduce(&dummy, ReduceType::Sum)?;

        if results_len > 0 {
            self.broadcast_distro_results(prev_self_distro_results.as_ref().unwrap())?;
        }

        self.comm.broadcast(&batch_data.input_ids)?;
        if let Some(labels) = &batch_data.labels {
            self.comm.broadcast(labels)?;
        }
        if let Some(position_ids) = &batch_data.position_ids {
            self.comm.broadcast(position_ids)?;
        }

        let ret = self.local.train(
            step,
            data,
            warmup_lr_between,
            zero_optim,
            rollback,
            prev_self_distro_results,
            cancel_training,
        )?;

        // reduce the loss across all shards
        let loss = Tensor::from_slice(&[ret.loss])
            .to_kind(Kind::Float)
            .to_device(self.device);
        let _ = self.comm.all_reduce(&loss, ReduceType::Sum);

        let mut loss: f32 = loss.try_into().unwrap();
        loss /= self.comm.size() as f32; // average from all reduced sums of loss above
        loss *= padded_bs as f32 / original_batch_size as f32; // undilute for padding

        trace!("Train operation complete on all Python clients");
        self.comm.delete(&iteration.to_string())?;

        Ok(TrainOutput {
            trainer: Self {
                local: match ret.trainer {
                    Trainer::Local(local_trainer) => Box::new(local_trainer),
                    Trainer::PythonDistributed(_) => unreachable!(),
                },
                comm: self.comm,
                device: self.device,
                iteration: self.iteration,
                model: self.model,
            }
            .into(),
            loss,
            ..ret
        })
    }

    pub fn optimize(
        self,
        step: u32,
        warmup_lr_between: Option<(u32, u32)>,
        distro_results: Option<Vec<DistroResults>>,
    ) -> Result<Self, ApplyDistroResultError> {
        let _no_grad = tch::no_grad_guard();

        let results_len = match &distro_results {
            // we assume (as we do else where) that each result is identically shaped
            Some(distro_results) => distro_results.len(),
            None => 0,
        };

        let operation = serde_json::json!({
            "operation": "optimize",
            "step": step,
            "warmup_lr_between": warmup_lr_between,
            "results_len": results_len,
            "results_metadata": distro_results.as_ref().map(|r| Self::distro_results_metadata(r)),
        });

        let iteration = self.iteration.fetch_add(1, Ordering::Relaxed);
        trace!(
            "Sending optimize operation to Python clients, iteration = {}",
            iteration
        );

        self.comm
            .set(&iteration.to_string(), &operation.to_string())?;

        // barrier to ensure everyone has seen the broadcast
        let dummy = Tensor::zeros([], (Kind::Float, self.device));
        self.comm.all_reduce(&dummy, ReduceType::Sum)?;

        if results_len > 0 {
            self.broadcast_distro_results(distro_results.as_ref().unwrap())?;
        }

        let result = self.local.optimize(step, warmup_lr_between, distro_results);

        trace!("Optimize operation complete on all Python clients");
        self.comm.delete(&iteration.to_string())?;

        result.map(|x| Self {
            local: Box::new(x),
            comm: self.comm,
            iteration: self.iteration,
            device: self.device,
            model: self.model,
        })
    }

    pub fn extract(&mut self) -> Result<HashMap<String, Tensor>, TrainerThreadCommunicationError> {
        let operation = serde_json::json!({
            "operation": "extract",
        });

        let iteration = self.iteration.fetch_add(1, Ordering::Relaxed);
        trace!(
            "Sending extract operation to Python clients, iteration = {}",
            iteration
        );

        self.comm
            .set(&iteration.to_string(), &operation.to_string())?;

        // barrier to ensure everyone has seen the broadcast
        let dummy = Tensor::zeros([], (Kind::Float, self.device));
        self.comm.all_reduce(&dummy, ReduceType::Sum)?;

        let result = self.local.extract()?;

        trace!("Extract operation complete on all Python clients");
        self.comm.delete(&iteration.to_string())?;

        Ok(result)
    }

    fn broadcast_distro_results(&self, distro_results: &[DistroResults]) -> PyResult<()> {
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
            let sparse_idx = Tensor::stack(&sparse_idx, 0).to(self.device);
            self.comm.broadcast(&sparse_idx)?;
            let sparse_val = Tensor::stack(&sparse_val, 0).to(self.device);
            self.comm.broadcast(&sparse_val)?;
        }
        Ok(())
    }

    fn distro_results_metadata(distro_results: &[DistroResults]) -> serde_json::Value {
        serde_json::json!({
            "sparse_idx_size": distro_results.first().map(|y| y.iter().map(|z| z.sparse_idx.size()).collect::<Vec<_>>()),
            "sparse_idx_dtype": distro_results.first().map(|y| y.first().map(|z| z.sparse_idx.kind().c_int())),
            "sparse_val_size": distro_results.first().map(|y| y.iter().map(|z| z.sparse_val.size()).collect::<Vec<_>>()),
            "sparse_val_dtype": distro_results.first().map(|y| y.first().map(|z| z.sparse_val.kind().c_int())),
            "xshape": distro_results.first().map(|y| y.iter().map(|z| z.xshape.clone()).collect::<Vec<_>>()),
            "totalk": distro_results.first().map(|y| y.iter().map(|z| z.totalk).collect::<Vec<_>>()),
        })
    }

    pub fn can_do_inference(&self) -> bool {
        self.local.can_do_inference()
    }
}

impl From<PythonDistributedTrainer> for Trainer {
    fn from(value: PythonDistributedTrainer) -> Self {
        Self::PythonDistributed(value)
    }
}

impl CausalLM for PythonDistributedTrainer {
    fn forward(
        &self,
        x: &Tensor,
        labels: Option<&Tensor>,
        position_ids: Option<&Tensor>,
        sequence_lengths: Option<&Vec<Vec<i32>>>,
        num_logits_to_keep: Option<i64>,
        loss_scale: Option<f64>,
    ) -> (Option<Tensor>, Option<Tensor>) {
        self.model.forward(
            x,
            labels,
            position_ids,
            sequence_lengths,
            num_logits_to_keep,
            loss_scale,
        )
    }

    fn bos_token_id(&self) -> Option<i64> {
        self.model.bos_token_id()
    }

    fn eos_token_ids(&self) -> Option<EosToks> {
        self.model.eos_token_ids()
    }

    fn device(&self) -> Device {
        self.device
    }

    fn variables(&self) -> StableVariableIterator {
        self.model.variables()
    }

    fn communicator(&self) -> Option<Arc<Communicator>> {
        self.model.communicator()
    }

    fn prepare_for_training(&self) {
        self.model.prepare_for_training();
    }

    fn clip_grad_norm(&self, max_grad_norm: f64) {
        self.model.clip_grad_norm(max_grad_norm);
    }

    fn max_context_length(&self) -> usize {
        self.model.max_context_length()
    }

    fn shutdown(&self) {
        self.model.shutdown();
    }

    fn convert(&self, state_dict: Option<HashMap<String, Tensor>>) -> HashMap<String, Tensor> {
        self.model.convert(state_dict)
    }
}
