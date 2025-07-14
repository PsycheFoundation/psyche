use crate::{
    CausalLM, Communicator, ParallelismConfig, PretrainedSource, PythonCausalLM, ReduceType,
    StableVariableIterator,
    python_causal_lm::{PythonCausalLMError, PythonModelConfig},
};

use pyo3::{PyErr, PyResult, Python, prelude::*, types::PyDict};
use pyo3_tch::PyTensor;
use std::{
    process::{Child, Command},
    sync::Arc,
    thread::JoinHandle,
};
use tch::{Device, Tensor};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum PythonDistributedCausalLMError {
    #[error("Local deivce must be rank 0, instead got {0}")]
    LocalNotRankZero(usize),

    #[error("Local device not a CUDA device")]
    NonCUDADevice,

    #[error("Python error: {0}")]
    PythonError(#[from] PyErr),

    #[error("Sidecar spawn error: {0}")]
    SidecarSpawnError(#[from] std::io::Error),

    #[error("Local load error: {0}")]
    LocalLoadError(#[from] PythonCausalLMError),
}

#[derive(Debug, Clone)]
pub struct TorchDistributedCommunicator {
    store: PyObject,
    rank: Option<usize>,
    world_size: Option<usize>,
}

unsafe impl Send for TorchDistributedCommunicator {}

impl TorchDistributedCommunicator {
    pub fn new(
        backend: Option<String>,
        init_method: Option<String>,
        rank: Option<usize>,
        world_size: Option<usize>,
    ) -> PyResult<Self> {
        let result: PyResult<PyObject> = Python::with_gil(|py| {
            let distributed = Python::import(py, "torch.distributed")?;
            let init_process_group = distributed.getattr("init_process_group")?;
            let kwargs = PyDict::new(py);
            if let Some(backend) = backend {
                kwargs.set_item("backend", backend).unwrap();
            }
            if let Some(init_method) = init_method {
                kwargs.set_item("init_method", init_method).unwrap();
            }
            if let Some(world_size) = world_size {
                kwargs.set_item("world_size", world_size).unwrap();
            }
            if let Some(rank) = rank {
                kwargs.set_item("rank", rank).unwrap();
            }
            init_process_group.call((), Some(&kwargs))?;
            let distributed_c10d = Python::import(py, "torch.distributed.distributed_c10d")?;
            let get_default_store = distributed_c10d.getattr("_get_default_store")?;
            let store = get_default_store.call0()?;
            Ok(store.unbind())
        });
        Ok(Self {
            store: result?,
            rank,
            world_size,
        })
    }

    pub fn set(&self, key: &str, value: &str) -> PyResult<()> {
        Python::with_gil(|py| {
            let store = self.store.bind(py);
            let set = store.getattr("set")?;
            let _res = set.call1((key, value))?;
            Ok(())
        })
    }

    pub fn size(&self) -> usize {
        self.world_size.expect("World size not specified")
    }

    pub fn rank(&self) -> usize {
        self.rank.expect("Rank not specified")
    }

    pub fn broadcast(&self, tensor: &Tensor) -> PyResult<()> {
        Python::with_gil(|py| {
            let distributed = Python::import(py, "torch.distributed")?;
            let broadcast = distributed.getattr("broadcast")?;
            broadcast.call1((PyTensor(tensor.shallow_clone()), 0))?;
            Ok(())
        })
    }

    pub fn all_reduce(&self, tensor: &Tensor, op: ReduceType) -> PyResult<()> {
        assert!(op == ReduceType::Sum);
        Python::with_gil(|py| {
            let distributed = Python::import(py, "torch.distributed")?;
            let all_reduce = distributed.getattr("all_reduce")?;
            all_reduce.call1((PyTensor(tensor.shallow_clone()), 0))?;
            Ok(())
        })
    }
}

#[derive(Debug)]
pub struct PythonDistributedCausalLM {
    comm: TorchDistributedCommunicator,
    local: PythonCausalLM,
    pub(crate) parallelism: ParallelismConfig,
    #[allow(unused)]
    children: Vec<Child>,
}

unsafe impl Send for PythonDistributedCausalLM {}

impl PythonDistributedCausalLM {
    pub fn new(
        architecture: String,
        source: PretrainedSource<PythonModelConfig>,
        device: Device,
        parallelism: ParallelismConfig,
        override_max_position_embeddings: Option<usize>,
    ) -> Result<Self, PythonDistributedCausalLMError> {
        let world_size = parallelism.dp * parallelism.tp;
        let rank = match device {
            Device::Cuda(0) => 0,
            Device::Cuda(rank) => {
                return Err(PythonDistributedCausalLMError::LocalNotRankZero(rank));
            }
            _ => return Err(PythonDistributedCausalLMError::NonCUDADevice),
        };
        let backend = "nccl".to_string();
        let init_method = "tcp://127.0.0.1:34567".to_string();
        let local: JoinHandle<Result<_, PythonDistributedCausalLMError>> = {
            let backend = backend.clone();
            let init_method = init_method.clone();
            std::thread::spawn(move || {
                let comm = TorchDistributedCommunicator::new(
                    Some(backend),
                    Some(init_method),
                    Some(rank),
                    Some(world_size),
                )?;
                comm.set("architecture", &architecture)?;
                match &source {
                    PretrainedSource::RepoFiles(path_bufs) => {
                        comm.set("source", "files")?;
                        let files = path_bufs
                            .iter()
                            .map(|x| x.to_str().unwrap())
                            .collect::<Vec<_>>();
                        let files = serde_json::to_string(&files).unwrap();
                        comm.set("files", &files)?;
                    }
                    PretrainedSource::ConfigAndTensors(_, _hash_map) => todo!(),
                }
                comm.set("dp", &format!("{}", parallelism.dp))?;
                comm.set("tp", &format!("{}", parallelism.tp))?;
                let local = PythonCausalLM::new(
                    &architecture,
                    &source,
                    device,
                    Some(parallelism),
                    override_max_position_embeddings,
                )?;
                Ok((comm, local))
            })
        };
        let pid = format!("{}", std::process::id());
        tracing::debug!("Spawned local model load, pid is {pid}");
        let children: Result<Vec<Child>, _> = (1..world_size)
            .map(|rank| {
                let res = Command::new("python")
                    .arg("-m")
                    .arg("psyche.sidecar")
                    .arg("--parent-pid")
                    .arg(pid.clone())
                    .arg("--backend")
                    .arg(backend.clone())
                    .arg("--init-method")
                    .arg(init_method.clone())
                    .arg("--world-size")
                    .arg(format!("{world_size}"))
                    .arg("--rank")
                    .arg(format!("{rank}"))
                    .spawn();
                match res.as_ref() {
                    Ok(child) => tracing::debug!("Spawned sidecar process {}", child.id()),
                    Err(err) => tracing::error!("{err}"),
                };
                res
            })
            .collect();
        let children = children?;
        let (comm, local) = local.join().unwrap()?;

        Ok(Self {
            comm,
            local,
            parallelism,
            children,
        })
    }
}

impl CausalLM for PythonDistributedCausalLM {
    fn forward(
        &mut self,
        x: &Tensor,
        labels: Option<&tch::Tensor>,
        num_logits_to_keep: Option<i64>,
        loss_scale: Option<f64>,
    ) -> (Tensor, Option<Tensor>) {
        self.local
            .forward(x, labels, num_logits_to_keep, loss_scale)
    }

    fn device(&self) -> Device {
        self.local.device()
    }

    fn communicator(&self) -> Option<Arc<Communicator>> {
        #[allow(clippy::arc_with_non_send_sync)]
        // TODO: analyze how we're using Arc here, is this right?
        Some(Arc::new(self.comm.clone().into()))
    }

    fn prepare_for_training(&mut self) {
        self.local.prepare_for_training();
    }

    fn variables(&self) -> StableVariableIterator {
        self.local.variables()
    }

    fn clip_grad_norm(&mut self, max_grad_norm: f64) {
        self.local.clip_grad_norm(max_grad_norm);
    }

    fn bos_token_id(&self) -> Option<i64> {
        self.local.bos_token_id()
    }

    fn eos_token_ids(&self) -> Option<crate::EosToks> {
        self.local.eos_token_ids()
    }
}
