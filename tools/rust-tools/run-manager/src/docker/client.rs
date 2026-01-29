use anyhow::{Context, Result};
use bollard::Docker;
use bollard::container::{
    Config, CreateContainerOptions, LogOutput, LogsOptions, RemoveContainerOptions,
    StartContainerOptions, StopContainerOptions, WaitContainerOptions,
};
use bollard::exec::{CreateExecOptions, StartExecResults};
use bollard::image::CreateImageOptions;
use bollard::models::{DeviceMapping, DeviceRequest, HostConfig};
use futures_util::StreamExt;
use std::path::Path;
use tracing::{error, info};

/// Wrapper around Bollard's Docker client providing high-level operations
pub struct DockerClient {
    docker: Docker,
}

impl DockerClient {
    /// Create a new Docker client and verify the connection
    pub fn new() -> Result<Self> {
        let docker = Docker::connect_with_local_defaults()
            .context("Failed to connect to Docker. Is Docker installed and running?")?;

        Ok(Self { docker })
    }

    /// Verify Docker is accessible by checking the version
    pub async fn verify_connection(&self) -> Result<()> {
        self.docker
            .version()
            .await
            .context("Failed to get Docker version. Is Docker running?")?;
        Ok(())
    }

    /// Pull an image from a registry with streaming progress output
    pub async fn pull_image(&self, image_name: &str) -> Result<()> {
        info!("Pulling image: {}", image_name);

        // Parse image name into repository and tag
        let (repo, tag) = if let Some(at_pos) = image_name.find('@') {
            // Image with digest: repo@sha256:...
            let repo = &image_name[..at_pos];
            let digest = &image_name[at_pos + 1..];
            (repo.to_string(), digest.to_string())
        } else if let Some(colon_pos) = image_name.rfind(':') {
            // Check if this colon is part of a port number (e.g., registry:5000/image)
            let after_colon = &image_name[colon_pos + 1..];
            if after_colon.contains('/') {
                // Colon is part of port, no tag specified
                (image_name.to_string(), "latest".to_string())
            } else {
                // Colon separates repo from tag
                (image_name[..colon_pos].to_string(), after_colon.to_string())
            }
        } else {
            (image_name.to_string(), "latest".to_string())
        };

        let options = CreateImageOptions {
            from_image: repo,
            tag,
            ..Default::default()
        };

        let mut stream = self.docker.create_image(Some(options), None, None);

        while let Some(result) = stream.next().await {
            match result {
                Ok(info) => {
                    // Print progress information
                    if let Some(status) = &info.status {
                        if let Some(progress) = &info.progress {
                            println!("{}: {}", status, progress);
                        } else if let Some(id) = &info.id {
                            println!("{}: {}", id, status);
                        } else {
                            println!("{}", status);
                        }
                    }
                }
                Err(e) => {
                    return Err(anyhow::anyhow!("Docker pull failed: {}", e));
                }
            }
        }

        info!("Successfully pulled image: {}", image_name);
        Ok(())
    }

    /// Create and start a container, returning the container ID
    pub async fn run_container(
        &self,
        image_name: &str,
        env_vars: Vec<String>,
        env_file_path: &Path,
        scratch_dir: Option<&str>,
        entrypoint: Option<&str>,
        cmd_args: Option<Vec<String>>,
    ) -> Result<String> {
        info!("Creating container from image: {}", image_name);

        // Read environment variables from env file
        let mut all_env = env_vars;
        if env_file_path.exists() {
            let env_content =
                std::fs::read_to_string(env_file_path).context("Failed to read env file")?;
            for line in env_content.lines() {
                let line = line.trim();
                // Skip empty lines and comments
                if !line.is_empty() && !line.starts_with('#') && line.contains('=') {
                    all_env.push(line.to_string());
                }
            }
        }

        // Build bind mounts
        let mut binds = Vec::new();
        if let Some(dir) = scratch_dir {
            binds.push(format!("{}:/scratch", dir));
        }

        // GPU device request (equivalent to --gpus=all)
        let device_requests = vec![DeviceRequest {
            driver: Some("nvidia".to_string()),
            count: Some(-1), // -1 means all GPUs
            capabilities: Some(vec![vec!["gpu".to_string()]]),
            ..Default::default()
        }];

        // InfiniBand device mapping
        let devices = vec![DeviceMapping {
            path_on_host: Some("/dev/infiniband".to_string()),
            path_in_container: Some("/dev/infiniband".to_string()),
            cgroup_permissions: Some("rwm".to_string()),
        }];

        let host_config = HostConfig {
            network_mode: Some("host".to_string()),
            shm_size: Some(1024 * 1024 * 1024), // 1GB
            privileged: Some(true),
            device_requests: Some(device_requests),
            devices: Some(devices),
            binds: if binds.is_empty() { None } else { Some(binds) },
            ..Default::default()
        };

        let config = Config {
            image: Some(image_name.to_string()),
            env: Some(all_env),
            entrypoint: entrypoint.map(|e| vec![e.to_string()]),
            cmd: cmd_args,
            host_config: Some(host_config),
            ..Default::default()
        };

        let container = self
            .docker
            .create_container(None::<CreateContainerOptions<String>>, config)
            .await
            .context("Failed to create container")?;

        let container_id = container.id;

        self.docker
            .start_container(&container_id, None::<StartContainerOptions<String>>)
            .await
            .context("Failed to start container")?;

        info!("Started container: {}", container_id);
        Ok(container_id)
    }

