use crate::{
    Communicator,
    parallelism::{tensor_shard, unsharded_tensor_size},
};

use std::{iter::Iterator, sync::Arc};
use tch::{
    Tensor,
    nn::{Shard, VarStore},
};

#[cfg(feature = "parallelism")]
use crate::parallelism::AllGather;

pub trait Variable {
    fn name(&self) -> &str;
    fn logical_tensor(&self) -> Tensor;
    fn local_tensor(&self) -> Tensor;
    fn gather_full_tensor(&self) -> Tensor;
    fn shard_other_tensor_like_me(&self, tensor: Tensor) -> Tensor;
    fn full_tensor_shape(&self) -> Vec<i64>;
    fn is_sharded(&self) -> bool;
    fn zeros_like(&self, name: String) -> Box<dyn Variable>;
    fn set_grad(&self, tensor: Tensor);
    fn zero_grad(&self);
}

#[derive(Debug)]
pub struct StableVarStoreIterator {
    #[allow(clippy::type_complexity)]
    entries: Vec<(String, Tensor, Option<Shard>, Option<Arc<Communicator>>)>,
}

impl StableVarStoreIterator {
    pub fn new(vs: &VarStore, comm: Option<Arc<Communicator>>) -> Self {
        let variables = vs.variables_.lock().unwrap();

        let mut entries: Vec<_> = variables
            .named_variables
            .iter()
            .map(|(name, tensor)| {
                let shard = variables.shards.get(name).cloned();
                (name.clone(), tensor.shallow_clone(), shard, comm.clone())
            })
            .collect();

        // this is in reverse order! then we can pop off the back as we iterate
        entries.sort_by(|a, b| b.0.cmp(&a.0));

        Self { entries }
    }
}

impl Clone for StableVarStoreIterator {
    fn clone(&self) -> Self {
        Self {
            entries: self
                .entries
                .iter()
                .map(|(a, b, c, d)| (a.clone(), b.shallow_clone(), *c, d.clone()))
                .collect(),
        }
    }
}

impl Iterator for StableVarStoreIterator {
    type Item = Box<dyn Variable>;

    fn next(&mut self) -> Option<Self::Item> {
        self.entries.pop().map(|x| Box::new(x) as Box<dyn Variable>)
    }
}

pub type StableVariableIterator = Box<dyn Iterator<Item = Box<dyn Variable>>>;

impl Variable for (String, Tensor, Option<Shard>, Option<Arc<Communicator>>) {
    fn name(&self) -> &str {
        &self.0
    }

    fn local_tensor(&self) -> Tensor {
        self.1.shallow_clone()
    }

    fn logical_tensor(&self) -> Tensor {
        self.1.shallow_clone()
    }

    fn gather_full_tensor(&self) -> Tensor {
        match &self.2 {
            #[cfg(feature = "parallelism")]
            Some(shard) => {
                assert!(self.3.is_some());
                let shards = (0..shard.world_size)
                    .map(|_| self.1.empty_like())
                    .collect::<Vec<_>>();
                self.1.all_gather(&shards, &self.3);

                crate::parallelism::unshard_tensor(shards, shard)
            }
            #[cfg(not(feature = "parallelism"))]
            Some(_) => panic!("Sharded tensor without parallelism feature?"),
            None => self.1.shallow_clone(),
        }
    }

    fn full_tensor_shape(&self) -> Vec<i64> {
        match &self.2 {
            Some(shard) => unsharded_tensor_size(&self.1.size(), shard),
            None => self.1.size(),
        }
    }

    fn shard_other_tensor_like_me(&self, tensor: Tensor) -> Tensor {
        match &self.2 {
            Some(shard) => tensor_shard(&tensor, shard),
            None => tensor,
        }
    }

    fn is_sharded(&self) -> bool {
        self.2.is_some()
    }

    fn zeros_like(&self, name: String) -> Box<dyn Variable> {
        Box::new((name, self.1.empty_like(), self.2, self.3.clone()))
    }

    fn set_grad(&self, tensor: Tensor) {
        self.1
            .grad()
            .copy_(&self.shard_other_tensor_like_me(tensor));
    }

    fn zero_grad(&self) {
        let grad = self.1.grad();
        if grad.defined() {
            let _ = self.1.grad().zero_();
        }
    }
}
