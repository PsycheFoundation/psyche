use crate::{
    unsharded_cpu_variables, AllReduce, CausalLM, Communicator, CommunicatorId, CudaSynchronize,
    Distro, DistroResult, EosToks, Fp32GradientAccumulator, Optimizer, ReduceType,
    StableVariableIterator,
};
use anyhow::{Error, Result};
use psyche_core::{Barrier, BatchId, LearningRateSchedule, OptimizerDefinition};
use std::{
    collections::HashMap,
    ops::ControlFlow,
    sync::{mpsc, Arc},
    time::Instant,
};
use tch::{Device, Kind, Tensor};
use thiserror::Error;
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, trace, warn};

#[cfg(feature = "parallelism")]
use tch::CNCCL;

pub struct ParallelModels {
    pub models: Vec<Box<dyn CausalLM>>,
    pub barrier: Arc<dyn Barrier>,
    pub data_parallel: Option<Vec<DataParallel>>,
}

pub type DistroResults = Vec<DistroResult>;

#[derive(Debug)]
pub enum BatchData {
    CPU(Vec<Vec<i32>>),
    GPU(Tensor),
}

impl BatchData {
    pub fn size(&self) -> usize {
        match self {
            BatchData::CPU(items) => items.len(),
            BatchData::GPU(tensor) => tensor.size()[0] as usize,
        }
    }

    pub fn gpu(self, device: Device) -> Self {
        Self::GPU(
            match self {
                BatchData::CPU(cpu) => Tensor::from_slice2(&cpu).to_kind(Kind::Int64),
                BatchData::GPU(tensor) => tensor,
            }
            .to(device),
        )
    }
}