    /// Stream container logs to stdout/stderr
    pub async fn stream_logs(&self, container_id: &str) -> Result<()> {
        info!("Streaming logs for container: {}", container_id);

        let options = LogsOptions::<String> {
            follow: true,
            stdout: true,
            stderr: true,
            ..Default::default()
        };

        let mut stream = self.docker.logs(container_id, Some(options));

        while let Some(result) = stream.next().await {
            match result {
                Ok(output) => match output {
                    LogOutput::StdOut { message } => {
                        print!("{}", String::from_utf8_lossy(&message));
                    }
                    LogOutput::StdErr { message } => {
                        eprint!("{}", String::from_utf8_lossy(&message));
                    }
                    LogOutput::Console { message } => {
                        print!("{}", String::from_utf8_lossy(&message));
                    }
                    LogOutput::StdIn { .. } => {}
                },
                Err(e) => {
                    // Log stream ended or error - this is normal when container exits
                    if !e.to_string().contains("broken pipe") {
                        error!("Log stream error: {}", e);
                    }
                    break;
                }
            }
        }

        Ok(())
    }

    /// Wait for a container to exit and return its exit code
    pub async fn wait_for_container(&self, container_id: &str) -> Result<i32> {
        let options = WaitContainerOptions {
            condition: "not-running",
        };

        let mut stream = self.docker.wait_container(container_id, Some(options));

        if let Some(result) = stream.next().await {
            match result {
                Ok(response) => {
                    let exit_code = response.status_code as i32;
                    Ok(exit_code)
                }
                Err(e) => Err(anyhow::anyhow!("Docker wait failed: {}", e)),
            }
        } else {
            Err(anyhow::anyhow!("No response from docker wait"))
        }
    }

    /// Stop a running container
    pub async fn stop_container(&self, container_id: &str) -> Result<()> {
        let options = StopContainerOptions { t: 10 }; // 10 second timeout

        match self
            .docker
            .stop_container(container_id, Some(options))
            .await
        {
            Ok(_) => Ok(()),
            Err(e) => {
                // Container might already be stopped
                let err_str = e.to_string();
                if err_str.contains("is not running") || err_str.contains("No such container") {
                    Ok(())
                } else {
                    error!("Warning: Docker stop failed: {}", e);
                    Ok(()) // Don't fail on stop errors
                }
            }
        }
    }

    /// Remove a container
    pub async fn remove_container(&self, container_id: &str) -> Result<()> {
        let options = RemoveContainerOptions {
            force: true,
            ..Default::default()
        };

        match self
            .docker
            .remove_container(container_id, Some(options))
            .await
        {
            Ok(_) => Ok(()),
            Err(e) => {
                let err_str = e.to_string();
                if err_str.contains("No such container") {
                    Ok(())
                } else {
                    error!("Warning: Docker rm failed: {}", e);
                    Ok(()) // Don't fail on remove errors
                }
            }
        }
    }

    /// Stop and remove a container
    pub async fn stop_and_remove_container(&self, container_id: &str) -> Result<()> {
        info!("Stopping and removing container: {}", container_id);
        self.stop_container(container_id).await?;
        self.remove_container(container_id).await?;
        Ok(())
    }

    /// Execute a command in a running container and return success status
    pub async fn exec_in_container(&self, container_name: &str, cmd: Vec<&str>) -> Result<bool> {
        let config = CreateExecOptions {
            cmd: Some(cmd),
            attach_stdout: Some(true),
            attach_stderr: Some(true),
            ..Default::default()
        };

        let exec = self
            .docker
            .create_exec(container_name, config)
            .await
            .context("Failed to create exec")?;

        let result = self
            .docker
            .start_exec(&exec.id, None)
            .await
            .context("Failed to start exec")?;

        // Consume the output stream
        if let StartExecResults::Attached { mut output, .. } = result {
            while output.next().await.is_some() {
                // Discard output
            }
        }

        // Check exec exit code
        let inspect = self
            .docker
            .inspect_exec(&exec.id)
            .await
            .context("Failed to inspect exec")?;

        Ok(inspect.exit_code == Some(0))
    }
}
