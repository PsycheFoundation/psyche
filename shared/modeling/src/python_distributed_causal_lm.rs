use crate::{
    AttentionImplementation, CausalLM, Communicator, ParallelismConfig, PretrainedSource,
    PythonCausalLM, ReduceType, StableVariableIterator,
    python_causal_lm::{PythonCausalLMError, PythonModelConfig},
};

use pyo3::{PyErr, PyResult, Python, prelude::*, types::PyDict};
use pyo3_tch::PyTensor;
use std::{
    process::{Child, Command},
    sync::Arc,
    thread::JoinHandle,
    time::Duration,
};
use tch::{Device, Tensor};
use thiserror::Error;
use tracing::trace;

#[derive(Debug, Error)]
pub enum PythonDistributedCausalLMError {
    #[error("Local device must be rank 0, instead got {0}")]
    LocalNotRankZero(usize),

    #[error("Local device not a CUDA device")]
    NonCUDADevice,

    #[error("Python error: {0}")]
    PythonError(#[from] PyErr),

    #[error("Sidecar spawn error: {0}")]
    SidecarSpawnError(#[from] std::io::Error),

    #[error("Local load error: {0}")]
    LocalLoadError(#[from] PythonCausalLMError),

    #[error("Calculated world size \"{0}\" is less than number of total GPU processes \"{1}\"")]
    IncompatibleWorldSize(usize, usize),
}

#[derive(Debug, Clone)]
pub struct TorchDistributedCommunicator {
    store: Arc<PyObject>,
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
            let timeout = Duration::from_secs(60 * 60 * 2); // use a large timeout for warmup

            let store = match &init_method {
                Some(init_method) => {
                    if let Some(init_method) = init_method.strip_prefix("tcp://") {
                        let (host_name, port) = init_method.split_once(":").unwrap();
                        let tcp_store = distributed.getattr("TCPStore")?;
                        let kwargs = PyDict::new(py);
                        kwargs.set_item("host_name", host_name).unwrap();
                        kwargs
                            .set_item("port", port.parse::<usize>().unwrap())
                            .unwrap();
                        kwargs.set_item("world_size", world_size.unwrap()).unwrap();
                        kwargs.set_item("is_master", true).unwrap();
                        kwargs.set_item("timeout", timeout).unwrap();
                        kwargs.set_item("use_libuv", false).unwrap();
                        Some(tcp_store.call((), Some(&kwargs))?)
                    } else {
                        None
                    }
                }
                None => None,
            };

            let init_process_group = distributed.getattr("init_process_group")?;
            let kwargs = PyDict::new(py);
            if let Some(backend) = backend {
                kwargs.set_item("backend", backend).unwrap();
            }
            if let Some(store) = store.clone() {
                kwargs.set_item("store", store).unwrap();
            } else if let Some(init_method) = init_method {
                kwargs.set_item("init_method", init_method).unwrap();
            }
            if let Some(world_size) = world_size {
                kwargs.set_item("world_size", world_size).unwrap();
            }
            if let Some(rank) = rank {
                kwargs.set_item("rank", rank).unwrap();
            }
            kwargs.set_item("timeout", timeout).unwrap();
            init_process_group.call((), Some(&kwargs))?;

            let store = match store {
                Some(store) => store,
                None => {
                    let distributed_c10d =
                        Python::import(py, "torch.distributed.distributed_c10d")?;
                    let get_default_store = distributed_c10d.getattr("_get_default_store")?;
                    get_default_store.call0()?
                }
            };

            Ok(store.unbind())
        });
        Ok(Self {
            store: Arc::new(result?),
            rank,
            world_size,
        })
    }

    pub fn set(&self, key: &str, value: &str) -> PyResult<()> {
        let ret = Python::with_gil(|py| {
            let store = self.store.bind(py);
            let set = store.getattr("set")?;
            let _res = set.call1((key, value))?;
            Ok(())
        });
        trace!("Set key {} (length {}) in store", key, value.len());
        ret
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
            all_reduce.call1((PyTensor(tensor.shallow_clone()),))?;
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
        attn_implementation: AttentionImplementation,
        parallelism: ParallelismConfig,
        override_max_position_embeddings: Option<usize>,
        num_local_ranks: Option<i64>,
    ) -> Result<Self, PythonDistributedCausalLMError> {
        if !tch::Cuda::is_available() {
            return Err(PythonDistributedCausalLMError::NonCUDADevice);
        }
        let num_local_ranks = num_local_ranks.unwrap_or_else(tch::Cuda::device_count);
        let world_size = parallelism.dp * parallelism.tp;
        if world_size < (num_local_ranks as usize) {
            return Err(PythonDistributedCausalLMError::IncompatibleWorldSize(
                world_size,
                num_local_ranks as usize,
            ));
        }

        let rank = match device {
            Device::Cuda(0) => 0,
            Device::Cuda(rank) => {
                // TODO: is this actually a bug?
                // Does the 0th cuda device *have* to be rank 0?
                return Err(PythonDistributedCausalLMError::LocalNotRankZero(rank));
            }
            _ => return Err(PythonDistributedCausalLMError::NonCUDADevice),
        };
        let backend = "nccl".to_string();
        let init_method = "tcp://0.0.0.0:34567".to_string();
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
                            .map(|x| contract_home_path(x))
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
                    attn_implementation,
                    Some(parallelism),
                    override_max_position_embeddings,
                )?;
                Ok((comm, local))
            })
        };
        let pid = format!("{}", std::process::id());
        tracing::debug!("Spawned local model load, pid is {pid}");
        let children: Result<Vec<Child>, _> = (1..num_local_ranks)
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
                    .arg("--device")
                    .arg(format!("{rank}"))
                    .spawn();
                match res.as_ref() {
                    Ok(child) => tracing::info!("Spawned sidecar process {}", child.id()),
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

    fn max_context_length(&self) -> usize {
        self.local.max_context_length()
    }
}

use std::path::Path;

fn contract_home_path(path: &Path) -> String {
    if let Ok(home_dir) = std::env::var("HOME") {
        if let Some(path_str) = path.to_str() {
            let home_str = home_dir.to_string();
            if path_str.starts_with(&home_str) {
                // Replace the home directory part with ~/
                return format!("~/{}", &path_str[home_str.len()..]);
            }
        }
    }
    // If we can't contract it, return as is
    path.to_str().unwrap_or_default().to_string()
}
