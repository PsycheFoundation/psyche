use serde::{Deserialize, Serialize};
use std::fs;

#[derive(Deserialize, Serialize)]
struct PromptEntry {
    text: String,
}

#[derive(Deserialize, Serialize)]
struct PromptsJson {
    prompts: Vec<PromptEntry>,
}

pub fn get_prompt_texts() -> Vec<String> {
    let json_content = fs::read_to_string("website/frontend/public/prompts/index.json")
        .expect("Failed to read prompts JSON file");
    let prompts_data: PromptsJson =
        serde_json::from_str(&json_content).expect("Failed to parse prompts JSON");
    prompts_data.prompts.into_iter().map(|p| p.text).collect()
}
