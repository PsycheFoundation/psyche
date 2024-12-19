use crate::traits::Backend;
use anyhow::Result;
use psyche_coordinator::{Client, Coordinator, RunState};
use psyche_network::NetworkableNodeIdentity;
use std::{collections::HashMap, mem::replace};

pub struct BackendWatcher<T, B>
where
    T: NetworkableNodeIdentity,
    B: Backend<T> + Send + 'static,
{
    backend: B,
    client_lookup: HashMap<[u8; 32], Client<T>>,
    state: Option<Coordinator<T>>,
}

impl<T, B> BackendWatcher<T, B>
where
    T: NetworkableNodeIdentity,
    B: Backend<T> + Send + 'static,
{
    pub fn new(backend: B) -> Self {
        Self {
            backend,
            client_lookup: HashMap::new(),
            state: None,
        }
    }

    /// # Cancel safety
    ///
    /// This method is cancel safe. If `poll_next` is used as the event in a
    /// [`tokio::select!`](crate::select) statement and some other branch
    /// completes first, it is guaranteed that no state changes are missed.
    pub async fn poll_next(&mut self) -> Result<(Option<Coordinator<T>>, &Coordinator<T>)> {
        let new_state = self.backend.wait_for_new_state().await?;
        if new_state.run_state == RunState::Warmup {
            self.client_lookup = HashMap::from_iter(
                new_state
                    .clients
                    .iter()
                    .map(|client| (*client.id.get_p2p_public_key(), *client)),
            );
        }
        let prev = replace(&mut self.state, Some(new_state));
        Ok((prev, self.state.as_ref().unwrap()))
    }

    pub fn backend(&self) -> &B {
        &self.backend
    }

    pub fn backend_mut(&mut self) -> &mut B {
        &mut self.backend
    }

    pub fn get_client_for_p2p_public_key(&self, p2p_public_key: &[u8; 32]) -> Option<&Client<T>> {
        self.client_lookup.get(p2p_public_key)
    }
}
