use crate::{
    AttentionImplementation, AutoConfig, CausalLM, Communicator, EosToks, ModelLoadError,
    ParallelismConfig, PretrainedSource, StableVariableIterator, Variable,
    device_utils::DevicePytorchStr,
};

use pyo3::{
    prelude::*,
    types::{IntoPyDict, PyDict, PyList, PyString, PyTuple},
};
use pyo3_tch::PyTensor;
use std::collections::HashMap;
use std::{rc::Rc, sync::Arc};
use tch::{Device, Tensor};
use thiserror::Error;

#[derive(Clone, Debug)]
pub struct PythonModelConfig {
    config: serde_json::Value,
}

impl serde::Serialize for PythonModelConfig {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.config.serialize(serializer)
    }
}

impl<'de> serde::Deserialize<'de> for PythonModelConfig {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        Ok(Self {
            config: serde_json::Value::deserialize(deserializer)?,
        })
    }
}

impl From<serde_json::Value> for PythonModelConfig {
    fn from(config: serde_json::Value) -> Self {
        Self { config }
    }
}

impl TryFrom<AutoConfig> for PythonModelConfig {
    type Error = ModelLoadError;

    fn try_from(value: AutoConfig) -> Result<Self, Self::Error> {
        match value {
            AutoConfig::Auto(config) => Ok(config),
            _ => Err(ModelLoadError::WrongConfigType),
        }
    }
}

impl TryFrom<PretrainedSource<AutoConfig>> for PretrainedSource<PythonModelConfig> {
    type Error = ModelLoadError;

    fn try_from(value: PretrainedSource<AutoConfig>) -> Result<Self, Self::Error> {
        match value {
            PretrainedSource::RepoFiles(x) => Ok(Self::RepoFiles(x)),
            PretrainedSource::ConfigAndTensors(AutoConfig::Auto(x), y) => {
                Ok(Self::ConfigAndTensors(x, y))
            }
            PretrainedSource::ConfigAndTensors(_, _) => Err(ModelLoadError::WrongConfigType),
        }
    }
}

impl PythonModelConfig {
    pub fn bos_token_id(&self) -> Option<i64> {
        self.config
            .as_object()
            .and_then(|x| x.get("bos_token_id"))
            .and_then(|x| x.as_i64())
    }

    pub fn eos_token_ids(&self) -> Option<EosToks> {
        self.config
            .as_object()
            .and_then(|x| x.get("eos_token_id"))
            .and_then(|x| match x {
                serde_json::Value::Number(number) => {
                    Some(EosToks::Single(number.as_i64().unwrap()))
                }
                serde_json::Value::Array(values) => Some(EosToks::Multiple(
                    values.iter().map(|x| x.as_i64().unwrap()).collect(),
                )),
                _ => None,
            })
    }

    pub fn max_position_embeddings(&self) -> Option<usize> {
        self.config
            .as_object()
            .and_then(|x| x.get("max_position_embeddings"))
            .and_then(|x| x.as_u64())
            .map(|x| x as usize)
    }
}

