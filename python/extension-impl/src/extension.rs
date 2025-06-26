use psyche_core::{Barrier, BatchId, ClosedInterval, LearningRateSchedule, OptimizerDefinition};
use psyche_modeling::{Batch, BatchData, CausalLM, NopBarrier, ParallelModels, PythonCausalLM};
use pyo3::exceptions::PyRuntimeError;
use pyo3::prelude::*;
use pyo3_tch::{wrap_tch_err, PyTensor};
use std::{cell::Cell, ops::Deref, sync::Arc, time::Duration};
use sysinfo::{Pid, System};
use tokio_util::sync::CancellationToken;

#[pyfunction]
fn add_one(tensor: PyTensor) -> PyResult<PyTensor> {
    let tensor = tensor.f_add_scalar(1.0).map_err(wrap_tch_err)?;
    Ok(PyTensor(tensor))
}

#[pyfunction]
fn start_process_watcher(pid: usize, duration: Duration) -> PyResult<()> {
    std::thread::spawn(move || loop {
        std::thread::sleep(duration);
        let mut system = System::new_all();
        if !system.refresh_process(Pid::from(pid)) {
            println!("Parent process {pid} gone, exiting");
            system
                .process(Pid::from_u32(std::process::id()))
                .unwrap()
                .kill();
        }
    });
    Ok(())
}

#[pyclass]
pub struct Trainer {
    trainer: Cell<Option<psyche_modeling::LocalTrainer>>,
    cancel: CancellationToken,
}

#[pyclass]
#[derive(Clone)]
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
        distro_results: Option<Vec<Vec<Self>>>,
    ) -> PyResult<Option<Vec<Vec<psyche_modeling::DistroResult>>>> {
        match distro_results {
            Some(distro_results) => {
                let mut ret = vec![];
                for x in distro_results {
                    let mut vec = vec![];
                    for y in x {
                        let sparse_idx: PyTensor = y.sparse_idx.extract(py)?;
                        let sparse_val: PyTensor = y.sparse_val.extract(py)?;
                        vec.push(psyche_modeling::DistroResult {
                            sparse_idx: sparse_idx.0,
                            sparse_val: sparse_val.0,
                            xshape: y.xshape,
                            totalk: y.totalk,
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
            .map_err(|err| PyRuntimeError::new_err(format!("{}", err)))?;
        let models = vec![Box::new(PythonCausalLM::from_python(
            causal_lm,
            device.clone(),
            config,
        )) as Box<dyn CausalLM>];

        let lr_scheduler: LearningRateSchedule = serde_json::from_str(lr_scheduler_json)
            .map_err(|err| PyRuntimeError::new_err(format!("{}", err)))?;
        let optimizer: OptimizerDefinition = serde_json::from_str(optimizer_json)
            .map_err(|err| PyRuntimeError::new_err(format!("{}", err)))?;

        let trainer = psyche_modeling::LocalTrainer::new(
            ParallelModels {
                models,
                barrier: Arc::new(NopBarrier::new()) as Arc<dyn Barrier>,
                data_parallel: None,
            },
            lr_scheduler,
            optimizer,
            micro_batch_size,
            None,
            grad_accum_in_fp32,
        );

        Ok(Self {
            trainer: Cell::new(Some(trainer)),
            cancel: CancellationToken::new(),
        })
    }

    pub fn train(
        self_: PyRef<'_, Self>,
        step: u32,
        batch_id: (u64, u64),
        batch_data: PyTensor,
        zero_optim: bool,
        warmup_lr_between: Option<(u32, u32)>,
        prev_self_distro_results: Option<Vec<Vec<DistroResult>>>,
    ) -> PyResult<Option<Vec<DistroResult>>> {
        let trainer = self_.trainer.take().unwrap();
        let id = BatchId(ClosedInterval::new(batch_id.0, batch_id.1));
        let data = BatchData::GPU(batch_data.deref().shallow_clone());
        let cancel = self_.cancel.clone();
        let prev_self_distro_results =
            DistroResult::to_native(self_.py(), prev_self_distro_results)?;
        let output = self_
            .py()
            .allow_threads(move || {
                trainer.train(
                    step,
                    Batch { id, data },
                    warmup_lr_between,
                    zero_optim,
                    vec![],
                    prev_self_distro_results,
                    cancel,
                )
            })
            .unwrap();
        self_.trainer.set(Some(match output.trainer {
            psyche_modeling::Trainer::Local(local_trainer) => local_trainer,
            _ => unreachable!(),
        }));
        Ok(output.distro_results.map(|distro_results| {
            distro_results
                .into_iter()
                .map(|result| DistroResult {
                    sparse_idx: PyTensor(result.sparse_idx).into_py(self_.py()),
                    sparse_val: PyTensor(result.sparse_val).into_py(self_.py()),
                    xshape: result.xshape,
                    totalk: result.totalk,
                })
                .collect()
        }))
    }

    pub fn optimize(
        self_: PyRef<'_, Self>,
        step: u32,
        warmup_lr_between: Option<(u32, u32)>,
        distro_results: Option<Vec<Vec<DistroResult>>>,
    ) -> PyResult<()> {
        let trainer = self_.trainer.take().unwrap();
        let distro_results = DistroResult::to_native(self_.py(), distro_results)?;
        let output = self_
            .py()
            .allow_threads(move || trainer.optimize(step, warmup_lr_between, distro_results))
            .unwrap();
        self_.trainer.set(Some(output));
        Ok(())
    }

    pub fn extract(self_: PyRef<'_, Self>) -> PyResult<()> {
        let trainer = self_.trainer.take();
        if let Some(mut trainer) = trainer {
            let trainer = self_.py().allow_threads(move || {
                let _ = trainer.extract();
                trainer
            });
            self_.trainer.set(Some(trainer));
        }
        Ok(())
    }
}

#[pymodule]
#[pyo3(name = "_psyche_ext")]
pub fn psyche(py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    py.import_bound("torch")?;
    m.add_function(wrap_pyfunction!(add_one, m)?)?;
    m.add_function(wrap_pyfunction!(start_process_watcher, m)?)?;
    m.add_class::<Trainer>()?;
    m.add_class::<DistroResult>()?;
    Ok(())
}

#[cfg(not(feature = "python-extension"))]
pub fn load_module() {
    pyo3::append_to_inittab!(psyche);
}
