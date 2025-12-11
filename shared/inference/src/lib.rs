//! Psyche Inference
//!
//! This crate provides inference capabilities for Psyche, including:
//! - vLLM integration via PyO3
//! - Inference node (P2P participant that serves models)
//! - Inference client (discovers and requests from inference nodes)
//!
//! # Architecture
//!
//! - `vllm`: PyO3 bindings to Python vLLM engine
//! - `node`: Inference node implementation
//! - `client`: Inference client for making requests
//! - `protocol`: Request/response types for P2P communication

pub mod vllm;

// TODO: Add these modules as we build them
// pub mod node;
// pub mod client;
// pub mod protocol;
