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

    #[test]
    fn test_response_serialization() {
        let resp = InferenceResponse {
            request_id: "test-123".to_string(),
            generated_text: "Hello, world!".to_string(),
            full_text: "Once upon a time Hello, world!".to_string(),
            finish_reason: Some("stop".to_string()),
        };

        let json = serde_json::to_string(&resp).unwrap();
        let parsed: InferenceResponse = serde_json::from_str(&json).unwrap();

        assert_eq!(resp.request_id, parsed.request_id);
        assert_eq!(resp.generated_text, parsed.generated_text);
        assert_eq!(resp.full_text, parsed.full_text);
        assert_eq!(resp.finish_reason, parsed.finish_reason);
    }

    #[test]
    fn test_response_optional_finish_reason() {
        let resp = InferenceResponse {
            request_id: "test-456".to_string(),
            generated_text: "Test".to_string(),
            full_text: "Prompt Test".to_string(),
            finish_reason: None,
        };

        let json = serde_json::to_string(&resp).unwrap();
        // finish_reason should be omitted when None
        assert!(!json.contains("finish_reason"));

        let parsed: InferenceResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.finish_reason, None);
    }

    #[test]
    fn test_request_with_custom_params() {
        let json = r#"{
            "request_id": "custom-1",
            "prompt": "Test prompt",
            "max_tokens": 200,
            "temperature": 0.5,
            "top_p": 0.95,
            "stream": true
        }"#;

        let req: InferenceRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.request_id, "custom-1");
        assert_eq!(req.prompt, "Test prompt");
        assert_eq!(req.max_tokens, 200);
        assert_eq!(req.temperature, 0.5);
        assert_eq!(req.top_p, 0.95);
        assert_eq!(req.stream, true);
    }
}
