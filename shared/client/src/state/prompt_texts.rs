use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize)]
struct PromptEntry {
    text: String,
}

#[derive(Deserialize, Serialize)]
struct PromptsJson {
    prompts: Vec<PromptEntry>,
}

pub fn get_prompt_texts() -> Vec<String> {
    let json_content = include_str!("prompt_texts/index.json");
    let prompts_data: PromptsJson =
        serde_json::from_str(json_content).expect("Failed to parse prompts JSON");
    prompts_data.prompts.into_iter().map(|p| p.text).collect()
}
