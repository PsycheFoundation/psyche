use crate::{
    DeepseekConfig, Devices, LlamaConfig, LoadSafetensorsError, parallelism::tensor_shard,
    safetensor_utils::load_safetensors_into_variables,
};
use std::{
    collections::{HashMap, HashSet},
    io,
    path::PathBuf,
    sync::Arc,
};
use tch::Tensor;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ModelLoadError {
    #[error("unavailable device(s)")]
    UnavailbleDevice(Devices),

    #[error("missing config.json")]
    MissingConfigJSON,

    #[error("failed to read file config.json")]
    FailedToReadConfig(#[from] io::Error),

    #[error("could not parse config.json: {0}")]
    FailedToParseConfig(#[from] serde_json::Error),

    #[error("this model uses tied embeddings, which aren't supported.")]
    ModelHasTiedEmbeddings,

    #[error("Directly setting attention implementation to FlashAttention-2 is unsupported for now")]
    ModelExplicitlyUsesFA2,

    #[error("Failed to initialize CNCCL for tensor parallelism {0}")]
    TensorParallelismFailedInit(tch::TchError),

    #[error("Tried to use tensor parallelism with feature \"parallelism\" disabled")]
    TensorParallelismNotEnabled,

    #[error("Failed to load safetensors from disk: {0}")]
    LoadSafetensorsError(#[from] LoadSafetensorsError),

    #[error("Failed to copy tensor into variable store: {0}")]
    CopyTensorError(#[from] tch::TchError),

    #[error("Some parameters were not loaded: {0:?}")]
    LoadTensorError(HashSet<String>),

    #[error("Wrong config type")]
    WrongConfigType,

    #[error("Communicator/CommunicatorId mismatch")]
    CommunicatorMismatch,

    #[cfg(feature = "python")]
    #[error("Python error: {0}")]
    PythonError(#[from] pyo3::PyErr),

    #[cfg(feature = "python")]
    #[error("Python distributed error: {0}")]
    PythonDistributedError(String),

    #[error("No device available for rank {0} for devices config {1}")]
    NoDeviceForRank(usize, Devices),
}

#[derive(Clone)]
pub enum PretrainedSource<T: serde::Serialize + Clone> {
    RepoFiles(Vec<PathBuf>),
    ConfigAndTensors(T, Arc<HashMap<String, Tensor>>),
}

unsafe impl<T: serde::Serialize + Clone> Send for PretrainedSource<T> {}
unsafe impl<T: serde::Serialize + Clone> Sync for PretrainedSource<T> {}

impl<T: serde::Serialize + Clone + serde::de::DeserializeOwned> PretrainedSource<T> {
    pub fn get_config(&self) -> Result<T, ModelLoadError> {
        match self {
            PretrainedSource::RepoFiles(repo_files) => {
                let config_file = std::fs::read_to_string(
                    repo_files
                        .iter()
                        .find(|x| x.ends_with("config.json"))
                        .ok_or(ModelLoadError::MissingConfigJSON)?
                        .as_path(),
                )?;
                let llama_config: T = serde_json::from_str(&config_file)?;
                Ok(llama_config)
            }
            PretrainedSource::ConfigAndTensors(config, _) => Ok(config.clone()),
        }
    }

    pub fn load(&self, variables: &mut tch::nn::VarStore) -> Result<(), ModelLoadError> {
        match self {
            PretrainedSource::RepoFiles(repo_files) => {
                load_safetensors_into_variables(variables, repo_files)?
            }
            PretrainedSource::ConfigAndTensors(_, parameters) => {
                let mut unmatched = variables
                    .variables()
                    .keys()
                    .cloned()
                    .collect::<HashSet<_>>();

                let _no_grad = tch::no_grad_guard();
                let mut variables = variables.variables_.lock().unwrap();
                let shards = variables.shards.clone();
                for (name, var) in variables.named_variables.iter_mut() {
                    let tensor = parameters.get(name).unwrap();
                    if let Some(shard) = shards.get(name) {
                        let tensor = tensor_shard(tensor, shard);
                        var.f_copy_(&tensor)?;
                    } else {
                        var.f_copy_(tensor)?
                    };

                    unmatched.remove(name);
                }
                if !unmatched.is_empty() {
                    return Err(ModelLoadError::LoadTensorError(unmatched));
                };
            }
        };
        Ok(())
    }
}

impl<T: serde::Serialize + Clone> PretrainedSource<T> {
    pub fn serialize_config(&self) -> Result<String, ModelLoadError> {
        match self {
            PretrainedSource::RepoFiles(repo_files) => Ok(std::fs::read_to_string(
                repo_files
                    .iter()
                    .find(|x| x.ends_with("config.json"))
                    .ok_or(ModelLoadError::MissingConfigJSON)?
                    .as_path(),
            )?),
            PretrainedSource::ConfigAndTensors(config, _) => Ok(serde_json::to_string(config)?),
        }
    }
}

#[derive(serde::Deserialize, Debug, Default, Clone, Copy, PartialEq)]
pub enum AttentionImplementation {
    #[serde(rename = "eager")]
    Eager,
    #[serde(rename = "sdpa")]
    #[default]
    Sdpa,
    #[cfg(feature = "parallelism")]
    #[serde(rename = "flash_attention_2")]
    FlashAttention2,
}

impl AttentionImplementation {
    pub fn to_pytorch_attn_impl_str(&self) -> &str {
        match self {
            AttentionImplementation::Eager => "eager",
            AttentionImplementation::Sdpa => "sdpa",
            #[cfg(feature = "parallelism")]
            AttentionImplementation::FlashAttention2 => "flash_attention_2",
        }
    }
}

#[derive(Debug, Clone)]
#[allow(clippy::large_enum_variant)]
pub enum AutoConfig {
    Llama(LlamaConfig),
    Deepseek(DeepseekConfig),
    #[cfg(feature = "python")]
    Auto(crate::PythonModelConfig),
}

impl serde::Serialize for AutoConfig {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match self {
            AutoConfig::Llama(config) => config.serialize(serializer),
            AutoConfig::Deepseek(config) => config.serialize(serializer),
            #[cfg(feature = "python")]
            AutoConfig::Auto(config) => config.serialize(serializer),
        }
    }
}
