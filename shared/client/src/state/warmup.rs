use std::sync::{Arc, Mutex};

use psyche_core::NodeIdentity;

use super::{
    evals::{EvalRunner, MaybeRunningEvals, RunningEvals},
    round_state::RoundState,
};

pub struct WarmupStepMetadata {
    pub eval_runner: EvalRunner,
}

impl WarmupStepMetadata {
    pub fn start<T: NodeIdentity>(
        &self,
        evals_or_trainers: impl Into<MaybeRunningEvals>,
        previous_round: Arc<Mutex<RoundState<T>>>,
        current_round: Arc<Mutex<RoundState<T>>>,
    ) -> WarmupStep {
        // reset the transient states
        let mut previous_round = previous_round.lock().unwrap();
        *previous_round = RoundState::default();
        let mut current_round = current_round.lock().unwrap();
        *current_round = RoundState::default();

        let evals = self
            .eval_runner
            .start_if_not_running(evals_or_trainers.into());
        WarmupStep { evals }
    }
}

#[derive(Debug)]
pub struct WarmupStep {
    evals: RunningEvals,
}

impl WarmupStep {
    pub fn finish(self) -> RunningEvals {
        self.evals
    }
}
