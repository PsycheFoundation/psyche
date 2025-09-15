use psyche_core::{Barrier, BatchId, ClosedInterval, LearningRateSchedule, OptimizerDefinition};
use psyche_modeling::{
    Batch, BatchData, BatchDataGPU, CausalLM, NopBarrier, ParallelModels, PythonCausalLM,
};
use pyo3::exceptions::PyRuntimeError;
use pyo3::prelude::*;
use pyo3_tch::{PyTensor, wrap_tch_err};
use std::{
    ops::Deref,
    sync::{Arc, RwLock},
    time::Duration,
};
use sysinfo::{Pid, System};
use tokio_util::sync::CancellationToken;
use tracing::trace;

#[pyfunction]
fn add_one(tensor: PyTensor) -> PyResult<PyTensor> {
    let tensor = tensor.f_add_scalar(1.0).map_err(wrap_tch_err)?;
    Ok(PyTensor(tensor))
}

#[pyfunction]
fn start_process_watcher(pid: usize, duration: Duration) -> PyResult<()> {
    std::thread::spawn(move || {
        loop {
            std::thread::sleep(duration);
            let mut system = System::new_all();
            if !system.refresh_process(Pid::from(pid)) {
                println!("Parent process {pid} gone, exiting");
                system
                    .process(Pid::from_u32(std::process::id()))
                    .unwrap()
                    .kill();
            }
        }
    });
    Ok(())
}

#[pyclass]

pub struct Trainer {
    trainer: RwLock<Option<psyche_modeling::LocalTrainer>>,
    cancel: CancellationToken,
}

#[pyclass]
pub struct DistroResult {
    #[pyo3(get)]
    pub sparse_idx: PyObject,
    #[pyo3(get)]
    pub sparse_val: PyObject,
    #[pyo3(get)]
    pub xshape: Vec<i64>,
    #[pyo3(get)]
    pub totalk: i64,
}

#[pymethods]
impl DistroResult {
    #[new]
    fn new(sparse_idx: PyObject, sparse_val: PyObject, xshape: Vec<i64>, totalk: i64) -> Self {
        Self {
            sparse_idx,
            sparse_val,
            xshape,
            totalk,
        }
    }
}

impl DistroResult {
    pub fn to_native(
        py: Python<'_>,
        distro_results: Option<Vec<Vec<Py<Self>>>>,
    ) -> PyResult<Option<Vec<Vec<psyche_modeling::DistroResult>>>> {
        match distro_results {
            Some(distro_results) => {
                let mut ret = vec![];
                for x in distro_results {
                    let mut vec = vec![];
                    for y in x {
                        let borrowed = y.borrow(py);
                        let sparse_idx: PyTensor = borrowed.sparse_idx.extract(py)?;
                        let sparse_val: PyTensor = borrowed.sparse_val.extract(py)?;
                        vec.push(psyche_modeling::DistroResult {
                            sparse_idx: sparse_idx.0,
                            sparse_val: sparse_val.0,
                            xshape: borrowed.xshape.clone(),
                            totalk: borrowed.totalk,
                            stats: None,
                        });
                    }
                    ret.push(vec);
                }
                Ok(Some(ret))
            }
            None => Ok(None),
        }
    }
}

