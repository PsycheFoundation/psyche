use std::sync::Arc;
use std::time::Duration;

use crate::subprocess_watcher::ProcessRegistry;
use crate::utils::SolanaTestClient;
use tokio::sync::Mutex;

#[derive(Clone, Debug)]
pub enum ChaosAction {
    Pause {
        duration_secs: i64,
        targets: Vec<String>,
    },
    Delay {
        duration_secs: i64,
        latency_ms: i64,
        targets: Vec<String>,
    },
    Kill {
        targets: Vec<String>,
    },
    PacketLoss {
        duration_secs: i64,
        loss_percent: f64,
        correlation: f64,
        targets: Vec<String>,
    },
}

#[derive(Clone)]
pub struct ChaosScheduler {
    registry: Arc<Mutex<ProcessRegistry>>,
    solana_client: Arc<SolanaTestClient>,
}

impl ChaosScheduler {
    pub fn new(
        registry: Arc<Mutex<ProcessRegistry>>,
        solana_client: Arc<SolanaTestClient>,
    ) -> Self {
        Self {
            registry,
            solana_client,
        }
    }

    pub async fn schedule_chaos(self, action: ChaosAction, chaos_step: u64) {
        if chaos_step == 0 {
            self.apply_chaos(&action).await;
            let targets = action_targets(&action);
            println!("Chaos correctly applied for processes: {targets:?}");
        } else {
            tokio::spawn({
                async move {
                    loop {
                        let current_step = self.solana_client.get_last_step().await;
                        if current_step >= chaos_step as u32 {
                            self.apply_chaos(&action).await;
                            let targets = action_targets(&action);
                            println!(
                                "Chaos correctly applied for processes {targets:?} in step {chaos_step}"
                            );
                            break;
                        }
                        tokio::time::sleep(Duration::from_secs(1)).await;
                    }
                }
            });
        }
    }

    async fn apply_chaos(&self, action: &ChaosAction) {
        match action {
            ChaosAction::Pause {
                duration_secs,
                targets,
            } => {
                // SIGSTOP to pause, sleep, SIGCONT to resume
                let registry = self.registry.lock().await;
                for target in targets {
                    if let Some(pid) = registry.get_pid(target) {
                        unsafe {
                            libc::kill(pid as i32, libc::SIGSTOP);
                        }
                        println!("Paused process {target} (pid {pid})");
                    } else {
                        println!("Warning: process {target} not found in registry");
                    }
                }
                let pids: Vec<(String, u32)> = targets
                    .iter()
                    .filter_map(|t| registry.get_pid(t).map(|pid| (t.clone(), pid)))
                    .collect();
                drop(registry);

                let duration = *duration_secs;
                tokio::spawn(async move {
                    tokio::time::sleep(Duration::from_secs(duration as u64)).await;
                    for (name, pid) in &pids {
                        unsafe {
                            libc::kill(*pid as i32, libc::SIGCONT);
                        }
                        println!("Resumed process {name} (pid {pid})");
                    }
                });
            }
            ChaosAction::Kill { targets } => {
                let registry = self.registry.lock().await;
                for target in targets {
                    if let Some(pid) = registry.get_pid(target) {
                        unsafe {
                            libc::kill(pid as i32, libc::SIGKILL);
                        }
                        println!("Killed process {target} (pid {pid})");
                    } else {
                        println!("Warning: process {target} not found in registry");
                    }
                }
            }
            ChaosAction::Delay { targets, .. } => {
                // Network delay requires tc/netem (CAP_NET_ADMIN) — not available without docker.
                println!(
                    "Warning: ChaosAction::Delay is not supported in subprocess mode (needs CAP_NET_ADMIN). Targets: {targets:?}"
                );
            }
            ChaosAction::PacketLoss { targets, .. } => {
                // Packet loss requires tc/netem (CAP_NET_ADMIN) — not available without docker.
                println!(
                    "Warning: ChaosAction::PacketLoss is not supported in subprocess mode (needs CAP_NET_ADMIN). Targets: {targets:?}"
                );
            }
        }
    }
}

fn action_targets(action: &ChaosAction) -> &Vec<String> {
    match action {
        ChaosAction::Pause { targets, .. }
        | ChaosAction::Delay { targets, .. }
        | ChaosAction::Kill { targets, .. }
        | ChaosAction::PacketLoss { targets, .. } => targets,
    }
}
