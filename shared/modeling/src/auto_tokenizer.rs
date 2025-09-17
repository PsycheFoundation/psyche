use serde_json::Value;
use std::path::PathBuf;
use thiserror::Error;
use tokenizers::Tokenizer;

#[derive(Error, Debug)]
pub enum AutoTokenizerError {
    #[error("Failed to load tokenizer from tokenizer.json")]
    CouldntLoadTokenizer(#[from] tokenizers::Error),

    #[error("Could not find tokenizer.json")]
    FileNotFound,

    #[error("Failed to read tokenizer_config.json")]
    ConfigReadError(#[from] std::io::Error),

    #[error("Failed to parse tokenizer_config.json")]
    ConfigParseError(#[from] serde_json::Error),
}

pub fn auto_tokenizer(repo_files: &[PathBuf]) -> Result<Tokenizer, AutoTokenizerError> {
    // Find tokenizer.json
    let tokenizer_path = repo_files
        .iter()
        .find(|x| x.ends_with("tokenizer.json"))
        .ok_or(AutoTokenizerError::FileNotFound)?;

    // Load base tokenizer
    let mut tokenizer = Tokenizer::from_file(tokenizer_path.as_path())?;

    // Try to find and apply tokenizer_config.json
    if let Some(config_path) = repo_files
        .iter()
        .find(|x| x.ends_with("tokenizer_config.json"))
    {
        if let Ok(config_content) = std::fs::read_to_string(config_path) {
            if let Ok(config) = serde_json::from_str::<Value>(&config_content) {
                apply_tokenizer_config(&mut tokenizer, &config);
            }
            // Silently continue if config parsing fails - tokenizer will still work
        }
    }

    Ok(tokenizer)
}

fn apply_tokenizer_config(tokenizer: &mut Tokenizer, config: &Value) {
    // Apply special tokens if they exist in the config
    if let Some(bos_token) = config.get("bos_token").and_then(|v| v.as_str()) {
        if let Some(bos_id) = tokenizer.token_to_id(bos_token) {
            // Note: The tokenizers crate doesn't expose a direct way to set special tokens
            // after creation, but the tokenizer.json should already contain them.
            // This is mainly for logging/verification purposes.
            tracing::debug!("Found BOS token: {} (id: {})", bos_token, bos_id);
        }
    }

    if let Some(eos_token) = config.get("eos_token").and_then(|v| v.as_str()) {
        if let Some(eos_id) = tokenizer.token_to_id(eos_token) {
            tracing::debug!("Found EOS token: {} (id: {})", eos_token, eos_id);
        }
    }

    if let Some(pad_token) = config.get("pad_token").and_then(|v| v.as_str()) {
        if let Some(pad_id) = tokenizer.token_to_id(pad_token) {
            tracing::debug!("Found PAD token: {} (id: {})", pad_token, pad_id);
        }
    }

    // Apply model_max_length if specified
    if let Some(max_len) = config.get("model_max_length").and_then(|v| v.as_u64()) {
        // Enable truncation with the model's max length
        if let Some(mut truncation) = tokenizer.get_truncation().cloned() {
            truncation.max_length = max_len as usize;
            let _ = tokenizer.with_truncation(Some(truncation));
        } else {
            // Create new truncation config if none exists
            let _ = tokenizer.with_truncation(Some(tokenizers::TruncationParams {
                max_length: max_len as usize,
                ..Default::default()
            }));
        }
        tracing::debug!("Set model max length to: {}", max_len);
    }
}
