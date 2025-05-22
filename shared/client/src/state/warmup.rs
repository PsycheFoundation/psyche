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
        previous_round: &mut RoundState<T>,
        current_round: &mut RoundState<T>,
    ) -> WarmupStep {
        // reset the transient states
        *previous_round = RoundState::default();
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
