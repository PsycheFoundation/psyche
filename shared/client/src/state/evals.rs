use futures::future::try_join_all;
use psyche_core::RunningAverage;
use psyche_eval::{EvalTaskOptions, Task};
use psyche_modeling::Trainer;
use rand::{Rng, seq::SliceRandom, thread_rng};
use std::sync::Arc;
use thiserror::Error;
use tokenizers::Tokenizer;
use tokio::{
    sync::{Notify, RwLock},
    task::{JoinError, JoinHandle},
};
use tokio_util::sync::CancellationToken;
use tracing::{Level, error, info, span, trace};

use crate::state::{prompt::PromptTask, prompt_texts::PROMPT_TEXTS};
pub const PROMPT_TASK_NAME: &str = "Prompt";

#[derive(Debug)]

pub struct ModelTask {
    pub task: EnumModelTask,
}

#[allow(clippy::large_enum_variant)]
#[derive(Debug)]
pub enum EnumModelTask {
    EvalTask(EvalTask),
    PromptTask(PromptTask),
}

#[derive(Debug)]
pub struct EvalTask {
    pub task: psyche_eval::PreparedTask,
    results: Arc<RunningAverage>,
    next_indices: std::sync::Mutex<Vec<usize>>,
}

impl ModelTask {
    pub fn new_eval_task(eval_task: EvalTask) -> Self {
        Self {
            task: EnumModelTask::EvalTask(eval_task),
        }
    }
    pub fn new_prompt_task(prompt_task: PromptTask) -> Self {
        Self {
            task: EnumModelTask::PromptTask(prompt_task),
        }
    }

    pub fn name(&self) -> &str {
        match &self.task {
            EnumModelTask::EvalTask(task) => task.task.name(),
            EnumModelTask::PromptTask(_prompt) => PROMPT_TASK_NAME,
        }
    }
}
impl EvalTask {
    pub fn run(
        &self,
        trainer: &mut Trainer,
        cancel: CancellationToken,
        skip_and_step_by: Option<(usize, usize)>,
        limit: Option<usize>,
    ) {
        let result = self.task.run(
            EvalTaskOptions {
                model: trainer,
                skip_and_step_by,
                live_results: Some(self.results.clone()),
                cancel: Some(cancel),
                limit,
            },
            false,
        );
        self.next_indices
            .lock()
            .unwrap()
            .insert(0, result.next_index);
    }

    pub fn results(&self) -> &RunningAverage {
        &self.results
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
    Done(Vec<Arc<ModelTask>>),
    Failed(JoinError),
}

#[derive(Debug, Clone)]
pub struct ModelTaskRunner {
    tasks: Arc<LoadingState>,
    data_parallelism: usize,
}

impl ModelTaskRunner {
    pub fn new(
        eval_tasks: Vec<Task>,
        prompt_task: bool,
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
                let mut model_tasks = eval_tasks
                    .into_iter()
                    .map(|task| {
                        let prepared = task.prepare(&tokenizer, eval_task_max_docs);
                        tracing::info!("Loading evaluation task: {}", &prepared.name());

                        Arc::new(ModelTask::new_eval_task(EvalTask {
                            task: prepared,
                            results: Arc::new(RunningAverage::new()),
                            next_indices: std::sync::Mutex::new(Vec::from_iter(
                                0..data_parallelism,
                            )),
                        }))
                    })
                    .collect::<Vec<_>>();

                if prompt_task {
                    let mut rng = rand::thread_rng();

                    let prompt_index = rng.gen_range(0..PROMPT_TEXTS.len());
                    tracing::info!(
                        "Loading prompt task, selected prompt index {}",
                        prompt_index
                    );

                    let prompt_task = Arc::new(ModelTask::new_prompt_task(PromptTask::new(
                        prompt_index,
                        PROMPT_TEXTS[prompt_index].to_string(),
                        &tokenizer,
                    )));
                    model_tasks.push(prompt_task);
                }

                model_tasks
            })
            .await;

            let mut state = tasks_clone.state.write().await;
            *state = match result {
                Ok(tasks) => {
                    info!("Model tasks loaded successfully");
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
    ) -> Option<Vec<Arc<ModelTask>>> {
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

    pub fn tasks(&self) -> Option<Vec<Arc<ModelTask>>> {
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
                .map(|mut trainer| {
                    let data_parallelism = self.data_parallelism;
                    let cancel = cancel.clone();
                    let tasks = self.tasks.clone();

                    tokio::task::spawn(async move {
                        let mut model_tasks = match Self::wait_for_tasks(tasks, &cancel).await {
                            Some(tasks) => tasks,
                            None => return Ok(trainer), // Return early if cancelled or failed
                        };

                        tokio::task::spawn_blocking(move || {
                            'eval_loop: while !cancel.is_cancelled() {
                                model_tasks.shuffle(&mut thread_rng());
                                let span = span!(Level::TRACE, "eval_task").entered();
                                for model_task in &model_tasks {
                                    if cancel.is_cancelled() {
                                        break 'eval_loop;
                                    }

                                    // prompt task will run only on the first trainer to prevent parallel execution.

                                    match &model_task.task {
                                        EnumModelTask::EvalTask(eval_task) => {
                                            let next_index = {
                                                let mut next_indices =
                                                    eval_task.next_indices.lock().unwrap();
                                                next_indices.pop().unwrap()
                                            };
                                            trace!(
                                                "Running eval task {} on index {}",
                                                eval_task.task.name(),
                                                next_index
                                            );
                                            // mmlu_pro takes a very long time so let's use limit=1 for that one
                                            let limit = if eval_task.task.name() == "mmlu_pro" {
                                                Some(1)
                                            } else {
                                                Some(10)
                                            };
                                            eval_task.run(
                                                &mut trainer,
                                                cancel.clone(),
                                                Some((next_index, data_parallelism)),
                                                limit,
                                            );
                                            trace!("Done eval task {}", eval_task.task.name());
                                        }
                                        EnumModelTask::PromptTask(prompt) => {
                                            let mut is_running = prompt.is_running.lock().unwrap();
                                            if *is_running {
                                                continue;
                                            } else {
                                                *is_running = true;
                                            }
                                            drop(is_running);
                                            trace!(
                                                "Running {} task on prompt index: {}",
                                                model_task.name(),
                                                prompt.selected_prompt
                                            );

                                            prompt.run(&mut trainer, cancel.clone());
                                            *prompt.is_running.lock().unwrap() = false;
                                        }
                                    }
                                    trace!("Done model task {}", model_task.name());
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