impl Clone for BatchData {
    fn clone(&self) -> Self {
        match self {
            Self::CPU(cpu) => Self::CPU(cpu.clone()),
            Self::GPU(gpu) => Self::GPU(gpu.shallow_clone()),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Batch {
    pub id: BatchId,
    pub data: BatchData,
}

impl Batch {
    pub fn gpu(self, device: Device) -> Self {
        Self {
            id: self.id,
            data: self.data.gpu(device),
        }
    }
}

pub struct TrainOutput {
    pub batch_id: BatchId,
    pub trainer: Trainer,
    pub loss: f32,
    pub step: u32,
    pub nonce: u32,
    pub distro_results: Option<DistroResults>,
    pub cancelled: bool,
}

#[derive(Clone, Debug)]
pub struct DataParallel {
    pub id: CommunicatorId,
    pub barrier: Arc<dyn Barrier>,
    pub rank: usize,
    pub world_size: usize,
}

enum ParallelAssignment {
    Train {
        batch: Batch,
        step: u32,
        warmup_lr_between: Option<(u32, u32)>,
        zero_optim: bool,
        #[allow(unused)]
        rollback: Vec<(u32, Vec<DistroResults>)>,
        cancel_training: CancellationToken,
        prev_self_distro_results: Option<Vec<DistroResults>>,
    },
    Optimize {
        distro_results: Option<Vec<DistroResults>>,
        step: u32,
        warmup_lr_between: Option<(u32, u32)>,
    },
    Forward {
        data: Tensor,
        labels: Option<Tensor>,
        num_logits_to_keep: Option<i64>,
        loss_scale: Option<f64>,
    },
    Extract,
}

#[derive(Debug)]
enum ParallelResult {
    Train {
        loss: f32,
        nonce: u32,
        cancelled: bool,
        distro_results: Option<DistroResults>,
    },
    Optimize,
    Forward {
        logits_and_loss: Option<(Tensor, Option<Tensor>)>,
    },
    Extract {
        variables: HashMap<String, Tensor>,
    },
}

#[derive(Debug)]
pub enum Trainer {
    Local(LocalTrainer),
    #[cfg(feature = "python")]
    PythonDistributed(crate::PythonDistributedTrainer),
}

impl Trainer {
    #[allow(clippy::too_many_arguments)]
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
        match self {
            Trainer::Local(local_trainer) => local_trainer.train(
                step,
                data,
                warmup_lr_between,
                zero_optim,
                rollback,
                prev_self_distro_results,
                cancel_training,
            ),
            #[cfg(feature = "python")]
            Trainer::PythonDistributed(python) => python.train(
                step,
                data,
                warmup_lr_between,
                zero_optim,
                rollback,
                prev_self_distro_results,
                cancel_training,
            ),
        }
    }

    pub fn optimize(
        self,
        step: u32,
        warmup_lr_between: Option<(u32, u32)>,
        distro_results: Option<Vec<DistroResults>>,
    ) -> Result<Self, ApplyDistroResultError> {
        match self {
            Trainer::Local(local_trainer) => local_trainer
                .optimize(step, warmup_lr_between, distro_results)
                .map(|x| x.into()),
            #[cfg(feature = "python")]
            Trainer::PythonDistributed(python) => python
                .optimize(step, warmup_lr_between, distro_results)
                .map(|x| x.into()),
        }
    }

    pub fn extract(&mut self) -> Result<HashMap<String, Tensor>, TrainerThreadCommunicationError> {
        match self {
            Trainer::Local(local_trainer) => local_trainer.extract(),
            #[cfg(feature = "python")]
            Trainer::PythonDistributed(python) => python.extract(),
        }
    }

    pub fn get_lr(
        lr_scheduler: &LearningRateSchedule,
        step: u32,
        warmup_lr_between: Option<(u32, u32)>,
    ) -> f64 {
        let factor = match warmup_lr_between {
            Some((start, end)) => match step >= start && step <= end {
                true => (step - start) as f64 / (end - start) as f64,
                false => 1.,
            },
            None => 1.,
        };
        lr_scheduler.get_lr(step) * factor
    }

    pub fn quantize_results(results: &DistroResults) -> DistroResults {
        results
            .iter()
            .map(|x| DistroResult {
                sparse_idx: x.sparse_idx.copy(),
                sparse_val: Distro::quantize_nozeros_tensor_to_boolean_sign(&x.sparse_val),
                xshape: x.xshape.clone(),
                totalk: x.totalk,
                stats: x.stats.clone(),
            })
            .collect()
    }

    pub fn causal_lm(&self) -> &dyn CausalLM {
        match self {
            Trainer::Local(local_trainer) => local_trainer,
            #[cfg(feature = "python")]
            Trainer::PythonDistributed(python_distributed_trainer) => python_distributed_trainer,
        }
    }
}

impl From<LocalTrainer> for Trainer {
    fn from(value: LocalTrainer) -> Self {
        Self::Local(value)
    }
}

#[derive(Debug)]
pub struct LocalTrainer {
    models: Vec<(
        mpsc::Sender<ParallelAssignment>,
        mpsc::Receiver<ParallelResult>,
    )>,
    first_model_device: Device,
    barrier: Arc<dyn Barrier>,
    data_parallel: Option<Vec<DataParallel>>,
    is_dummy_model: bool,
}

#[derive(Debug, Error)]
pub enum TrainerThreadCommunicationError {
    #[error("Failed to send command to trainer thread; thread has dropped RX")]
    SendCommand,

    #[error("Failed to recv result from trainer thread; thread has dropped TX")]
    RecvResult,

    #[error("Got unexpected result from trainer thread: {0}")]
    UnexpectedResult(String),

    #[cfg(feature = "python")]
    #[error("Python error: {0}")]
    PythonError(#[from] pyo3::PyErr),
}

impl LocalTrainer {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        models: ParallelModels,
        lr_scheduler: LearningRateSchedule,
        optimizer: OptimizerDefinition,
        micro_batch_size: usize,
        stats: Option<u32>,
        grad_accum_in_fp32: bool,
    ) -> Self {
        let ParallelModels {
            models,
            barrier,
            data_parallel,
        } = models;

        assert!(!models.is_empty());
        let first_model_device = models[0].device();
        let is_dummy_model = models[0].is_dummy_model();

        let mut ret = Vec::with_capacity(models.len());

        let data_parallels = match &data_parallel {
            Some(data_parallel) => {
                assert_eq!(data_parallel.len(), models.len());
                data_parallel
                    .iter()
                    .map(|x| Some(x.clone()))
                    .collect::<Vec<_>>()
            }
            None => std::iter::repeat_n(None, models.len()).collect(),
        };

        for (index, (model, data_parallel)) in models.into_iter().zip(data_parallels).enumerate() {
            let (assignment_tx, assignment_rx) = mpsc::channel();
            let (result_tx, result_rx) = mpsc::channel();
            ret.push((assignment_tx, result_rx));

            let optimizer = Optimizer::new(optimizer, model.as_ref());

            let barrier = barrier.clone();
            let data_parallel = data_parallel.clone();

            std::thread::spawn(move || {
                Self::model_thread(
                    model,
                    assignment_rx,
                    result_tx,
                    optimizer,
                    index,
                    micro_batch_size,
                    lr_scheduler,
                    barrier,
                    stats,
                    grad_accum_in_fp32,
                    data_parallel,
                )
            });
        }

        Self {
            models: ret,
            first_model_device,
            barrier,
            data_parallel,
            is_dummy_model,
        }
    }

    fn forward_backward(
        model: &mut dyn CausalLM,
        inputs: Tensor,
        barrier: &Arc<dyn Barrier>,
        loss_scale: Option<f64>,
    ) -> Result<Option<Tensor>> {
        let targets = inputs.copy();
        if barrier.wait().is_err() {
            return Ok(None);
        }
        let device = inputs.device();
        let (_, loss) = model.forward(&inputs, Some(&targets), None, loss_scale);
        let loss = loss.ok_or(Error::msg("No loss"))?;
        loss.backward();
        if device.is_cuda() {
            device.cuda_synchronize();
        }
        Ok(Some(loss.detach()))
    }

    fn forward(
        model: &mut dyn CausalLM,
        data: &Tensor,
        labels: Option<&Tensor>,
        barrier: &Arc<dyn Barrier>,
        num_logits_to_keep: Option<i64>,
        loss_scale: Option<f64>,
    ) -> Option<(Tensor, Option<Tensor>)> {
        let _guard = tch::no_grad_guard();
        if barrier.wait().is_err() {
            return None;
        }
        let device = model.device();
        let inputs = data.to(device);
        let labels = labels.map(|x| x.to(device));
        let device = inputs.device();
        let (logits, loss) =
            model.forward(&inputs, labels.as_ref(), num_logits_to_keep, loss_scale);
        if device.is_cuda() {
            device.cuda_synchronize();
        }
        Some((logits, loss.map(|x| x.detach())))
    }

    #[allow(clippy::too_many_arguments)]
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
        if !rollback.is_empty() {
            error!(
                "we have not implemented getting data from previous rounds. this should be impossible to hit.. this step is {step}, rollback passed is {:?}",
                rollback.iter().map(|(step, _)| step).collect::<Vec<_>>());
        }
        self.barrier.reset();
        for (tx, _) in &self.models {
            tx.send(ParallelAssignment::Train {
                batch: data.clone(),
                step,
                warmup_lr_between,
                zero_optim,
                rollback: rollback.clone(),
                prev_self_distro_results: prev_self_distro_results.clone(),
                cancel_training: cancel_training.clone(),
            })
            .map_err(|_| TrainerThreadCommunicationError::SendCommand)?;
        }

        let mut final_loss = 0.0;
        let mut final_distro_results = None;
        let mut final_cancelled = false;
        let mut final_nonce = 0;
        for (_, rx) in &self.models {
            match rx
                .recv()
                .map_err(|_| TrainerThreadCommunicationError::RecvResult)?
            {
                ParallelResult::Train {
                    loss,
                    distro_results,
                    cancelled,
                    nonce,
                } => {
                    if final_distro_results.is_none() {
                        final_distro_results = distro_results;
                        final_nonce = nonce;
                    }
                    final_cancelled = cancelled;
                    final_loss += loss;
                }
                weird => {
                    return Err(TrainerThreadCommunicationError::UnexpectedResult(format!(
                        "{:?}",
                        weird
                    )))
                }
            }
        }
        final_loss /= self.models.len() as f32;
        Ok(TrainOutput {
            batch_id: data.id,
            trainer: Trainer::Local(self),
            loss: final_loss,
            step,
            distro_results: final_distro_results,
            cancelled: final_cancelled,
            nonce: final_nonce,
        })
    }

    pub fn optimize(
        self,
        step: u32,
        warmup_lr_between: Option<(u32, u32)>,
        results: Option<Vec<DistroResults>>,
    ) -> Result<Self, ApplyDistroResultError> {
        self.barrier.reset();
        for (tx, _) in &self.models {
            tx.send(ParallelAssignment::Optimize {
                distro_results: results.clone(),
                step,
                warmup_lr_between,
            })
            .map_err(|_| ApplyDistroResultError::SendOptimize)?;
        }
        let start = Instant::now();
        for (_, rx) in &self.models {
            match rx.recv()? {
                ParallelResult::Optimize => {
                    trace!(
                        "ParallelResult::Optimize received in {}s",
                        (Instant::now() - start).as_secs_f32()
                    );
                }
                o => {
                    return Err(ApplyDistroResultError::RecievedWrongResultType(format!(
                        "{o:?}"
                    )))
                }
            }
        }
        Ok(self)
    }

    pub fn extract(&mut self) -> Result<HashMap<String, Tensor>, TrainerThreadCommunicationError> {
        self.barrier.reset();
        for (tx, _) in &self.models {
            tx.send(ParallelAssignment::Extract)
                .map_err(|_| TrainerThreadCommunicationError::SendCommand)?;
        }
        let mut extracted = HashMap::new();
        for (_, rx) in &self.models {
            match rx
                .recv()
                .map_err(|_| TrainerThreadCommunicationError::RecvResult)?
            {
                ParallelResult::Extract { variables } => {
                    if extracted.is_empty() && !variables.is_empty() {
                        extracted = variables;
                    }
                }
                result => {
                    return Err(TrainerThreadCommunicationError::UnexpectedResult(format!(
                        "{:?}",
                        result
                    )))
                }
            }
        }
        Ok(extracted)
    }

    // todo: refactor args into a struct
    #[allow(clippy::too_many_arguments)]
    fn model_thread(
        mut model: Box<dyn CausalLM>,
        assignment: mpsc::Receiver<ParallelAssignment>,
        submission: mpsc::Sender<ParallelResult>,
        mut optimizer: Optimizer,
        index: usize,
        micro_batch_size: usize,
        lr_scheduler: LearningRateSchedule,
        barrier: Arc<dyn Barrier>,
        optim_stats_every_n_steps: Option<u32>,
        grad_accum_in_fp32: bool,
        data_parallel_def: Option<DataParallel>,
    ) {
        #[allow(unused_mut)]
        let mut data_parallel: Option<(Arc<Communicator>, Arc<dyn Barrier>)> = None;

        #[cfg(feature = "parallelism")]
        if let Some(data_parallel_def) = data_parallel_def {
            let id = match data_parallel_def.id {
                CommunicatorId::NCCL(cstore) => cstore,
                _ => panic!("Wrong communicator type for model_thread"),
            };
            let comm = match CNCCL::new(
                id,
                data_parallel_def.rank as i64,
                data_parallel_def.world_size as i64,
                model.device(),
            ) {
                Ok(comm) => comm,
                Err(err) => {
                    error!("Error creating DP mesh: {err:#}");
                    return;
                }
            };
            data_parallel = Some((
                Arc::new(Communicator::NCCL(comm)),
                data_parallel_def.barrier,
            ))
        };

        #[cfg(not(feature = "parallelism"))]
        if data_parallel_def.is_some() {
            error!("DP with parallelism feature off");
            return;
        }

        if barrier.wait().is_err() {
            error!("Incorrect model_thread boot");
            return;
        }
        model.prepare_for_training();

        let mut grad_accum: Option<Fp32GradientAccumulator> = None;
        let mut nonce = 0;
        loop {
            match assignment.recv() {
                Ok(ParallelAssignment::Train {
                    batch,
                    step,
                    warmup_lr_between,
                    zero_optim,
                    rollback: _,
                    prev_self_distro_results,
                    cancel_training,
                }) => {
                    // this needs even more work for overlapped
                    // for (step, result) in rollback.iter().rev() {
                    //     // TODO freeze the optimizer and prevent this from modifying its internal state, methinks, right? or maybe save it and restore it later?
                    //     // we just want to have the same optimizer state wyhen we exit, save for the main operation (if not frozen. hmm)
                    //     let lr = lr_scheduler.get_lr(*step);
                    //     if optimize_step(&mut model, lr, &mut optimizer, Some(result), &barrier)
                    //         .is_break()
                    //     {
                    //         panic!("Failed to roll back.")
                    //     };
                    // }

                    debug!(batch_id=%batch.id, "model thread training on batch {}", batch.id);

                    let batch_size = batch.data.size();

                    let mut grad_accum_steps = batch_size / micro_batch_size;
                    if batch_size % micro_batch_size != 0 {
                        grad_accum_steps += 1;
                    }
                    if grad_accum_in_fp32 && grad_accum_steps != 1 && grad_accum.is_none() {
                        debug!("Allocating FP32 gradient accumulator");
                        grad_accum = Some(Fp32GradientAccumulator::new(model.as_ref()))
                    }
                    let grad_accum_divisor = grad_accum_steps as f64;

                    let micro_batches = match batch.data {
                        BatchData::CPU(data) => data
                            .chunks(micro_batch_size)
                            .map(|chunk| {
                                Tensor::from_slice2(chunk)
                                    .to(model.device())
                                    .to_kind(Kind::Int64)
                            })
                            .collect::<Vec<_>>(),
                        BatchData::GPU(tensor) => tensor
                            .to_kind(Kind::Int64)
                            .chunk(grad_accum_steps as i64, 0),
                    };
                    assert_eq!(micro_batches.len(), grad_accum_steps);

                    if let Some(grad_accum) = &mut grad_accum {
                        grad_accum.zero_grad();
                    }

                    let lr = Trainer::get_lr(&lr_scheduler, step, warmup_lr_between);
                    let prev_lr = match step {
                        0 => Trainer::get_lr(&lr_scheduler, 0, warmup_lr_between),
                        step => Trainer::get_lr(&lr_scheduler, step - 1, warmup_lr_between),
                    };

                    tracing::debug!(
                        lr = lr,
                        prev_lr = prev_lr,
                        step = step,
                        micro_batches = grad_accum_steps,
                        "Train begin"
                    );

                    match &mut optimizer {
                        Optimizer::Torch { optimizer, .. } => {
                            optimizer.zero_grad().unwrap();
                            if zero_optim {
                                tracing::warn!("Zeroing optimizing states not supported for AdamW");
                            }
                        }
                        Optimizer::Distro { optimizer, .. } => {
                            optimizer.zero_grad();
                            if zero_optim {
                                optimizer.zero_optim();
                                tracing::info!("Zeroed optimizer states");
                            }
                            match &prev_self_distro_results {
                                Some(_) => optimizer.error_correction(model.as_ref(), prev_lr),
                                None => {
                                    error!(
                                        "Got DisTrO train assignment, but null previous results"
                                    );
                                    return;
                                }
                            };
                        }
                        Optimizer::Null => {}
                    };

                    let mut loss = None;
                    let mut cancelled = false;
                    for (index, micro_batch) in micro_batches.into_iter().enumerate() {
                        if cancel_training.is_cancelled() {
                            cancelled = true;
                            barrier.cancel();
                            warn!("Aborting training upon request");
                            break;
                        }
                        match Self::forward_backward(
                            &mut *model,
                            micro_batch,
                            &barrier,
                            Some(grad_accum_divisor),
                        ) {
                            Ok(Some(batch_loss)) => match loss.as_mut() {
                                Some(loss) => *loss += batch_loss,
                                None => {
                                    loss = Some(batch_loss);
                                }
                            },
                            Ok(None) => {
                                // cancelled barrier catching race to on run_state
                                cancelled = true;
                                warn!("Aborting training, run state changed");
                                break;
                            }
                            Err(err) => {
                                error!("Train error: {err:#}");
                                return;
                            }
                        }
                        if let Some(grad_accum) = &mut grad_accum {
                            grad_accum.accumulate_gradients();
                        }
                        trace!(micro_batch = index, "Finished micro batch forward/backward");
                    }
                    if let Some(grad_accum) = &mut grad_accum {
                        grad_accum.apply_accumulation();
                    }

                    // reduce grads across DP ranks
                    if let Some((dp_comm, dp_barrier)) = &data_parallel {
                        dp_barrier.wait().unwrap(); // cannot cancel dp
                        match &mut grad_accum {
                            Some(grad_accum) => grad_accum.reduce_gradients(dp_comm.clone()),
                            None => {
                                for variable in model.variables() {
                                    let mut grad = variable.local_tensor().grad();
                                    if grad.defined() {
                                        // reduce grads in fp32
                                        let mut fp32_grad = grad.to_kind(Kind::Float);
                                        fp32_grad
                                            .all_reduce(&Some(dp_comm.clone()), ReduceType::Mean);
                                        grad.copy_(&fp32_grad.to_kind(grad.kind()));
                                    }
                                }
                            }
                        }
                        if let Some(loss) = loss.as_mut() {
                            loss.all_reduce(&Some(dp_comm.clone()), ReduceType::Mean);
                        }
                        dp_barrier.wait().unwrap(); // cannot cancel dp
                    }

                    let distro_results = match cancelled {
                        false => match &mut optimizer {
                            Optimizer::Torch {
                                optimizer: _,
                                clip_grad_norm: _,
                            } => None,
                            Optimizer::Distro {
                                optimizer,
                                clip_grad_norm,
                                quantize_1bit: _,
                            } => {
                                let clipped = match clip_grad_norm {
                                    Some(clip_grad_norm) => match barrier.wait() {
                                        Ok(_) => {
                                            model.clip_grad_norm(*clip_grad_norm as f64);
                                            barrier.wait().is_ok()
                                        }
                                        Err(_) => false,
                                    },
                                    None => true,
                                };
                                if clipped {
                                    let ret = optimizer.generate(
                                        model.as_ref(),
                                        &prev_self_distro_results.unwrap_or_default(),
                                        prev_lr,
                                        lr,
                                        optim_stats_every_n_steps
                                            .map(|stats| step % stats == 0)
                                            .unwrap_or(false),
                                    );
                                    // just need results from one of the ranks
                                    match index == 0 {
                                        true => Some(ret),
                                        false => None,
                                    }
                                } else {
                                    cancelled = true;
                                    None
                                }
                            }
                            Optimizer::Null => None,
                        },
                        true => None,
                    };
                    if submission
                        .send(ParallelResult::Train {
                            loss: match loss {
                                Some(loss) => loss.try_into().unwrap_or(0.),
                                None => 0.,
                            },
                            distro_results,
                            cancelled,
                            nonce,
                        })
                        .is_err()
                    {
                        return;
                    }

                    nonce += 1;

                    // for (_, result) in rollback.iter() {
                    //     // TODO freeze the optimizer and prevent this from modifying its internal state, methinks, right? or maybe save it and restore it later?
                    //     // we just want to have the same optimizer state wyhen we exit, save for the main operation (if not frozen. hmm)
                    //     if optimize_step(&mut model, lr, &mut optimizer, Some(result), &barrier)
                    //         .is_break()
                    //     {
                    //         panic!("Failed to roll forwards.")
                    //     };
                    // }
                }
                Ok(ParallelAssignment::Optimize {
                    distro_results,
                    step,
                    warmup_lr_between,
                }) => {
                    let lr = Trainer::get_lr(&lr_scheduler, step, warmup_lr_between);
                    if optimize_step(
                        &mut model,
                        lr,
                        &mut optimizer,
                        distro_results.as_ref(),
                        &barrier,
                    )
                    .is_break()
                    {
                        return;
                    }
                    if submission.send(ParallelResult::Optimize).is_err() {
                        return;
                    }
                }
                Ok(ParallelAssignment::Forward {
                    data,
                    labels,
                    num_logits_to_keep,
                    loss_scale,
                }) => {
                    let logits_and_loss = Self::forward(
                        &mut *model,
                        &data,
                        labels.as_ref(),
                        &barrier,
                        num_logits_to_keep,
                        loss_scale,
                    );
                    if submission
                        .send(ParallelResult::Forward { logits_and_loss })
                        .is_err()
                    {
                        return;
                    }
                }
                Ok(ParallelAssignment::Extract) => {
                    match unsharded_cpu_variables(model.as_ref(), model.communicator()) {
                        Ok(variables) => {
                            if submission
                                .send(ParallelResult::Extract { variables })
                                .is_err()
                            {
                                return;
                            }
                        }
                        Err(err) => {
                            error!("Unexpected error in extract: {err:#}");
                            return;
                        }
                    }
                }
                Err(_) => {
                    return;
                }
            }
        }
    }

    pub fn device(&self) -> &Device {
        &self.first_model_device
    }

    pub fn data_parallel_barrier(&self) {
        if let Some(data_parallel) = &self.data_parallel {
            for dp in data_parallel {
                dp.barrier.reset();
            }
        }
    }

    pub fn data_parallel_world_size(&self) -> usize {
        self.data_parallel
            .as_ref()
            .and_then(|x| x.first().map(|y| y.world_size))
            .unwrap_or(1)
    }
}

#[derive(Error, Debug)]
pub enum ApplyDistroResultError {
    #[error("failed to send optimization to trainer thread - trainer thread RX is closed")]
    SendOptimize,