#[derive(Debug, Error)]
pub enum PythonCausalLMError {
    #[error("Python error: {0}")]
    PythonError(#[from] PyErr),

    #[error("Model load error: {0}")]
    ModelLoadError(#[from] ModelLoadError),
}

#[derive(Debug)]
pub struct PythonCausalLM {
    causal_lm: PyObject,
    device: Device,
    order: StablePythonParametersIterator,
    bos_token_id: Option<i64>,
    eos_token_id: Option<EosToks>,
    max_context_length: usize,
    pure_fsdp: Option<bool>,
}

unsafe impl Send for PythonCausalLM {}
unsafe impl Sync for PythonCausalLM {}

impl PythonCausalLM {
    pub fn new(
        architecture: &str,
        source: &PretrainedSource<PythonModelConfig>,
        device: Device,
        attn_implementation: AttentionImplementation,
        parallelism: Option<ParallelismConfig>,
        override_max_position_embeddings: Option<usize>,
    ) -> Result<PythonCausalLM, PythonCausalLMError> {
        let config = source.get_config()?;
        let result: PyResult<PyObject> = Python::with_gil(|py| {
            let psyche = Python::import(py, "psyche")?;
            let make_causal_lm = psyche.getattr("make_causal_lm")?;
            let source: PyObject = match source {
                PretrainedSource::RepoFiles(path_bufs) => {
                    let files = PyList::new(
                        py,
                        path_bufs
                            .iter()
                            .map(|x| x.to_string_lossy().into_owned())
                            .collect::<Vec<_>>(),
                    )?;
                    let class = psyche.getattr("PretrainedSourceRepoFiles")?;
                    let args = (files,);
                    class.call1(args)?.into()
                }
                PretrainedSource::ConfigAndTensors(config, state_dict) => {
                    let config_json = serde_json::to_string_pretty(&config).unwrap();
                    let state_dict = state_dict
                        .iter()
                        .map(|(k, v)| {
                            PyTensor(v.shallow_clone())
                                .into_pyobject(py)
                                .map(|pyobject| (k.clone(), pyobject))
                        })
                        .collect::<Result<Vec<_>, _>>()?
                        .into_py_dict(py)?;
                    let class = psyche.getattr("PretrainedSourceStateDict")?;
                    let args = (config_json, state_dict);
                    class.call1(args)?.into()
                }
            };
            let args = (
                architecture,
                source,
                device.to_pytorch_device_string(),
                attn_implementation.to_pytorch_attn_impl_str().to_owned(),
                parallelism.as_ref().map(|x| x.dp).unwrap_or(1),
                parallelism.as_ref().map(|x| x.tp).unwrap_or(1),
                override_max_position_embeddings,
            );
            let causal_lm = make_causal_lm.call1(args)?;
            Ok(causal_lm.unbind())
        });
        let causal_lm = result?;
        let order = StablePythonParametersIterator::new(&causal_lm);
        let max_context_length = override_max_position_embeddings
            .or(config.max_position_embeddings())
            .unwrap_or(2048); // Default fallback
        let pure_fsdp = parallelism.map(|x| x.dp >= 1 && x.tp == 1);
        Ok(Self {
            causal_lm,
            device,
            order,
            bos_token_id: config.bos_token_id(),
            eos_token_id: config.eos_token_ids(),
            max_context_length,
            pure_fsdp,
        })
    }

    pub fn from_python(causal_lm: PyObject, device: Device, config: serde_json::Value) -> Self {
        let config = PythonModelConfig { config };
        Self {
            order: StablePythonParametersIterator::new(&causal_lm),
            causal_lm,
            device,
            bos_token_id: config.bos_token_id(),
            eos_token_id: config.eos_token_ids(),
            max_context_length: config.max_position_embeddings().unwrap_or(2048),
            pure_fsdp: None,
        }
    }

    pub fn device(&self) -> Device {
        self.device
    }
}

impl CausalLM for PythonCausalLM {
    fn forward(
        &self,
        input_ids: &Tensor,
        labels: Option<&Tensor>,
        position_ids: Option<&Tensor>,
        sequence_lengths: Option<&Vec<Vec<i32>>>,
        num_logits_to_keep: Option<i64>,
        loss_scale: Option<f64>,
    ) -> (Option<Tensor>, Option<Tensor>) {
        let result: PyResult<(Option<Tensor>, Option<Tensor>)> = Python::with_gil(|py| {
            let causal_lm = self.causal_lm.bind(py);
            let forward = causal_lm.getattr("forward")?;
            let input_ids = PyTensor(input_ids.shallow_clone());
            let labels = labels.map(|x| PyTensor(x.shallow_clone()));
            let position_ids = position_ids.map(|x| PyTensor(x.shallow_clone()));
            let args = (
                input_ids,
                labels,
                position_ids,
                sequence_lengths,
                num_logits_to_keep,
                loss_scale,
            );
            let result: Bound<PyTuple> = forward.call1(args)?.downcast_into()?;
            let logits = result.get_item(0)?;
            let logits: Option<Tensor> = match logits.is_none() {
                true => None,
                false => Some(logits.extract::<PyTensor>()?),
            }
            .map(|x| x.0);
            let loss = result.get_item(1)?;
            let loss: Option<Tensor> = match loss.is_none() {
                true => None,
                false => Some(loss.extract::<PyTensor>()?),
            }
            .map(|x| x.0);
            Ok((logits, loss))
        });
        match result {
            Ok(result) => result,
            Err(err) => {
                panic!("Error in python forward: {err}");
            }
        }
    }

    fn device(&self) -> Device {
        self.device
    }

    fn communicator(&self) -> Option<Arc<Communicator>> {
        None
    }

    fn prepare_for_training(&self) {
        let result: PyResult<()> = Python::with_gil(|py| {
            let causal_lm = self.causal_lm.bind(py);

            let train = causal_lm.getattr("train")?;
            train.call0()?;

            Ok(())
        });
        result.unwrap();
    }

    fn variables(&self) -> StableVariableIterator {
        Box::new(self.order.clone())
    }

    fn clip_grad_norm(&self, max_grad_norm: f64) {
        assert!(
            self.pure_fsdp.unwrap_or(true),
            "Only pure FSDP supports `clip_grad_norm`"
        );
        let result: PyResult<()> = Python::with_gil(|py| {
            let module = py.import("torch.nn.utils")?;
            let clip_grad_norm = module.getattr("clip_grad_norm_")?;
            let tensors: Vec<_> = self
                .order
                .entries
                .iter()
                .map(|x| x.python.clone_ref(py))
                .collect();
            clip_grad_norm.call1((tensors, max_grad_norm))?;
            Ok(())
        });
        result.unwrap();
    }

    fn bos_token_id(&self) -> Option<i64> {
        self.bos_token_id
    }

    fn eos_token_ids(&self) -> Option<EosToks> {
        self.eos_token_id.clone()
    }

    fn max_context_length(&self) -> usize {
        self.max_context_length
    }

    fn convert(&self, state_dict: Option<HashMap<String, Tensor>>) -> HashMap<String, Tensor> {
        let result: PyResult<HashMap<String, Tensor>> = Python::with_gil(|py| {
            let causal_lm = self.causal_lm.bind(py);
            let convert = causal_lm.getattr("convert")?;
            let state_dict = match state_dict {
                Some(map) => {
                    let py_dict = PyDict::new(py);
                    for (k, v) in map {
                        py_dict.set_item(k, PyTensor(v.shallow_clone()))?;
                    }
                    Some(py_dict)
                }
                None => None,
            };
            let result: Bound<PyDict> = convert.call1((state_dict,))?.downcast_into()?;
            result
                .iter()
                .map(|(k, v)| {
                    let name = k.downcast_into::<PyString>()?.to_str()?.to_owned();
                    let tensor: PyTensor = v.extract()?;
                    Ok((name, tensor.0))
                })
                .collect()
        });
        match result {
            Ok(result) => result,
            Err(err) => {
                panic!("Error in python convert: {err}");
            }
        }
    }
}

#[derive(Debug)]
struct DTensorReferences {
    gather_full_tensor: PyObject,
    calculate_local_tensor_from_full: PyObject,
    zeros_like: PyObject,
    local_tensor: PyObject,
    set_grad: PyObject,
    zero_grad: PyObject,
}

#[derive(Debug)]
struct PythonCausalLMVariable {
    name: String,
    python: Rc<PyObject>,
    tensor: Tensor,
    local_tensor: Tensor,
    sharded: bool,
    full_shape: Vec<i64>,
    dtensor_references: Arc<DTensorReferences>,
}

impl Clone for PythonCausalLMVariable {
    fn clone(&self) -> Self {
        Self {
            name: self.name.clone(),
            python: self.python.clone(),
            tensor: self.tensor.shallow_clone(),
            local_tensor: self.local_tensor.shallow_clone(),
            sharded: self.sharded,
            full_shape: self.full_shape.clone(),
            dtensor_references: self.dtensor_references.clone(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct StablePythonParametersIterator {
    entries: Vec<PythonCausalLMVariable>,
}

impl StablePythonParametersIterator {
    pub fn new(causal_lm: &PyObject) -> Self {
        let entries: PyResult<Vec<PythonCausalLMVariable>> = Python::with_gil(|py| {
            let psyche = py.import("psyche.dtensor_helpers")?;
            let full_tensor_shape = psyche.getattr("full_tensor_shape")?;
            let gather_full_tensor = psyche.getattr("gather_full_tensor")?.unbind();
            let calculate_local_tensor_from_full =
                psyche.getattr("calculate_local_tensor_from_full")?.unbind();
            let zeros_like = psyche.getattr("zeros_like")?.unbind();
            let local_tensor = psyche.getattr("local_tensor")?;
            let set_grad = psyche.getattr("set_grad")?.unbind();
            let zero_grad = psyche.getattr("zero_grad")?.unbind();
            let dtensor_references = Arc::new(DTensorReferences {
                gather_full_tensor,
                calculate_local_tensor_from_full,
                zeros_like,
                local_tensor: local_tensor.clone().unbind(),
                set_grad,
                zero_grad,
            });
            let causal_lm = causal_lm.bind(py);
            let named_parameters = causal_lm.getattr("named_parameters")?;
            let named_parameters = named_parameters.call0()?.downcast_into::<PyDict>()?;
            let distributed_tensor = Python::import(py, "torch.distributed.tensor")?;
            let dtensor = distributed_tensor.getattr("DTensor")?;
            let result: Result<Vec<PythonCausalLMVariable>, PyErr> = named_parameters
                .iter()
                .map(|(k, v)| {
                    let name = k.downcast_into::<PyString>()?.to_str()?.to_owned();
                    let is_dtensor = v.is_instance(&dtensor)?;
                    let tensor: PyTensor = v.extract()?;
                    let full_shape: Vec<i64> = match is_dtensor {
                        true => full_tensor_shape.call1((v.clone(),))?.extract()?,
                        false => tensor.0.size(),
                    };
                    let local_tensor = match is_dtensor {
                        true => {
                            let local_tensor: PyTensor =
                                local_tensor.call1((v.clone(),))?.extract()?;
                            local_tensor.0
                        }
                        false => tensor.0.shallow_clone(),
                    };
                    //println!("pid={}, variable={}, is_dtensor={}", std::process::id(), name, is_dtensor);
                    Ok(PythonCausalLMVariable {
                        name,
                        python: v.unbind().into(),
                        tensor: tensor.0,
                        local_tensor,
                        sharded: is_dtensor,
                        full_shape,
                        dtensor_references: dtensor_references.clone(),
                    })
                })
                .collect();
            result
        });

        let mut entries = entries.unwrap();

        // this is in reverse order! then we can pop off the back as we iterate
        entries.sort_by(|a, b| b.name.cmp(&a.name));

        Self { entries }
    }
}

impl Iterator for StablePythonParametersIterator {
    type Item = Box<dyn Variable>;

    fn next(&mut self) -> Option<Self::Item> {
        self.entries.pop().map(|x| Box::new(x) as Box<dyn Variable>)
    }
}

impl Variable for PythonCausalLMVariable {
    fn name(&self) -> &str {
        &self.name
    }

    fn local_tensor(&self) -> Tensor {
        self.local_tensor.shallow_clone()
    }

    fn logical_tensor(&self) -> Tensor {
        self.tensor.shallow_clone()
    }

    fn gather_full_tensor(&self) -> Tensor {
        match self.sharded {
            true => {
                let result: PyResult<Tensor> = Python::with_gil(|py| {
                    let gather_full_tensor = self.dtensor_references.gather_full_tensor.bind(py);
                    let tensor: PyTensor = gather_full_tensor
                        .call1((self.python.clone_ref(py),))?
                        .extract()?;
                    Ok(tensor.0)
                });
                result.unwrap()
            }
            false => self.tensor.shallow_clone(),
        }
    }

    fn shard_other_tensor_like_me(&self, tensor: Tensor) -> Tensor {
        match self.sharded {
            true => {
                let result: PyResult<Tensor> = Python::with_gil(|py| {
                    let calculate_local_tensor_from_full = self
                        .dtensor_references
                        .calculate_local_tensor_from_full
                        .bind(py);
                    let tensor: PyTensor = calculate_local_tensor_from_full
                        .call1((PyTensor(tensor), self.python.clone_ref(py)))?
                        .extract()?;
                    Ok(tensor.0)
                });
                result.unwrap()
            }
            false => tensor,
        }
    }

    fn full_tensor_shape(&self) -> Vec<i64> {
        self.full_shape.clone()
    }

    fn is_sharded(&self) -> bool {
        self.sharded
    }

    fn zeros_like(&self, name: String) -> Box<dyn Variable> {
        match self.sharded {
            true => {
                let result: PyResult<PythonCausalLMVariable> = Python::with_gil(|py| {
                    let zeros_like = self.dtensor_references.zeros_like.bind(py);
                    let local_tensor = self.dtensor_references.local_tensor.bind(py);
                    let python = zeros_like.call1((self.python.clone_ref(py),))?;
                    let tensor: PyTensor = python.extract()?;
                    let local_tensor: PyTensor =
                        local_tensor.call1((python.clone(),))?.extract()?;
                    Ok(PythonCausalLMVariable {
                        name,
                        python: python.unbind().into(),
                        tensor: tensor.0,
                        local_tensor: local_tensor.0,
                        sharded: self.sharded,
                        full_shape: self.full_shape.clone(),
                        dtensor_references: self.dtensor_references.clone(),
                    })
                });
                Box::new(result.unwrap())
            }
            false => {
                let tensor = self.tensor.zeros_like();
                let local_tensor = tensor.shallow_clone();
                let python = PyTensor(tensor.shallow_clone());
                Python::with_gil(|py| {
                    Box::new(PythonCausalLMVariable {
                        name,
                        python: python
                            .into_pyobject(py)
                            .expect("tensor to pyobject should never fail")
                            .unbind()
                            .into(),
                        tensor,
                        local_tensor,
                        sharded: self.sharded,
                        full_shape: self.full_shape.clone(),
                        dtensor_references: self.dtensor_references.clone(),
                    })
                })
            }
        }
    }

    fn set_grad(&self, tensor: Tensor) {
        Python::with_gil(|py| {
            let set_grad = self.dtensor_references.set_grad.bind(py);
            set_grad
                .call1((PyTensor(self.tensor.shallow_clone()), PyTensor(tensor)))
                .unwrap();
        });
    }

    fn zero_grad(&self) {
        Python::with_gil(|py| {
            let zero_grad = self.dtensor_references.zero_grad.bind(py);
            zero_grad
                .call1((PyTensor(self.tensor.shallow_clone()),))
                .unwrap();
        });
    }
}

#[derive(Clone, Debug)]
pub struct WrappedPythonCausalLM {
    local: Arc<PythonCausalLM>,
}

impl CausalLM for WrappedPythonCausalLM {
    fn forward(
        &self,
        x: &Tensor,
        labels: Option<&Tensor>,
        position_ids: Option<&Tensor>,
        sequence_lengths: Option<&Vec<Vec<i32>>>,
        num_logits_to_keep: Option<i64>,
        loss_scale: Option<f64>,
    ) -> (Option<Tensor>, Option<Tensor>) {
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
        self.local.device()
    }

    fn max_context_length(&self) -> usize {
        self.local.max_context_length()
    }

    fn variables(&self) -> StableVariableIterator {
        self.local.variables()
    }

    fn communicator(&self) -> Option<Arc<Communicator>> {
        self.local.communicator()
    }

    fn prepare_for_training(&self) {
        self.local.prepare_for_training();
    }

    fn clip_grad_norm(&self, max_grad_norm: f64) {
        self.local.clip_grad_norm(max_grad_norm);
    }

    fn convert(&self, state_dict: Option<HashMap<String, Tensor>>) -> HashMap<String, Tensor> {
        self.local.convert(state_dict)
    }
}

impl From<PythonCausalLM> for WrappedPythonCausalLM {
    fn from(val: PythonCausalLM) -> Self {
        WrappedPythonCausalLM {
            local: Arc::new(val),
        }
    }
}
