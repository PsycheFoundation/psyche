//! Psyche Inference
//!
//! This crate provides inference capabilities for Psyche, including:
//! - vLLM integration via PyO3
//! - Inference node (P2P participant that serves models)
//! - Inference client (discovers and requests from inference nodes)

pub mod node;
pub mod protocol;
pub mod vllm;

// TODO: Add client module
// pub mod client;
