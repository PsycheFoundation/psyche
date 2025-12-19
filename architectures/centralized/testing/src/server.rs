use crate::{COOLDOWN_TIME, test_utils::sample_rand_run_id};
use crate::{MAX_ROUND_TRAIN_TIME, ROUND_WITNESS_TIME, WARMUP_TIME};
use bytemuck::Zeroable;
use psyche_centralized_server::app::App as ServerApp;
use psyche_centralized_shared::ClientId;
use psyche_coordinator::model::LLMDataLocations;
use psyche_coordinator::{Client, Round};
use psyche_coordinator::{
    Coordinator, CoordinatorConfig, CoordinatorEpochState, RunState, SOLANA_MAX_NUM_CLIENTS,
    model::{Checkpoint, LLM, Model},
};
use psyche_core::FixedVec;
use std::{collections::HashSet, mem::Discriminant, ops::ControlFlow};
use tokio::{
    select,
    sync::{
        mpsc::{self, Receiver},
        oneshot,
    },
};
use tracing::debug;

enum TestingQueryMsg {
    Clients {
        respond_to: oneshot::Sender<FixedVec<Client<ClientId>, SOLANA_MAX_NUM_CLIENTS>>,
    },
    ClientsLen {
        respond_to: oneshot::Sender<usize>,
    },
    PendingClients {
        respond_to: oneshot::Sender<HashSet<ClientId>>,
    },
    PendingClientsLen {
        respond_to: oneshot::Sender<usize>,
    },
    RunState {
        respond_to: oneshot::Sender<RunState>,
    },
    Rounds {
        respond_to: oneshot::Sender<[Round; 4]>,
    },
    RoundsHead {
        respond_to: oneshot::Sender<u32>,
    },
    Epoch {
        respond_to: oneshot::Sender<u16>,
    },
    Checkpoint {
        respond_to: oneshot::Sender<Checkpoint>,
    },
    Coordinator {
        respond_to: oneshot::Sender<Coordinator<ClientId>>,
    },
}

struct CoordinatorServer {
    inner: ServerApp,
    query_chan_receiver: Receiver<TestingQueryMsg>,
    port: u16,
    run_id: String,
}

impl CoordinatorServer {
    pub async fn new(
        query_chan_receiver: Receiver<TestingQueryMsg>,
        min_clients: u16,
        global_batch_size: u16,
        witness_nodes: u16,
    ) -> Self {
        let coordinator_config = CoordinatorConfig {
            warmup_time: WARMUP_TIME,
            cooldown_time: COOLDOWN_TIME,
            max_round_train_time: MAX_ROUND_TRAIN_TIME,
            round_witness_time: ROUND_WITNESS_TIME,
            min_clients,
            init_min_clients: min_clients,
            global_batch_size_start: global_batch_size,
            global_batch_size_end: global_batch_size,
            global_batch_size_warmup_tokens: 0,
            verification_percent: 0,
            witness_nodes,
            total_steps: 100,
            waiting_for_members_extra_time: 2,
            epoch_time: 30,
        };

        let epoch_state = CoordinatorEpochState {
            first_round: true.into(),
            ..CoordinatorEpochState::<ClientId>::zeroed()
        };

        let run_id = sample_rand_run_id();
        let coordinator: Coordinator<ClientId> = Coordinator {
            run_id: run_id.as_str().try_into().unwrap(),
            model: Model::LLM(LLM::dummy()),
            config: coordinator_config,
            epoch_state,
            data_locations: LLMDataLocations::dummy(),
            ..Coordinator::<ClientId>::zeroed()
        };

        debug!("ServerApp::new() waiting...");

        let server = ServerApp::new(
            false,
            coordinator,
            None,
            None,
            None,
            Some(WARMUP_TIME),
            true,
        )
        .await
        .unwrap();
        debug!("ServerApp::new() done!");

        let port = server.get_port();

        Self {
            inner: server,
            query_chan_receiver,
            port,
            run_id,
        }
    }

    pub async fn handle_message(&mut self, msg: TestingQueryMsg) {
        match msg {
            TestingQueryMsg::Clients { respond_to } => {
                let clients = self.inner.get_clients();
                respond_to.send(clients).unwrap();
            }
            TestingQueryMsg::ClientsLen { respond_to } => {
                let clients = self.inner.get_clients();
                respond_to.send(clients.len()).unwrap();
            }
            TestingQueryMsg::PendingClients { respond_to } => {
                let clients = self.inner.get_pending_clients();
                respond_to.send(clients).unwrap();
            }
            TestingQueryMsg::PendingClientsLen { respond_to } => {
                let clients = self.inner.get_pending_clients();
                respond_to.send(clients.len()).unwrap();
            }
            TestingQueryMsg::RunState { respond_to } => {
                let run_state = self.inner.get_run_state();
                respond_to.send(run_state).unwrap();
            }
            TestingQueryMsg::Rounds { respond_to } => {
                let rounds = self.inner.get_rounds();
                respond_to.send(rounds).unwrap();
            }
            TestingQueryMsg::RoundsHead { respond_to } => {
                let rounds = self.inner.get_rounds_head();
                respond_to.send(rounds).unwrap();
            }
            TestingQueryMsg::Epoch { respond_to } => {
                let current_epoch = self.inner.get_current_epoch();
                respond_to.send(current_epoch).unwrap();
            }
            TestingQueryMsg::Checkpoint { respond_to } => {
                let checkpoint = self.inner.get_checkpoint();
                respond_to.send(checkpoint).unwrap();
            }
            TestingQueryMsg::Coordinator { respond_to } => {
                let coordinator = self.inner.get_coordinator();
                respond_to.send(coordinator).unwrap();
            }
        }
    }

