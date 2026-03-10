mod attention;
mod auto_config;
mod auto_model;
mod auto_tokenizer;
mod batcher;
mod causal_language_model;
mod device_utils;
mod distro;
mod dummy;
mod fp32_gradient_accumulator;
mod models;
mod optimizer;
mod parallelism;
#[cfg(feature = "python")]
mod python_causal_lm;
#[cfg(feature = "python")]
mod python_distributed_causal_lm;
#[cfg(feature = "python")]
mod python_distributed_trainer;
mod rms_norm;
mod rope;
mod safetensor_utils;
mod sampling;
mod token_output_stream;
mod trainer;
mod variable;

pub use attention::CausalSelfAttention;
pub use auto_config::{AttentionImplementation, AutoConfig, ModelLoadError, PretrainedSource};
pub use auto_model::auto_model_for_causal_lm_from_pretrained;
pub use auto_tokenizer::{AutoTokenizerError, auto_tokenizer};
pub use batcher::Batcher;
pub use causal_language_model::{
    CausalLM, CausalLanguageModel, EosToks, LanguageModelBuilder, LanguageModelConfig,
    LanguageModelForward,
};
pub use device_utils::{Devices, get_optimal_devices};
pub use distro::{CompressDCT, Distro, DistroResult, TransformDCT};
pub use dummy::{DummyModel, get_dummy_parameters};
pub use fp32_gradient_accumulator::Fp32GradientAccumulator;
pub use models::*;
pub use optimizer::Optimizer;
pub use parallelism::{
    AllReduce, ColumnParallelLinear, Communicator, CommunicatorId, CudaSynchronize,
    ParallelExpandHeads, ParallelismConfig, ReduceType, RowParallelLinear, unsharded_cpu_variables,
};
#[cfg(feature = "python")]
pub use python_causal_lm::{PythonCausalLM, PythonCausalLMError, PythonModelConfig};
#[cfg(feature = "python")]
pub use python_distributed_causal_lm::{
    PythonDistributedCausalLM, PythonDistributedCausalLMError, TorchDistributedCommunicator,
};
#[cfg(feature = "python")]
pub use python_distributed_trainer::{
    NopBarrier, PythonDistributedTrainer, PythonDistributedTrainerError,
};
pub use rms_norm::RMSNorm;
pub use rope::{RoPECache, RoPEConfig, RoPEType, default_rope, rotate_half, yarn_get_mscale};
pub use safetensor_utils::{
    LoadSafetensorsError, SaveSafetensorsError, load_safetensors_into_variables,
    save_tensors_into_safetensors,
};
pub use sampling::{LogitsProcessor, Sampling};
pub use token_output_stream::TokenOutputStream;
pub use trainer::{
    ApplyDistroResultError, Batch, BatchData, BatchDataCPU, BatchDataGPU, DataParallel,
    LocalTrainer, ParallelModels, TrainOutput, Trainer, TrainerThreadCommunicationError,
};
pub use variable::{StableVarStoreIterator, StableVariableIterator, Variable};

#[allow(unused)]
pub fn set_torch_rng_seed() {
    use rand::Rng;

    let seed: i64 = rand::rng().random();
    tch::manual_seed(seed);
    println!("torch seed set to: {seed}");
}

pub fn set_suggested_env_vars() {
    std::env::set_var("NCCL_P2P_DIRECT_DISABLE", "1");
    std::env::set_var("NCCL_LAUNCH_MODE", "GROUP");
}
