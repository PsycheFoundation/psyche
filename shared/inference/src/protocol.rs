//! Protocol types for inference requests and responses

use serde::{Deserialize, Serialize};

/// Inference request sent to an inference node
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InferenceRequest {
    /// Unique request ID for tracking
    pub request_id: String,

    /// Input prompt
    pub prompt: String,

    /// Maximum tokens to generate
    #[serde(default = "default_max_tokens")]
    pub max_tokens: usize,

    /// Sampling temperature (0.0 = deterministic, higher = more random)
    #[serde(default = "default_temperature")]
    pub temperature: f64,

    /// Nucleus sampling probability
    #[serde(default = "default_top_p")]
    pub top_p: f64,

    /// Whether to stream response token by token
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

/// Inference response from an inference node
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InferenceResponse {
    /// Request ID this response is for
    pub request_id: String,

    /// Generated text
    pub generated_text: String,

    /// Full text (prompt + generated)
    pub full_text: String,

    /// Reason for completion ("stop", "length", etc.)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub finish_reason: Option<String>,
}

/// Streaming chunk for incremental responses
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InferenceChunk {
    /// Request ID this chunk is for
    pub request_id: String,

    /// New tokens since last chunk
    pub delta: String,

    /// Whether this is the final chunk
    pub is_final: bool,

    /// Finish reason if final
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
