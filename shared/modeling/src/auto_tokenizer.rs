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
        println!("DEBUG: Found tokenizer_config.json at: {:?}", config_path);
        if let Ok(config_content) = std::fs::read_to_string(config_path) {
            println!(
                "DEBUG: Successfully read tokenizer_config.json, size: {} bytes",
                config_content.len()
            );
            if let Ok(config) = serde_json::from_str::<Value>(&config_content) {
                println!("DEBUG: Successfully parsed tokenizer_config.json");
                apply_tokenizer_config(&mut tokenizer, &config);
            } else {
                println!("DEBUG: Failed to parse tokenizer_config.json as JSON");
            }
        } else {
            println!("DEBUG: Failed to read tokenizer_config.json file");
        }
    } else {
        println!("DEBUG: No tokenizer_config.json found in repo files");
        println!(
            "DEBUG: Available files: {:?}",
            repo_files.iter().map(|p| p.file_name()).collect::<Vec<_>>()
        );
    }

    Ok(tokenizer)
}

fn apply_tokenizer_config(tokenizer: &mut Tokenizer, config: &Value) {
    println!("DEBUG: Applying tokenizer config...");
    println!(
        "DEBUG: Config keys: {:?}",
        config.as_object().map(|o| o.keys().collect::<Vec<_>>())
    );

    // Print all config for debugging
    if let Some(obj) = config.as_object() {
        for (key, value) in obj {
            println!("DEBUG: Config[{}] = {:?}", key, value);
        }
    }

    // Apply special tokens if they exist in the config
    // Handle both string format and object format {"content": "token", ...}
    let bos_token = config.get("bos_token").and_then(|v| {
        v.as_str()
            .or_else(|| v.get("content").and_then(|c| c.as_str()))
    });

    if let Some(bos_token) = bos_token {
        println!("DEBUG: Found BOS token in config: {}", bos_token);
        if let Some(bos_id) = tokenizer.token_to_id(bos_token) {
            println!(
                "DEBUG: BOS token exists in tokenizer: {} (id: {})",
                bos_token, bos_id
            );
        } else {
            println!(
                "DEBUG: BOS token not found in tokenizer vocabulary: {}",
                bos_token
            );
        }
    } else {
        println!("DEBUG: No bos_token found in config");
    }

    let eos_token = config.get("eos_token").and_then(|v| {
        v.as_str()
            .or_else(|| v.get("content").and_then(|c| c.as_str()))
    });

    if let Some(eos_token) = eos_token {
        println!("DEBUG: Found EOS token in config: {}", eos_token);
        if let Some(eos_id) = tokenizer.token_to_id(eos_token) {
            println!(
                "DEBUG: EOS token exists in tokenizer: {} (id: {})",
                eos_token, eos_id
            );
        } else {
            println!(
                "DEBUG: EOS token not found in tokenizer vocabulary: {}",
                eos_token
            );
        }
    } else {
        println!("DEBUG: No eos_token found in config");
    }

    let pad_token = config.get("pad_token").and_then(|v| {
        v.as_str()
            .or_else(|| v.get("content").and_then(|c| c.as_str()))
    });

    if let Some(pad_token) = pad_token {
        println!("DEBUG: Found PAD token in config: {}", pad_token);
        if let Some(pad_id) = tokenizer.token_to_id(pad_token) {
            println!(
                "DEBUG: PAD token exists in tokenizer: {} (id: {})",
                pad_token, pad_id
            );
        } else {
            println!(
                "DEBUG: PAD token not found in tokenizer vocabulary: {}",
                pad_token
            );
        }
    } else {
        println!("DEBUG: No pad_token found in config");
    }

    // Apply model_max_length if specified
    if let Some(max_len) = config.get("model_max_length").and_then(|v| v.as_u64()) {
        println!("DEBUG: Found model_max_length in config: {}", max_len);
        // Enable truncation with the model's max length
        if let Some(mut truncation) = tokenizer.get_truncation().cloned() {
            truncation.max_length = max_len as usize;
            let _ = tokenizer.with_truncation(Some(truncation));
            println!(
                "DEBUG: Updated existing truncation config to max_length: {}",
                max_len
            );
        } else {
            // Create new truncation config if none exists
            let _ = tokenizer.with_truncation(Some(tokenizers::TruncationParams {
                max_length: max_len as usize,
                ..Default::default()
            }));
            println!(
                "DEBUG: Created new truncation config with max_length: {}",
                max_len
            );
        }
    } else {
        println!("DEBUG: No model_max_length found in config");
    }

    // Check add_bos_token setting
    if let Some(add_bos_token) = config.get("add_bos_token").and_then(|v| v.as_bool()) {
        println!("DEBUG: Found add_bos_token setting: {}", add_bos_token);
        // Note: The tokenizers crate doesn't expose a way to modify the add_special_tokens behavior
        // after tokenizer creation. The evaluation harness needs to handle this during encode() calls.
        // For now, we just log this information.
    } else {
        println!("DEBUG: No add_bos_token setting found in config");
    }

    println!("DEBUG: Finished applying tokenizer config");
}
