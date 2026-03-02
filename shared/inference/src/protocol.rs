//! Protocol types for inference requests and responses

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ModelSource {
    HuggingFace(String),
    Local(String),
    // See test case below for additional future source types
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum InferenceGossipMessage {
    NodeAvailable {
        model_name: Option<String>, // None if no model loaded yet
        checkpoint_id: Option<String>,
        capabilities: Vec<String>,
    },
    NodeUnavailable,
    LoadModel {
        model_name: String,
        model_source: ModelSource,
    },
    ReloadCheckpoint {
        checkpoint_id: String,
        checkpoint_source: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum InferenceMessage {
    Request(InferenceRequest),
    Response(InferenceResponse),
    StreamChunk { request_id: String, text: String },
    Cancel { request_id: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InferenceRequest {
    pub request_id: String,
    pub messages: Vec<ChatMessage>,
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
            messages: vec![ChatMessage {
                role: "user".to_string(),
                content: "Once upon a time".to_string(),
            }],
            max_tokens: 50,
            temperature: 0.7,
            top_p: 0.9,
            stream: false,
        };

        let json = serde_json::to_string(&req).unwrap();
        let parsed: InferenceRequest = serde_json::from_str(&json).unwrap();

        assert_eq!(req.request_id, parsed.request_id);
        assert_eq!(req.messages.len(), parsed.messages.len());
        assert_eq!(req.messages[0].content, parsed.messages[0].content);
    }

    #[test]
    fn test_request_defaults() {
        let json = r#"{"request_id": "test", "messages": [{"role": "user", "content": "hello"}]}"#;
        let req: InferenceRequest = serde_json::from_str(json).unwrap();

        assert_eq!(req.max_tokens, 100);
        assert_eq!(req.temperature, 1.0);
        assert_eq!(req.top_p, 1.0);
        assert!(!req.stream);
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
            "messages": [{"role": "user", "content": "Test prompt"}],
            "max_tokens": 200,
            "temperature": 0.5,
            "top_p": 0.95,
            "stream": true
        }"#;

        let req: InferenceRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.request_id, "custom-1");
        assert_eq!(req.messages[0].content, "Test prompt");
        assert_eq!(req.max_tokens, 200);
        assert_eq!(req.temperature, 0.5);
        assert_eq!(req.top_p, 0.95);
        assert!(req.stream);
    }

    #[test]
    fn test_inference_message_serialization() {
        let req = InferenceRequest {
            request_id: "test-1".to_string(),
            messages: vec![ChatMessage {
                role: "user".to_string(),
                content: "Hello".to_string(),
            }],
            max_tokens: 10,
            temperature: 0.7,
            top_p: 0.9,
            stream: false,
        };

        let msg = InferenceMessage::Request(req);
        let bytes = postcard::to_stdvec(&msg).unwrap();
        let parsed: InferenceMessage = postcard::from_bytes(&bytes).unwrap();

        match parsed {
            InferenceMessage::Request(r) => {
                assert_eq!(r.request_id, "test-1");
                assert_eq!(r.messages[0].content, "Hello");
            }
            _ => panic!("Expected Request variant"),
        }
    }

    #[test]
    fn test_gossip_message_serialization() {
        let msg = InferenceGossipMessage::NodeAvailable {
            model_name: Some("gpt2".to_string()),
            checkpoint_id: Some("checkpoint-123".to_string()),
            capabilities: vec!["streaming".to_string()],
        };

        let bytes = postcard::to_stdvec(&msg).unwrap();
        let parsed: InferenceGossipMessage = postcard::from_bytes(&bytes).unwrap();

        match parsed {
            InferenceGossipMessage::NodeAvailable {
                model_name,
                checkpoint_id,
                capabilities,
            } => {
                assert_eq!(model_name, Some("gpt2".to_string()));
                assert_eq!(checkpoint_id, Some("checkpoint-123".to_string()));
                assert_eq!(capabilities, vec!["streaming"]);
            }
            _ => panic!("Expected NodeAvailable variant"),
        }
    }

    #[test]
    fn test_load_model_message_serialization() {
        let msg = InferenceGossipMessage::LoadModel {
            model_name: "gpt2".to_string(),
            model_source: ModelSource::HuggingFace("gpt2".to_string()),
        };

        let bytes = postcard::to_stdvec(&msg).unwrap();
        let parsed: InferenceGossipMessage = postcard::from_bytes(&bytes).unwrap();

        match parsed {
            InferenceGossipMessage::LoadModel {
                model_name,
                model_source,
            } => {
                assert_eq!(model_name, "gpt2");
                assert_eq!(model_source, ModelSource::HuggingFace("gpt2".to_string()));
            }
            _ => panic!("Expected LoadModel variant"),
        }
    }

    #[test]
    fn test_model_source_variants() {
        let hf = ModelSource::HuggingFace("NousResearch/Hermes-4-14B".to_string());
        let bytes = postcard::to_stdvec(&hf).unwrap();
        let parsed: ModelSource = postcard::from_bytes(&bytes).unwrap();
        assert_eq!(parsed, hf);
    }
}
