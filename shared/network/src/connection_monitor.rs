use iroh::endpoint::{AfterHandshakeOutcome, ConnectionInfo, EndpointHooks};
use iroh::{EndpointId, Watcher};
use n0_future::task::AbortOnDropHandle;
use psyche_metrics::ConnectionType;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::Duration;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};
use tokio::task::JoinSet;
use tracing::{Instrument, debug, info};

#[derive(Debug, Clone)]
pub struct ConnectionData {
    pub endpoint_id: EndpointId,
    pub connection_type: ConnectionType,
    pub latency: Duration,
}

/// track active connections and their metadata
#[derive(Clone, Debug)]
pub struct ConnectionMonitor {
    tx: UnboundedSender<ConnectionInfo>,
    connections: Arc<RwLock<HashMap<EndpointId, ConnectionData>>>,
    _task: Arc<AbortOnDropHandle<()>>,
}

impl EndpointHooks for ConnectionMonitor {
    async fn after_handshake(&self, conn: &ConnectionInfo) -> AfterHandshakeOutcome {
        self.tx.send(conn.clone()).ok();
        AfterHandshakeOutcome::Accept
    }
}

impl Default for ConnectionMonitor {
    fn default() -> Self {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        let connections = Arc::new(RwLock::new(HashMap::new()));
        let connections_clone = connections.clone();

        let task = tokio::spawn(
            Self::run(rx, connections_clone).instrument(tracing::debug_span!("connection_monitor")),
        );

        Self {
            tx,
            connections,
            _task: Arc::new(AbortOnDropHandle::new(task)),
        }
    }
}

impl ConnectionMonitor {
    async fn run(
        mut rx: UnboundedReceiver<ConnectionInfo>,
        connections: Arc<RwLock<HashMap<EndpointId, ConnectionData>>>,
    ) {
        let mut tasks = JoinSet::new();

        loop {
            tokio::select! {
                Some(conn) = rx.recv() => {
                    let remote_id = conn.remote_id();
                    let alpn = String::from_utf8_lossy(conn.alpn()).to_string();

                    let (conn_type, latency) = Self::extract_connection_info(&conn);

                    info!(
                        remote = %remote_id.fmt_short(),
                        %alpn,
                        ?conn_type,
                        latency_ms = latency.as_millis(),
                        "new connection"
                    );

                    {
                        let mut conns = connections.write().unwrap();
                        conns.insert(remote_id, ConnectionData {
                            endpoint_id: remote_id,
                            connection_type: conn_type,
                            latency,
                        });
                    }

                    // spawn a task to monitor this connection continuously
                    let connections_clone = connections.clone();
                    let paths_watcher = conn.paths();
                    tasks.spawn(async move {
                        let mut update_interval = tokio::time::interval(Duration::from_secs(5));
                        update_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

                        loop {
                            tokio::select! {
                                _ = update_interval.tick() => {
                                    let (conn_type, latency) = Self::extract_connection_info_from_watcher(&paths_watcher);

                                    let mut conns = connections_clone.write().unwrap();
                                    if let Some(data) = conns.get_mut(&remote_id) {
                                        let type_changed = data.connection_type != conn_type;
                                        let latency_delta = if latency != Duration::MAX && data.latency != Duration::MAX {
                                            latency.as_millis().abs_diff(data.latency.as_millis())
                                        } else {
                                            0
                                        };

                                        data.connection_type = conn_type;
                                        data.latency = latency;

                                        if type_changed {
                                            info!(
                                                remote = %remote_id.fmt_short(),
                                                new_type = ?conn_type,
                                                latency_ms = latency.as_millis(),
                                                "connection type changed"
                                            );
                                        } else if latency_delta > 50 {
                                            debug!(
                                                remote = %remote_id.fmt_short(),
                                                latency_ms = latency.as_millis(),
                                                delta_ms = latency_delta,
                                                "latency changed"
                                            );
                                        }
                                    }
                                }
                                result = conn.closed() => {
                                    match result {
                                        Some((close_reason, stats)) => {
                                            info!(
                                                remote = %remote_id.fmt_short(),
                                                %alpn,
                                                ?close_reason,
                                                udp_rx = stats.udp_rx.bytes,
                                                udp_tx = stats.udp_tx.bytes,
                                                "connection closed"
                                            );
                                        }
                                        None => {
                                            debug!(
                                                remote = %remote_id.fmt_short(),
                                                %alpn,
                                                "connection closed before tracking started"
                                            );
                                        }
                                    }

                                    let mut conns = connections_clone.write().unwrap();
                                    conns.remove(&remote_id);
                                    break;
                                }
                            }
                        }
                    }.instrument(tracing::Span::current()));
                }
                Some(res) = tasks.join_next(), if !tasks.is_empty() => {
                    res.expect("connection close task panicked");
                }
                else => break,
            }
        }

        while let Some(res) = tasks.join_next().await {
            res.expect("connection close task panicked");
        }
    }

    /// extract connection type and latency from ConnectionInfo
    fn extract_connection_info(conn: &ConnectionInfo) -> (ConnectionType, Duration) {
        let paths_watcher = conn.paths();
        Self::extract_connection_info_from_watcher(&paths_watcher)
    }

    /// extract connection type and latency from a paths watcher
    fn extract_connection_info_from_watcher<T: Watcher<Value = iroh::endpoint::PathInfoList>>(
        paths_watcher: &T,
    ) -> (ConnectionType, Duration) {
        let paths = paths_watcher.peek();

        if paths.is_empty() {
            return (ConnectionType::Direct, Duration::MAX);
        }

        // get minimum RTT across all paths
        let min_rtt = paths.iter().map(|p| p.rtt()).min().unwrap_or(Duration::MAX);

        // determine connection type based on paths
        let has_direct = paths.iter().any(|p| p.is_ip());
        let has_relay = paths.iter().any(|p| p.is_relay());

        let conn_type = match (has_direct, has_relay) {
            (true, true) => ConnectionType::Mixed,
            (true, false) => ConnectionType::Direct,
            (false, true) => ConnectionType::Relay,
            (false, false) => ConnectionType::None,
        };

        (conn_type, min_rtt)
    }

    /// get connection data for a specific endpoint
    pub fn get_connection(&self, endpoint_id: &EndpointId) -> Option<ConnectionData> {
        let conns = self.connections.read().unwrap();
        conns.get(endpoint_id).cloned()
    }

    /// get all active connections
    pub fn get_all_connections(&self) -> Vec<ConnectionData> {
        let conns = self.connections.read().unwrap();
        conns.values().cloned().collect()
    }

    /// get latency for a specific endpoint
    pub fn get_latency(&self, endpoint_id: &EndpointId) -> Option<Duration> {
        let conns = self.connections.read().unwrap();
        conns.get(endpoint_id).map(|data| data.latency)
    }
}
