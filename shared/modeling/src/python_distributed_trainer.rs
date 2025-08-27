use crate::{
    ApplyDistroResultError, Batch, BatchData, CausalLM, Communicator, EosToks, LocalTrainer,
    ParallelModels, PythonDistributedCausalLM, ReduceType, StableVariableIterator,
    TorchDistributedCommunicator, TrainOutput, Trainer, TrainerThreadCommunicationError,
    trainer::{BatchDataCPU, DistroResults},
};

use psyche_core::{Barrier, CancelledBarrier, LearningRateSchedule, OptimizerDefinition};
use pyo3::{PyErr, PyResult};
use std::{collections::HashMap, sync::Arc};
use tch::{Device, Kind, Tensor};
use thiserror::Error;
use tokio_util::sync::CancellationToken;
use tracing::{debug, trace};

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
            "lr_scheduler": lr_scheduler,
            "optimizer": optimizer,
            "micro_batch_size": micro_batch_size,
            "grad_accum_in_fp32": grad_accum_in_fp32
        });

        comm.set("hyperparameters", &hyperparameters.to_string())?;

        let device = model.device();
        let local = Box::new(LocalTrainer::new(
            ParallelModels {
                models: vec![Box::new(model) as Box<dyn CausalLM>],
                barrier: Arc::new(NopBarrier) as Arc<dyn Barrier>,
                data_parallel: None,
            },
            lr_scheduler,
            optimizer,
            micro_batch_size,
            stats,
            grad_accum_in_fp32,
        ));

        Ok(Self {
            local,
            comm,
            device,
            iteration: 0,
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
            debug!(
                "Checking batch padding: original batch size = {}, world_size = {}",
                original_batch_size, world_size
            );

            data = self.pad_batch_for_fsdp(data, world_size)?;

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

        trace!("Sending train operation to Python clients: {}", operation);

        self.comm
            .set(&self.iteration.to_string(), &operation.to_string())?;

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

        let loss: f32 = loss.try_into().unwrap();

        let loss = loss / std::cmp::min(original_batch_size, self.comm.size()) as f32;

        trace!("Train operation complete on all Python clients");

        Ok(TrainOutput {
            trainer: Self {
                local: match ret.trainer {
                    Trainer::Local(local_trainer) => Box::new(local_trainer),
                    Trainer::PythonDistributed(_) => unreachable!(),
                },
                comm: self.comm,
                device: self.device,
                iteration: self.iteration + 1,
            }
            .into(),
            loss,
            ..ret
        })
    }

    fn pad_batch_for_fsdp(
        &self,
        mut batch: Batch,
        world_size: usize,
    ) -> Result<Batch, TrainerThreadCommunicationError> {
        match &mut batch.data {
            BatchData::CPU(cpu_data) => {
                let current_batch_size = cpu_data.len();
                let remainder = current_batch_size % world_size;

                if remainder != 0 {
                    let padding_needed = world_size - remainder;
                    debug!(
                        "FSDP Padding: Batch size {} not divisible by world_size {}. Adding {} padding samples.",
                        current_batch_size, world_size, padding_needed
                    );

                    // Get sequence length from the first sample
                    let seq_len = if !cpu_data.is_empty() {
                        cpu_data[0].input_ids.len()
                    } else {
                        return Ok(batch);
                    };

                    // Create padding samples with labels set to -100
                    for _ in 0..padding_needed {
                        let padding_sample = BatchDataCPU {
                            // Use 0 as padding token ID
                            input_ids: vec![0; seq_len],
                            // Set labels to -100 so they're ignored in loss computation
                            labels: Some(vec![-100; seq_len]),
                            position_ids: cpu_data[0]
                                .position_ids
                                .as_ref()
                                .map(|_| vec![0; seq_len]),
                            sequence_lengths: cpu_data[0]
                                .sequence_lengths
                                .as_ref()
                                .map(|sl| vec![0; sl.len()]),
                        };
                        cpu_data.push(padding_sample);
                    }
                }
            }
            BatchData::GPU(_) => {
                return Err(TrainerThreadCommunicationError::PaddingBatch);
            }
        }

        Ok(batch)
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

        trace!(
            "Sending optimize operation to Python clients: {}",
            operation
        );

        self.comm
            .set(&self.iteration.to_string(), &operation.to_string())?;

        // barrier to ensure everyone has seen the broadcast
        let dummy = Tensor::zeros([], (Kind::Float, self.device));
        self.comm.all_reduce(&dummy, ReduceType::Sum)?;

        if results_len > 0 {
            self.broadcast_distro_results(distro_results.as_ref().unwrap())?;
        }

        let result = self
            .local
            .optimize(step, warmup_lr_between, distro_results)
            .map(|x| Self {
                local: Box::new(x),
                comm: self.comm,
                iteration: self.iteration + 1,
                device: self.device,
            });

        trace!("Optimize operation complete on all Python clients");

        result
    }

    pub fn extract(&mut self) -> Result<HashMap<String, Tensor>, TrainerThreadCommunicationError> {
        let operation = serde_json::json!({
            "operation": "extract",
        });

        trace!("Sending extract operation to Python clients: {}", operation);

        self.comm
            .set(&self.iteration.to_string(), &operation.to_string())?;

        // barrier to ensure everyone has seen the broadcast
        let dummy = Tensor::zeros([], (Kind::Float, self.device));
        self.comm.all_reduce(&dummy, ReduceType::Sum)?;

        self.iteration += 1;
        let result = self.local.extract();
        trace!("Extract operation complete on all Python clients");
        result
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
        position_ids: Option<&Tensor>,
        sequence_lengths: Option<&Vec<Vec<i32>>>,
        num_logits_to_keep: Option<i64>,
        loss_scale: Option<f64>,
    ) -> (Tensor, Option<Tensor>) {
        self.local.forward(
            x,
            labels,
            position_ids,
            sequence_lengths,
            num_logits_to_keep,
            loss_scale,
        )
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

    fn max_context_length(&self) -> usize {
        self.local.max_context_length()
    }
}
