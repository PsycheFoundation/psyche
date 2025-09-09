use crate::{
    AttentionImplementation, CausalLM, CommunicatorId, DeepseekForCausalLM, LlamaForCausalLM,
    ModelLoadError, PretrainedSource,
};

use std::path::PathBuf;
use tch::{Device, Kind};

pub fn auto_model_for_causal_lm_from_pretrained(
    repo_files: Vec<PathBuf>,
    kind: Option<Kind>,
    attn_implementation: Option<AttentionImplementation>,
    device: Option<Device>,
    tensor_parallelism_world: Option<(CommunicatorId, usize, usize)>,
    override_max_position_embeddings: Option<usize>,
) -> Result<Box<dyn CausalLM>, ModelLoadError> {
    let config_json = std::fs::read_to_string(
        repo_files
            .iter()
            .find(|x| x.ends_with("config.json"))
            .ok_or(ModelLoadError::MissingConfigJSON)?
            .as_path(),
    )?;
    let config_json: serde_json::Value = serde_json::from_str(&config_json)?;
    let model_type = config_json
        .as_object()
        .ok_or(ModelLoadError::WrongConfigType)?
        .get("model_type")
        .ok_or(ModelLoadError::WrongConfigType)?
        .as_str()
        .ok_or(ModelLoadError::WrongConfigType)?;
    match model_type {
        "llama" => LlamaForCausalLM::from_pretrained(
            &PretrainedSource::RepoFiles(repo_files),
            kind,
            attn_implementation,
            device,
            tensor_parallelism_world,
            override_max_position_embeddings,
        )
        .map(|x| Box::new(x) as Box<dyn CausalLM>),
        "deepseek_v2" | "deepseek_v3" => DeepseekForCausalLM::from_pretrained(
            &PretrainedSource::RepoFiles(repo_files),
            kind,
            attn_implementation,
            device,
            tensor_parallelism_world,
            override_max_position_embeddings,
        )
        .map(|x| Box::new(x) as Box<dyn CausalLM>),
        _ => Err(ModelLoadError::WrongConfigType),
    }
}
