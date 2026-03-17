use super::{
    evals::{MaybeRunningEvals, ModelTaskRunner, RunningEvals},
    round_state::RoundState,
};

pub struct WarmupStepMetadata {
    pub model_task_runner: ModelTaskRunner,
}

impl WarmupStepMetadata {
    pub fn start(
        &self,
        evals_or_trainers: impl Into<MaybeRunningEvals>,
        previous_round: &mut RoundState,
        current_round: &mut RoundState,
    ) -> WarmupStep {
        // reset the transient states
        *previous_round = RoundState::default();
        *current_round = RoundState::default();

        let evals = self
            .model_task_runner
            .start_if_not_running(evals_or_trainers.into());
        WarmupStep {
            evals,
            sent_finished: false,
            sent_witness: false,
        }
    }
}

#[derive(Debug)]
pub struct WarmupStep {
    evals: RunningEvals,
    pub sent_finished: bool,
    pub sent_witness: bool,
}

impl WarmupStep {
    pub fn finish(self) -> RunningEvals {
        self.evals
    }
}
