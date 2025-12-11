//! Protocol types for inference requests and responses

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InferenceRequest {
    pub request_id: String,
    pub prompt: String,
    #[serde(default = "default_max_tokens")]
    pub max_tokens: usize,
    #[serde(default = "default_temperature")]
    pub temperature: f64,
    #[serde(default = "default_top_p")]
    pub top_p: f64,
    #[serde(default)]
    pub stream: bool,
}

fn default_max_tokens() -> usize {
    100
}

fn default_temperature() -> f64 {
    1.0
}

fn default_top_p() -> f64 {
    1.0
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InferenceResponse {
    pub request_id: String,
    pub generated_text: String,
    pub full_text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub finish_reason: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_request_serialization() {
        let req = InferenceRequest {
            request_id: "test-123".to_string(),
            prompt: "Once upon a time".to_string(),
            max_tokens: 50,
            temperature: 0.7,
            top_p: 0.9,
            stream: false,
        };

        let json = serde_json::to_string(&req).unwrap();
        let parsed: InferenceRequest = serde_json::from_str(&json).unwrap();

        assert_eq!(req.request_id, parsed.request_id);
        assert_eq!(req.prompt, parsed.prompt);
    }

    #[test]
    fn test_request_defaults() {
        let json = r#"{"request_id": "test", "prompt": "hello"}"#;
        let req: InferenceRequest = serde_json::from_str(json).unwrap();

        assert_eq!(req.max_tokens, 100);
        assert_eq!(req.temperature, 1.0);
        assert_eq!(req.top_p, 1.0);
        assert_eq!(req.stream, false);
    }
}
