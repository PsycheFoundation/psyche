use futures::future::try_join_all;
use psyche_core::RunningAverage;
use psyche_eval::{EvalTaskOptions, Task};
use psyche_modeling::Trainer;
use rand::{seq::SliceRandom, thread_rng};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use thiserror::Error;
use tokenizers::Tokenizer;
use tokio::{
    sync::{Notify, RwLock},
    task::{JoinError, JoinHandle},
};
use tokio_util::sync::CancellationToken;
use tracing::{error, info, span, trace, Level};

#[derive(Debug)]
pub struct EvalTask {
    task: psyche_eval::PreparedTask,
    results: Arc<RunningAverage>,
    next_index: Arc<AtomicUsize>,
}

impl EvalTask {
    pub fn task(&self) -> &psyche_eval::PreparedTask {
        &self.task
    }

    pub fn results(&self) -> &RunningAverage {
        &self.results
    }

    pub fn run(
        &self,
        trainer: &mut Trainer,
        cancel: CancellationToken,
        skip_and_step_by: Option<(usize, usize)>,
        limit: Option<usize>,
        loop_if_empty: bool,
    ) {
        let result = self.task.run(
            EvalTaskOptions {
                model: trainer,
                skip_and_step_by,
                live_results: Some(self.results.clone()),
                cancel: Some(cancel),
                limit,
                loop_if_empty,
            },
            false,
        );
        self.next_index
            .fetch_max(result.next_index, Ordering::SeqCst);
    }
}

#[derive(Debug)]
struct LoadingState {
    state: RwLock<LoadingStateInner>,
    loaded_notify: Notify,
}

#[derive(Debug)]
enum LoadingStateInner {
    Loading,
    Done(Vec<Arc<EvalTask>>),
    Failed(JoinError),
}

#[derive(Debug, Clone)]
pub struct EvalRunner {
    tasks: Arc<LoadingState>,
    data_parallelism: usize,
}

impl EvalRunner {
    pub fn new(
        eval_tasks: Vec<Task>,
        tokenizer: Arc<Tokenizer>,
        eval_task_max_docs: Option<usize>,
        data_parallelism: usize,
    ) -> Self {
        let tasks = Arc::new(LoadingState {
            state: RwLock::new(LoadingStateInner::Loading),
            loaded_notify: Notify::new(),
        });
        let tasks_clone = tasks.clone();

        tokio::spawn(async move {
            let result = tokio::task::spawn_blocking(move || {
                eval_tasks
                    .into_iter()
                    .map(|task| {
                        let prepared = task.prepare(&tokenizer, None, eval_task_max_docs);
                        Arc::new(EvalTask {
                            task: prepared,
                            results: Arc::new(RunningAverage::new()),
                            next_index: Arc::new(AtomicUsize::new(0)),
                        })
                    })
                    .collect::<Vec<_>>()
            })
            .await;

            let mut state = tasks_clone.state.write().await;
            *state = match result {
                Ok(tasks) => {
                    info!("Eval tasks loaded successfully");
                    LoadingStateInner::Done(tasks)
                }
                Err(err) => {
                    error!("Failed to load eval tasks: {err:#}");
                    LoadingStateInner::Failed(err)
                }
            };
            tasks_clone.loaded_notify.notify_one();
        });

        Self {
            tasks,
            data_parallelism,
        }
    }

    async fn wait_for_tasks(
        tasks: Arc<LoadingState>,
        cancel: &CancellationToken,
    ) -> Option<Vec<Arc<EvalTask>>> {
        loop {
            // First check if already done
            {
                let state = tasks.state.read().await;
                match &*state {
                    LoadingStateInner::Done(tasks) => {
                        if tasks.is_empty() {
                            return None;
                        }
                        return Some(tasks.clone());
                    }
                    LoadingStateInner::Failed(err) => {
                        error!("Failed to load eval tasks: {err:#}");
                        return None;
                    }
                    LoadingStateInner::Loading => {
                        // Wait for either cancellation or completion
                        tokio::select! {
                            _ = cancel.cancelled() => {
                                trace!("Eval tasks early-cancelled");
                                return None;
                            }
                            _ = tasks.loaded_notify.notified() => {
                                // Loop around to see if we failed or suceeded to load
                                continue;
                            }
                        }
                    }
                }
            }
        }
    }

