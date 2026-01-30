use std::path::PathBuf;
use std::time::Instant;
use tokio::sync::RwLock;

/// Configuration for running a container
#[derive(Clone)]
pub struct RunConfig {
    pub env_file: PathBuf,
    pub coordinator_program_id: String,
    pub local: bool,
    pub entrypoint: Option<String>,
    pub entrypoint_args: Vec<String>,
}

/// Container info when running
pub struct ContainerInfo {
    pub container_id: String,
    pub image: String,
    pub run_id: String,
}

/// Shared state for the daemon
pub struct DaemonState {
    inner: RwLock<DaemonStateInner>,
}

struct DaemonStateInner {
    running: bool,
    container_id: Option<String>,
    image: Option<String>,
    start_time: Option<Instant>,
    run_id: Option<String>,
    shutdown_requested: bool,
    config: Option<RunConfig>,
}

impl DaemonState {
    pub fn new() -> Self {
        Self {
            inner: RwLock::new(DaemonStateInner {
                running: false,
                container_id: None,
                image: None,
                start_time: None,
                run_id: None,
                shutdown_requested: false,
                config: None,
            }),
        }
    }

    pub async fn set_running(&self, container_info: ContainerInfo, config: RunConfig) {
        let mut inner = self.inner.write().await;
        inner.running = true;
        inner.container_id = Some(container_info.container_id);
        inner.image = Some(container_info.image);
        inner.start_time = Some(Instant::now());
        inner.run_id = Some(container_info.run_id);
        inner.shutdown_requested = false;
        inner.config = Some(config);
    }

    pub async fn set_stopped(&self) {
        let mut inner = self.inner.write().await;
        inner.running = false;
        inner.container_id = None;
        inner.image = None;
        inner.start_time = None;
    }

    pub async fn request_shutdown(&self) {
        let mut inner = self.inner.write().await;
        inner.shutdown_requested = true;
    }

    pub async fn is_shutdown_requested(&self) -> bool {
        let inner = self.inner.read().await;
        inner.shutdown_requested
    }

    pub async fn get_status(
        &self,
    ) -> (
        bool,
        Option<String>,
        Option<String>,
        Option<u64>,
        Option<String>,
    ) {
        let inner = self.inner.read().await;
        let uptime = inner.start_time.map(|t| t.elapsed().as_secs());
        (
            inner.running,
            inner.container_id.clone(),
            inner.image.clone(),
            uptime,
            inner.run_id.clone(),
        )
    }

    pub async fn get_container_id(&self) -> Option<String> {
        let inner = self.inner.read().await;
        inner.container_id.clone()
    }

    pub async fn update_container_id(&self, container_id: String) {
        let mut inner = self.inner.write().await;
        inner.container_id = Some(container_id);
        if inner.start_time.is_none() {
            inner.start_time = Some(Instant::now());
        }
    }

    pub async fn update_image(&self, image: String) {
        let mut inner = self.inner.write().await;
        inner.image = Some(image);
    }

    pub async fn get_restart_config(&self) -> Option<RunConfig> {
        let inner = self.inner.read().await;
        inner.config.clone()
    }
}

impl Default for DaemonState {
    fn default() -> Self {
        Self::new()
    }
}
