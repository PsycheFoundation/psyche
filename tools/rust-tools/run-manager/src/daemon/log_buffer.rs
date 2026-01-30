use std::collections::VecDeque;
use std::sync::Arc;
use tokio::sync::{Mutex, broadcast};

const DEFAULT_CAPACITY: usize = 10_000;

/// Ring buffer for storing log lines with broadcast support for streaming
pub struct LogBuffer {
    lines: Mutex<VecDeque<String>>,
    capacity: usize,
    sender: broadcast::Sender<String>,
}

impl LogBuffer {
    pub fn new(capacity: usize) -> Arc<Self> {
        let (sender, _) = broadcast::channel(1024);
        Arc::new(Self {
            lines: Mutex::new(VecDeque::with_capacity(capacity)),
            capacity,
            sender,
        })
    }

    pub fn with_default_capacity() -> Arc<Self> {
        Self::new(DEFAULT_CAPACITY)
    }

    /// Push a new log line, evicting oldest if at capacity
    pub async fn push(&self, line: String) {
        let mut lines = self.lines.lock().await;
        if lines.len() >= self.capacity {
            lines.pop_front();
        }
        lines.push_back(line.clone());
        // Ignore send errors (no receivers)
        let _ = self.sender.send(line);
    }

    /// Get the last n lines (or all if n is None)
    pub async fn get_lines(&self, n: Option<usize>) -> Vec<String> {
        let lines = self.lines.lock().await;
        match n {
            Some(n) => lines.iter().rev().take(n).rev().cloned().collect(),
            None => lines.iter().cloned().collect(),
        }
    }

    /// Subscribe to new log lines for streaming
    pub fn subscribe(&self) -> broadcast::Receiver<String> {
        self.sender.subscribe()
    }

    /// Clear all buffered logs
    pub async fn clear(&self) {
        let mut lines = self.lines.lock().await;
        lines.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_buffer_capacity() {
        let buffer = LogBuffer::new(3);
        buffer.push("line1".to_string()).await;
        buffer.push("line2".to_string()).await;
        buffer.push("line3".to_string()).await;
        buffer.push("line4".to_string()).await;

        let lines = buffer.get_lines(None).await;
        assert_eq!(lines, vec!["line2", "line3", "line4"]);
    }

    #[tokio::test]
    async fn test_get_last_n() {
        let buffer = LogBuffer::new(10);
        for i in 0..5 {
            buffer.push(format!("line{}", i)).await;
        }

        let lines = buffer.get_lines(Some(2)).await;
        assert_eq!(lines, vec!["line3", "line4"]);
    }
}