    pub fn tasks(&self) -> Option<Vec<Arc<EvalTask>>> {
        // Synchronous access to tasks if they're ready
        match &*self.tasks.state.try_read().ok()? {
            LoadingStateInner::Done(tasks) => Some(tasks.clone()),
            LoadingStateInner::Loading | LoadingStateInner::Failed(_) => None,
        }
    }

    pub fn start_if_not_running(&self, trainers: MaybeRunningEvals) -> RunningEvals {
        match trainers {
            MaybeRunningEvals::NotRunning(trainers) => self.start(trainers),
            MaybeRunningEvals::Running(evals) => evals,
        }
    }

    pub fn start(&self, trainers: Vec<Trainer>) -> RunningEvals {
        let cancel = CancellationToken::new();
        trace!("Starting evals!");

        RunningEvals {
            cancel: cancel.clone(),
            eval_trainers: trainers
                .into_iter()
                .enumerate()
                .map(|(dp_index, mut trainer)| {
                    let data_parallelism = self.data_parallelism;
                    let cancel = cancel.clone();
                    let tasks = self.tasks.clone();

                    tokio::task::spawn(async move {
                        let prepared_eval_tasks = match Self::wait_for_tasks(tasks, &cancel).await {
                            Some(tasks) => tasks,
                            None => return Ok(trainer), // Return early if cancelled or failed
                        };

                        tokio::task::spawn_blocking(move || {
                            'eval_loop: while !cancel.is_cancelled() {
                                let mut iter = prepared_eval_tasks
                                    .iter()
                                    .zip(
                                        prepared_eval_tasks
                                            .iter()
                                            .map(|x| x.next_index.load(Ordering::SeqCst))
                                            .collect::<Vec<_>>(),
                                    )
                                    .collect::<Vec<_>>();
                                iter.shuffle(&mut thread_rng());
                                let span = span!(Level::TRACE, "eval_task").entered();
                                for (eval_task, next_index) in iter {
                                    if cancel.is_cancelled() {
                                        break 'eval_loop;
                                    }
                                    trace!(
                                        "Running eval task {} on index {}",
                                        eval_task.task.name(),
                                        next_index + dp_index
                                    );
                                    eval_task.run(
                                        &mut trainer,
                                        cancel.clone(),
                                        Some((next_index + dp_index, data_parallelism)),
                                        Some(10),
                                        true,
                                    );
                                    trace!("Done eval task {}", eval_task.task.name());
                                }
                                drop(span);
                            }
                            trainer
                        })
                        .await
                        .map_err(EvalError::JoinError)
                    })
                })
                .collect(),
        }
    }
}

#[derive(Debug)]
pub struct RunningEvals {
    cancel: CancellationToken,
    eval_trainers: Vec<JoinHandle<Result<Trainer, EvalError>>>,
}

#[derive(Debug)]
pub enum MaybeRunningEvals {
    Running(RunningEvals),
    NotRunning(Vec<Trainer>),
}

impl MaybeRunningEvals {
    pub fn is_empty(&self) -> bool {
        match self {
            Self::Running(_) => false,
            Self::NotRunning(t) => t.is_empty(),
        }
    }
    pub async fn stop_evals(self) -> Result<Vec<Trainer>, EvalError> {
        match self {
            MaybeRunningEvals::Running(evals) => evals.stop_evals().await,
            MaybeRunningEvals::NotRunning(trainers) => Ok(trainers),
        }
    }
}

impl From<RunningEvals> for MaybeRunningEvals {
    fn from(evals: RunningEvals) -> Self {
        Self::Running(evals)
    }
}

impl From<Vec<Trainer>> for MaybeRunningEvals {
    fn from(trainers: Vec<Trainer>) -> Self {
        Self::NotRunning(trainers)
    }
}

#[derive(Debug, Error)]
pub enum EvalError {
    #[error("Failed to join thread")]
    JoinError(#[from] JoinError),
}

impl RunningEvals {
    pub async fn stop_evals(self) -> Result<Vec<Trainer>, EvalError> {
        self.cancel.cancel();

        try_join_all(self.eval_trainers)
            .await?
            .into_iter()
            .collect()
    }
}
