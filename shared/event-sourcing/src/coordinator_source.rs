use std::task::{Context, Poll};

use tokio::sync::mpsc;

use crate::projection::CoordinatorStateSnapshot;

/// Async source of coordinator state updates.
/// Implement this trait for live backends (e.g. Solana on-chain watcher).
pub trait CoordinatorSource: Send + 'static {
    fn poll_next(&mut self, cx: &mut Context<'_>) -> Poll<Option<CoordinatorStateSnapshot>>;
}

/// Handle for pushing coordinator snapshots into a `ChannelCoordinatorSource`.
pub struct CoordinatorSourceHandle {
    tx: mpsc::UnboundedSender<CoordinatorStateSnapshot>,
}

impl CoordinatorSourceHandle {
    pub fn push(&self, state: CoordinatorStateSnapshot) {
        let _ = self.tx.send(state);
    }
}

/// In-process coordinator source backed by an unbounded channel.
/// Used by the integration test harness and for local testing.
pub struct ChannelCoordinatorSource {
    rx: mpsc::UnboundedReceiver<CoordinatorStateSnapshot>,
}

impl CoordinatorSource for ChannelCoordinatorSource {
    fn poll_next(&mut self, cx: &mut Context<'_>) -> Poll<Option<CoordinatorStateSnapshot>> {
        self.rx.poll_recv(cx)
    }
}

/// Create a linked (handle, source) pair for in-process coordinator feeding.
pub fn coordinator_source_channel() -> (CoordinatorSourceHandle, ChannelCoordinatorSource) {
    let (tx, rx) = mpsc::unbounded_channel();
    (
        CoordinatorSourceHandle { tx },
        ChannelCoordinatorSource { rx },
    )
}
