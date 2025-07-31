use anyhow::Result;
use iroh_n0des::{N0de, Registry};
use psyche_centralized_testing::{client::ClientHandle, server::CoordinatorServerHandle};
use psyche_network::Endpoint;
pub enum CentralizedPsycheNode {
    Server(CoordinatorServerHandle),
    Client(ClientHandle),
}

enum NodeConfig {
    Server {
        init_min_clients: u16,
        global_batch_size: u16,
        witness_nodes: u16,
    },
    Client {
        training_delay_secs: u64,
    },
}

impl N0de for CentralizedPsycheNode {
    async fn spawn(ep: Endpoint, metrics: &mut Registry, ctx: Context<NodeConfig>) -> Result<Self> {
        Ok(match ctx.node_config {
            NodeConfig::Client {
                training_delay_secs,
            } => {
                // how to get data from the server spawn?
                // can we enforce that nodes are spawned in a certain order?
                // should i spawn the server outside this config somewhere?
                // could `ctx` contain previously-spawned nodes?
                // are nodes spawned synchronously or not?
                Self::Client(
                    ClientHandle::new_with_training_delay(server_port, run_id, training_delay_secs)
                        .await,
                )
            }
            NodeConfig::Server {
                init_min_clients,
                global_batch_size,
                witness_nodes,
            } => Self::Server(
                CoordinatorServerHandle::new(init_min_clients, global_batch_size, witness_nodes)
                    .await,
            ),
        })
    }

    async fn shutdown(&mut self) -> Result<()> {
        Ok(())
    }
}