#[pymethods]
impl Trainer {
    #[new]
    pub fn new(
        device: i32,
        causal_lm: PyObject,
        lr_scheduler_json: &str,
        optimizer_json: &str,
        config_json: &str,
        micro_batch_size: usize,
        grad_accum_in_fp32: bool,
    ) -> PyResult<Self> {
        let device = tch::Device::from_c_int(device);
        let config: serde_json::Value = serde_json::from_str(config_json)
            .map_err(|err| PyRuntimeError::new_err(format!("{err}")))?;
        let models = vec![
            Box::new(PythonCausalLM::from_python(causal_lm, device, config)) as Box<dyn CausalLM>,
        ];

        let lr_scheduler: LearningRateSchedule = serde_json::from_str(lr_scheduler_json)
            .map_err(|err| PyRuntimeError::new_err(format!("{err}")))?;
        let optimizer: OptimizerDefinition = serde_json::from_str(optimizer_json)
            .map_err(|err| PyRuntimeError::new_err(format!("{err}")))?;

        let trainer = psyche_modeling::LocalTrainer::new(
            ParallelModels {
                models,
                barrier: Arc::new(NopBarrier) as Arc<dyn Barrier>,
                data_parallel: None,
            },
            lr_scheduler,
            optimizer,
            micro_batch_size,
            None,
            grad_accum_in_fp32,
        );

        Ok(Self {
            trainer: RwLock::new(Some(trainer)),
            cancel: CancellationToken::new(),
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub fn train(
        self_: PyRef<'_, Self>,
        step: u32,
        zero_optim: bool,
        batch_id: (u64, u64),
        input_ids: PyTensor,
        labels: Option<PyTensor>,
        position_ids: Option<PyTensor>,
        sequence_lengths: Option<Vec<Vec<i32>>>,
        warmup_lr_between: Option<(u32, u32)>,
        prev_self_distro_results: Option<Vec<Vec<Py<DistroResult>>>>,
    ) -> PyResult<(Option<Vec<DistroResult>>, f32)> {
        trace!("Python extension train() for step {step}");
        let trainer = self_.trainer.write().unwrap().take().unwrap();
        let id = BatchId(ClosedInterval::new(batch_id.0, batch_id.1));
        let cancel = self_.cancel.clone();
        let prev_self_distro_results =
            DistroResult::to_native(self_.py(), prev_self_distro_results)?;
        let output = self_
            .py()
            .allow_threads(move || {
                trainer.train(
                    step,
                    Batch {
                        id,
                        data: BatchData::GPU(BatchDataGPU {
                            input_ids: input_ids.deref().shallow_clone(),
                            labels: labels.map(|x| x.deref().shallow_clone()),
                            position_ids: position_ids.map(|x| x.deref().shallow_clone()),
                            sequence_lengths,
                        }),
                    },
                    warmup_lr_between,
                    zero_optim,
                    vec![],
                    prev_self_distro_results,
                    cancel,
                )
            })
            .unwrap();
        *self_.trainer.write().unwrap() = Some(match output.trainer {
            psyche_modeling::Trainer::Local(local_trainer) => local_trainer,
            _ => unreachable!("got a distributed trainer in local training mode"),
        });

        let results: Option<Result<Vec<DistroResult>, PyErr>> =
            output.distro_results.map(|distro_results| {
                distro_results
                    .into_iter()
                    .map(|result| {
                        Ok(DistroResult::new(
                            PyTensor(result.sparse_idx)
                                .into_pyobject(self_.py())?
                                .unbind(),
                            PyTensor(result.sparse_val)
                                .into_pyobject(self_.py())?
                                .unbind(),
                            result.xshape,
                            result.totalk,
                        ))
                    })
                    .collect()
            });
        Ok((results.transpose()?, output.loss))
    }

    pub fn optimize(
        self_: PyRef<'_, Self>,
        step: u32,
        warmup_lr_between: Option<(u32, u32)>,
        distro_results: Option<Vec<Vec<Py<DistroResult>>>>,
    ) -> PyResult<()> {
        trace!("Python extension optimize() for step {step}");
        let trainer = self_.trainer.write().unwrap().take().unwrap();
        let distro_results = DistroResult::to_native(self_.py(), distro_results)?;
        let output = self_
            .py()
            .allow_threads(move || trainer.optimize(step, warmup_lr_between, distro_results))
            .unwrap();
        *self_.trainer.write().unwrap() = Some(output);
        Ok(())
    }

    pub fn extract(self_: PyRef<'_, Self>) -> PyResult<()> {
        let trainer = self_.trainer.write().unwrap().take();
        if let Some(mut trainer) = trainer {
            let trainer = self_.py().allow_threads(move || {
                let _ = trainer.extract();
                trainer
            });
            *self_.trainer.write().unwrap() = Some(trainer);
        }
        Ok(())
    }
}

#[pymodule]
#[pyo3(name = "_psyche_ext")]
pub fn psyche(py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    py.import("torch")?;
    m.add_function(wrap_pyfunction!(add_one, m)?)?;
    m.add_function(wrap_pyfunction!(start_process_watcher, m)?)?;
    m.add_class::<Trainer>()?;
    m.add_class::<DistroResult>()?;
    Ok(())
}

pub fn load_module() {
    pyo3::append_to_inittab!(psyche);
}
