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

use crate::state::prompt::PromptTask;
pub const PROMPT_TASK_NAME: &str = "Prompt";

#[derive(Debug)]

pub struct ModelTask {
    pub task: EnumModelTask,
}

#[derive(Debug)]
pub enum EnumModelTask {
    EvalTask(EvalTask),
    PromptTask(PromptTask),
}

#[derive(Debug)]
pub struct EvalTask {
    pub task: psyche_eval::PreparedTask,
    results: Arc<RunningAverage>,
    next_index: Arc<AtomicUsize>,
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
            EnumModelTask::EvalTask(task) => &task.task.name(),
            EnumModelTask::PromptTask(_prompt) => PROMPT_TASK_NAME,
        }
    }

    pub fn next_index(&self) -> &Arc<AtomicUsize> {
        match &self.task {
            EnumModelTask::EvalTask(task) => &task.next_index,
            EnumModelTask::PromptTask(prompt) => &prompt.next_index,
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
                let mut eval_model_taks = eval_tasks
                    .into_iter()
                    .map(|task| {
                        let prepared = task.prepare(&tokenizer, None, eval_task_max_docs);
                        Arc::new(ModelTask::new_eval_task(EvalTask {
                            task: prepared,
                            results: Arc::new(RunningAverage::new()),
                            next_index: Arc::new(AtomicUsize::new(0)),
                        }))
                    })
                    .collect::<Vec<_>>();

                let prompt_task = Arc::new(ModelTask::new_prompt_task(PromptTask::new(
                    r"
                    EDWARD:
                    I wonder how our princely father 'scaped,
                    Or whether he be 'scaped away or no
                    From Clifford's and Northumberland's pursuit:
                    Had he been ta'en, we should have heard the news;
                    Had he been slain, we should have heard the news;
                    Or had he 'scaped, methinks we should have heard
                    The happy tidings of his good escape.
                    How fares my brother? why is he so sad?

                    RICHARD:
                    I cannot joy, until I be resolved
                    Where our right valiant father is become.
                    I saw him in the battle range about;
                    And watch'd him how he singled Clifford forth.
                    Methought he bore him in the thickest troop
                    As doth a lion in a herd of neat;
                    Or as a bear, encompass'd round with dogs,
                    Who having pinch'd a few and made them cry,
                    The rest stand all aloof, and bark at him.
                    So fared our father with his enemies;
                    So fled his enemies my warlike father:
                    Methinks, 'tis prize enough to be his son.
                    See how the morning opes her golden gates,
                    And takes her farewell of the glorious sun!
                    How well resembles it the prime of youth,
                    Trimm'd like a younker prancing to his love!

                    EDWARD:
                    Dazzle mine eyes, or do I see three suns?

                    RICHARD:
                    Three glorious suns, each one a perfect sun;
                    Not separated with the racking clouds,
                    But sever'd in a pale clear-shining sky.
                    See, see! they join, embrace, and seem to kiss,
                    As if they vow'd some league inviolable:
                    Now are they but one lamp, one light, one sun.
                    In this the heaven figures some event.

                    EDWARD:
                    'Tis wondrous strange, the like yet never heard of.
                    I think it cites us, brother, to the field,
                    That we, the sons of brave Plantagenet,
                    Each one already blazing by our meeds,
                    Should notwithstanding join our lights together
                    And over-shine the earth as this the world.
                    Whate'er it bodes, henceforward will I bear
                    Upon my target three fair-shining suns.
                    "
                    .to_string(),
                    &tokenizer,
                )));
                eval_model_taks.push(prompt_task);
                eval_model_taks
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
                                            .map(|x| x.next_index().load(Ordering::SeqCst))
                                            .collect::<Vec<_>>(),
                                    )
                                    .collect::<Vec<_>>();
                                iter.shuffle(&mut thread_rng());
                                let span = span!(Level::TRACE, "eval_task").entered();
                                for (eval_task, next_index) in iter {
                                    if cancel.is_cancelled() {
                                        break 'eval_loop;
                                    }

                                    info!(
                                        "Running eval task {} on index {}",
                                        eval_task.name(),
                                        next_index + dp_index
                                    );

                                    match &eval_task.task {
                                        EnumModelTask::EvalTask(eval) => {
                                            eval.run(
                                                &mut trainer,
                                                cancel.clone(),
                                                Some((next_index + dp_index, data_parallelism)),
                                                Some(10),
                                                true,
                                            );
                                        }
                                        // todo prevent parallel prompt execution
                                        EnumModelTask::PromptTask(prompt) => {
                                            prompt.run(&mut trainer, cancel.clone());
                                        }
                                    }
                                    info!("Done eval task {}", eval_task.name());
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