    #[error("failed to recv optimization result from trainer thread: {0}")]
    ReceiveResult(#[from] std::sync::mpsc::RecvError),

    #[error("recieved wrong result type from trainer thread. expected Optimize, got {0:?}")]
    RecievedWrongResultType(String),

    #[error("apply thread crashed")]
    ThreadCrashed,

    #[cfg(feature = "python")]
    #[error("Python error: {0}")]
    PythonError(#[from] pyo3::PyErr),
}

impl CausalLM for LocalTrainer {
    fn forward(
        &mut self,
        x: &Tensor,
        labels: Option<&Tensor>,
        num_logits_to_keep: Option<i64>,
        loss_scale: Option<f64>,
    ) -> (Tensor, Option<Tensor>) {
        self.barrier.reset();
        for (tx, _) in &self.models {
            tx.send(ParallelAssignment::Forward {
                data: x.shallow_clone(),
                labels: labels.map(|y| y.shallow_clone()),
                num_logits_to_keep,
                loss_scale,
            })
            .expect("Error getting result from forward");
        }
        let mut final_logits_and_loss = None;
        for (_, rx) in &self.models {
            match rx.recv() {
                Ok(ParallelResult::Forward { logits_and_loss }) => {
                    if final_logits_and_loss.is_none() {
                        final_logits_and_loss = logits_and_loss;
                    }
                }
                _ => panic!("Got unexpected forward result"),
            }
        }
        final_logits_and_loss.expect("No forward logits and loss")
    }

    fn bos_token_id(&self) -> Option<i64> {
        None
    }

    fn eos_token_ids(&self) -> Option<EosToks> {
        None
    }

    fn device(&self) -> tch::Device {
        self.first_model_device
    }

    fn variables(&self) -> StableVariableIterator {
        unimplemented!()
    }

    fn communicator(&self) -> Option<Arc<Communicator>> {
        unimplemented!()
    }

    fn prepare_for_training(&mut self) {}

    fn clip_grad_norm(&mut self, _max_grad_norm: f64) {}

    fn is_dummy_model(&self) -> bool {
        self.is_dummy_model
    }
}

fn optimize_step(
    model: &mut Box<dyn CausalLM>,
    lr: f64,
    optimizer: &mut Optimizer,
    distro_results: Option<&Vec<Vec<DistroResult>>>,
    barrier: &Arc<dyn Barrier>,
) -> ControlFlow<()> {
    match optimizer {
        Optimizer::Torch {
            optimizer,
            clip_grad_norm,
        } => {
            optimizer.set_learning_rate(lr).unwrap();
            if let Some(clip_grad_norm) = clip_grad_norm {
                if barrier.wait().is_err() {
                    return ControlFlow::Break(());
                }
                model.clip_grad_norm(*clip_grad_norm as f64);
                if barrier.wait().is_err() {
                    return ControlFlow::Break(());
                }
            }
            optimizer.step().unwrap();
            optimizer.zero_grad().unwrap();
        }
        Optimizer::Distro { optimizer, .. } => match distro_results {
            Some(results) => {
                if !results.is_empty() {
                    trace!("Applying {} DisTrO gradients", results.len());
                    if barrier.wait().is_err() {
                        return ControlFlow::Break(());
                    }
                    optimizer.apply(model.as_ref(), results, lr);
                    if barrier.wait().is_err() {
                        return ControlFlow::Break(());
                    }
                } else {
                    warn!("Empty DisTrO gradients, model parameters will not be updated");
                }
            }
            None => {
                error!("Got DisTrO optimizer assignment, but no results");
                return ControlFlow::Break(());
            }
        },
        Optimizer::Null => (),
    };
    ControlFlow::Continue(())
}

impl CausalLM for Trainer {
    fn forward(
        &mut self,
        x: &Tensor,
        labels: Option<&Tensor>,
        num_logits_to_keep: Option<i64>,
        loss_scale: Option<f64>,
    ) -> (Tensor, Option<Tensor>) {
        match self {
            Trainer::Local(local_trainer) => {
                local_trainer.forward(x, labels, num_logits_to_keep, loss_scale)
            }
            #[cfg(feature = "python")]
            Trainer::PythonDistributed(python_distributed_trainer) => {
                python_distributed_trainer.forward(x, labels, num_logits_to_keep, loss_scale)
            }
        }
    }

    fn bos_token_id(&self) -> Option<i64> {
        match self {
            Trainer::Local(local_trainer) => local_trainer.bos_token_id(),
            #[cfg(feature = "python")]
            Trainer::PythonDistributed(python_distributed_trainer) => {
                python_distributed_trainer.bos_token_id()
            }
        }
    }

    fn eos_token_ids(&self) -> Option<EosToks> {
        match self {
            Trainer::Local(local_trainer) => local_trainer.eos_token_ids(),
            #[cfg(feature = "python")]
            Trainer::PythonDistributed(python_distributed_trainer) => {
                python_distributed_trainer.eos_token_ids()
            }
        }
    }

    fn device(&self) -> Device {
        match self {
            Trainer::Local(local_trainer) => *local_trainer.device(),
            #[cfg(feature = "python")]
            Trainer::PythonDistributed(python_distributed_trainer) => {
                python_distributed_trainer.device()
            }
        }
    }

    fn variables(&self) -> StableVariableIterator {
        match self {
            Trainer::Local(local_trainer) => local_trainer.variables(),
            #[cfg(feature = "python")]
            Trainer::PythonDistributed(python_distributed_trainer) => {
                python_distributed_trainer.variables()
            }
        }
    }

    fn communicator(&self) -> Option<Arc<Communicator>> {
        match self {
            Trainer::Local(local_trainer) => local_trainer.communicator(),
            #[cfg(feature = "python")]
            Trainer::PythonDistributed(python_distributed_trainer) => {
                python_distributed_trainer.communicator()
            }
        }
    }

    fn prepare_for_training(&mut self) {
        match self {
            Trainer::Local(local_trainer) => local_trainer.prepare_for_training(),
            #[cfg(feature = "python")]
            Trainer::PythonDistributed(python_distributed_trainer) => {
                python_distributed_trainer.prepare_for_training()
            }
        }
    }

    fn clip_grad_norm(&mut self, max_grad_norm: f64) {
        match self {
            Trainer::Local(local_trainer) => local_trainer.clip_grad_norm(max_grad_norm),
            #[cfg(feature = "python")]
            Trainer::PythonDistributed(python_distributed_trainer) => {
                python_distributed_trainer.clip_grad_norm(max_grad_norm)
            }
        }
    }

    fn is_dummy_model(&self) -> bool {
        match self {
            Trainer::Local(local_trainer) => local_trainer.is_dummy_model(),
            #[cfg(feature = "python")]
            Trainer::PythonDistributed(python_distributed_trainer) => false,
        }
    }
}