    pub async fn run(&mut self) {
        loop {
            select! {
                res = self.inner.poll_next() => {
                    if let ControlFlow::Break(()) = res.unwrap() {
                        break
                    }
                },
                Some(client_msg) = self.query_chan_receiver.recv() => self.handle_message(client_msg).await
            }
        }
    }
}

pub struct CoordinatorServerHandle {
    query_chan_sender: mpsc::Sender<TestingQueryMsg>,
    pub server_port: u16,
    pub run_id: String,
}

impl CoordinatorServerHandle {
    pub async fn new(init_min_clients: u16, global_batch_size: u16, witness_nodes: u16) -> Self {
        debug!("creating coordinator server...");
        let (query_chan_sender, query_chan_receiver) = mpsc::channel(64);

        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_time()
            .enable_io()
            .thread_stack_size(10 * 1024 * 1024)
            .max_blocking_threads(8192)
            .build()
            .unwrap();

        let mut server = rt
            .spawn(CoordinatorServer::new(
                query_chan_receiver,
                init_min_clients,
                global_batch_size,
                witness_nodes,
            ))
            .await
            .unwrap();

        let server_port = server.port;
        let run_id = server.run_id.clone();

        // the above line will stack overflow, for reasons best left to contemplative reflection.
        // as a substitute to madness, we suggest the reader trust us on this point.
        std::thread::spawn(move || {
            rt.block_on(server.run());
        });
        debug!("coordinator server created on port {server_port}");

        Self {
            query_chan_sender,
            server_port,
            run_id,
        }
    }

    pub async fn get_clients(&self) -> FixedVec<Client<ClientId>, SOLANA_MAX_NUM_CLIENTS> {
        let (send, recv) = oneshot::channel();
        let msg = TestingQueryMsg::Clients { respond_to: send };
        let _ = self.query_chan_sender.send(msg).await;
        recv.await.expect("Coordinator actor task has been killed")
    }

    pub async fn get_clients_len(&self) -> usize {
        let (send, recv) = oneshot::channel();
        let msg = TestingQueryMsg::ClientsLen { respond_to: send };
        let _ = self.query_chan_sender.send(msg).await;
        recv.await.expect("Coordinator actor task has been killed")
    }

    pub async fn get_pending_clients(&self) -> HashSet<ClientId> {
        let (send, recv) = oneshot::channel();
        let msg = TestingQueryMsg::PendingClients { respond_to: send };
        let _ = self.query_chan_sender.send(msg).await;
        recv.await.expect("Coordinator actor task has been killed")
    }

    pub async fn get_pending_clients_len(&self) -> usize {
        let (send, recv) = oneshot::channel();
        let msg = TestingQueryMsg::PendingClientsLen { respond_to: send };
        let _ = self.query_chan_sender.send(msg).await;
        recv.await.expect("Coordinator actor task has been killed")
    }

    pub async fn get_run_state(&self) -> RunState {
        let (send, recv) = oneshot::channel::<RunState>();
        let msg = TestingQueryMsg::RunState { respond_to: send };
        let _ = self.query_chan_sender.send(msg).await;
        recv.await.expect("Coordinator actor task has been killed")
    }

    pub async fn get_rounds(&self) -> [Round; 4] {
        let (send, recv) = oneshot::channel::<[Round; 4]>();
        let msg = TestingQueryMsg::Rounds { respond_to: send };
        let _ = self.query_chan_sender.send(msg).await;
        recv.await.expect("Coordinator actor task has been killed")
    }

    pub async fn get_rounds_head(&self) -> u32 {
        let (send, recv) = oneshot::channel::<u32>();
        let msg = TestingQueryMsg::RoundsHead { respond_to: send };
        let _ = self.query_chan_sender.send(msg).await;
        recv.await.expect("Coordinator actor task has been killed")
    }

    pub async fn get_current_epoch(&self) -> u16 {
        let (send, recv) = oneshot::channel::<u16>();
        let msg = TestingQueryMsg::Epoch { respond_to: send };
        let _ = self.query_chan_sender.send(msg).await;
        recv.await.expect("Coordinator actor task has been killed")
    }

    // We only care about checking the checkpoint variant but not the hub repo value so we get the discriminant.
    pub async fn get_checkpoint(&self) -> Discriminant<Checkpoint> {
        let (send, recv) = oneshot::channel::<Checkpoint>();
        let msg = TestingQueryMsg::Checkpoint { respond_to: send };
        let _ = self.query_chan_sender.send(msg).await;
        let checkpoint = recv.await.expect("Coordinator actor task has been killed");
        std::mem::discriminant(&checkpoint)
    }

    pub async fn get_coordinator(&self) -> Coordinator<ClientId> {
        let (send, recv) = oneshot::channel::<Coordinator<ClientId>>();
        let msg = TestingQueryMsg::Coordinator { respond_to: send };
        let _ = self.query_chan_sender.send(msg).await;
        recv.await.expect("Coordinator actor task has been killed")
    }
}
