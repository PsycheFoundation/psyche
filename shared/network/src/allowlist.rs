use std::collections::HashSet;
use std::sync::{Arc, RwLock};

use iroh::EndpointId;

pub trait Allowlist: std::fmt::Debug + Clone {
    fn allowed(&self, addr: EndpointId) -> bool;
}

#[derive(Debug, Clone)]
pub struct AllowAll;

impl Allowlist for AllowAll {
    fn allowed(&self, _addr: EndpointId) -> bool {
        true
    }
}

#[derive(Debug, Clone)]
pub struct AllowDynamic {
    allowed_nodes: Arc<RwLock<HashSet<EndpointId>>>,
}

impl AllowDynamic {
    pub fn new() -> Self {
        AllowDynamic {
            allowed_nodes: Arc::new(RwLock::new(HashSet::new())),
        }
    }

    pub fn with_nodes(nodes: impl IntoIterator<Item = EndpointId>) -> Self {
        AllowDynamic {
            allowed_nodes: Arc::new(RwLock::new(nodes.into_iter().collect())),
        }
    }

    pub fn add(&self, addr: EndpointId) {
        self.allowed_nodes
            .write()
            .expect("RwLock poisoned")
            .insert(addr);
    }

    pub fn remove(&self, addr: &EndpointId) {
        self.allowed_nodes
            .write()
            .expect("RwLock poisoned")
            .remove(addr);
    }

    pub fn set(&self, nodes: impl IntoIterator<Item = EndpointId>) {
        *self.allowed_nodes.write().expect("RwLock poisoned") = nodes.into_iter().collect();
    }

    pub fn clear(&self) {
        self.allowed_nodes.write().expect("RwLock poisoned").clear();
    }
}

impl Allowlist for AllowDynamic {
    fn allowed(&self, addr: EndpointId) -> bool {
        self.allowed_nodes
            .read()
            .expect("RwLock poisoned")
            .contains(&addr)
    }
}

impl Default for AllowDynamic {
    fn default() -> Self {
        Self::new()
    }
}
