use crate::HubUploadInfo;

use psyche_coordinator::{
    Coordinator,
    model::{self, HubRepo},
};
use psyche_core::{FixedString, NodeIdentity};
use psyche_data_provider::{UploadModelError, upload_model_repo_async};
use psyche_modeling::{
    SaveSafetensorsError, Trainer, TrainerThreadCommunicationError, save_tensors_into_safetensors,
};
use std::{
    cmp::Reverse,
    collections::{BinaryHeap, HashMap},
    path::PathBuf,
    sync::Arc,
};
use tch::Tensor;
use thiserror::Error;
use tokio::{
    sync::{Mutex, mpsc},
    task::JoinHandle,
};
use tracing::{Instrument, error, info, info_span, warn};

use super::{
    CheckpointConfig,
    evals::{ModelTaskRunner, RunningEvals},
};

#[derive(Error, Debug)]
pub enum CooldownError {
    #[error("no trainers available for checkpointing!")]
    NoTrainers,

    #[error("checkpointing thread crashed")]
    CheckpointThreadCrashed,

    #[error("error while checkpointing: {0}")]
    Checkpoint(#[from] CheckpointError),
}

pub struct CooldownStepMetadata {
    tx_checkpoint: mpsc::UnboundedSender<model::HubRepo>,
    tx_model: mpsc::UnboundedSender<HashMap<String, Tensor>>,
    checkpoint_info: Option<CheckpointConfig>,
    checkpoint_extra_files: Vec<PathBuf>,

    model_task_runner: ModelTaskRunner,
    delete_queue: Arc<Mutex<BinaryHeap<Reverse<u32>>>>,
}

impl CooldownStepMetadata {
    pub fn new(
        tx_checkpoint: mpsc::UnboundedSender<model::HubRepo>,
        tx_model: mpsc::UnboundedSender<HashMap<String, Tensor>>,
        checkpoint_info: Option<CheckpointConfig>,
        checkpoint_extra_files: Vec<PathBuf>,
        model_task_runner: ModelTaskRunner,
    ) -> Self {
        Self {
            tx_checkpoint,
            tx_model,
            checkpoint_info,
            checkpoint_extra_files,
            model_task_runner,
            delete_queue: Arc::new(Mutex::new(BinaryHeap::new())),
        }
    }
}

#[derive(Error, Debug)]
pub enum CheckpointError {
    #[error("Extract thread crashed")]
    ExtractThreadCrashed,

    #[error("Trainer extract error: {0}")]
    Extract(#[from] TrainerThreadCommunicationError),

    #[error("Write thread crashed")]
    WriteThreadCrashed,

    #[error("Writing safetensors to disk failed: {0}")]
    WriteSafetensors(#[from] SaveSafetensorsError),

    #[error("Writing extra file to disk failed: {0}")]
    WriteExtraFile(#[from] tokio::io::Error),

    #[error("Couldn't upload model to huggingface: {0}")]
    UploadError(#[from] UploadModelError),

    #[error("Couldn't send checkpoint - channel closed")]
    SendCheckpoint,
}

impl CooldownStepMetadata {
    pub fn start<T: NodeIdentity>(
        &self,
        mut trainers: Vec<Trainer>,
        state: &Coordinator<T>,
    ) -> Result<CooldownStep, CooldownError> {
        let Some(mut trainer) = trainers.pop() else {
            return Err(CooldownError::NoTrainers);
        };

        let step = state.progress.step - 1;
        let run_id = String::from(&state.run_id);
        let checkpoint_extra_files = self.checkpoint_extra_files.clone();
        let checkpoint_info = self.checkpoint_info.clone();
        let tx_checkpoint = self.tx_checkpoint.clone();
        let tx_model = self.tx_model.clone();
        let model_task_runner = self.model_task_runner.clone();
        let doing_checkpoint = checkpoint_info.is_some();
        let delete_queue = self.delete_queue.clone();

        let checkpointing_and_evals = tokio::task::spawn(
            async move {
                info!("Extracting full model...");
                let (variables, trainer) =
                    tokio::task::spawn_blocking::<_, Result<_, CheckpointError>>(|| {
                        let variables = trainer.extract()?;
                        info!("Model extracted; {} parameters", variables.len());
                        Ok((variables, trainer))
                    })
                    .await
                    .map_err(|_| CheckpointError::ExtractThreadCrashed)??;

                let variables_clone: HashMap<String, Tensor> = variables
                    .iter()
                    .map(|(name, var)| (name.clone(), var.shallow_clone()))
                    .collect();

                trainers.push(trainer);
                let evals = model_task_runner.start(trainers);

                tx_model
                    .send(variables_clone)
                    .map_err(|_| CheckpointError::SendCheckpoint)?;

                let Some(CheckpointConfig {
                    hub_upload,
                    checkpoint_dir,
                    keep_steps,
                }) = checkpoint_info
                else {
                    // If there was no HF checkpointing configuration, return immediately
                    return Ok(evals);
                };

                // Start the upload process of the updated model parameters in a separate task
                tokio::task::spawn(async move {
                    let path = checkpoint_dir.join(format!("{run_id}-step{step}"));
                    info!("Saving to {}", path.display());
                    let mut local = tokio::task::spawn_blocking({
                        let path = path.clone();
                        move || save_tensors_into_safetensors(variables, path)
                    })
                    .await
                    .map_err(|_| CheckpointError::WriteThreadCrashed)??;
                    // push this step onto the delete queue only after we know we've at least created the dir. This avoids
                    // degenerate cases where the queue fills up with nonexistant dirs, causing our length test
                    // on the queue to return true even when the actual number of dirs is below the limit. There's a small risk this could
                    // cause us to miss dirs that get created but then the write fails after the dir is created. In that case, we likely
                    // have bigger issues to worry about anyway, and this is much faster than actually enumerating the checkpoint_dir
                    // parsing dir names, etc
                    // use a heap here to as a best-effort attempt to ensure we get rid of the lowest step number dir even if we spawn multiple tasks
                    // which may not finish writing their dirs in order. We note that even if we were to take the more complicated
                    // route of actually enumerating the checkpoint_dir there would still be a race condition, unless we took a lockfile
                    // or the like on the entire checkpoint_dir which probably isn't worth it just to support disk cleanup
                    if keep_steps.is_some() {
                        let mut delete_queue_guard = delete_queue.lock().await;
                        delete_queue_guard.push(Reverse(step));
                    }
                    for extra in checkpoint_extra_files {
                        let to = path.join(extra.file_name().unwrap());
                        tokio::fs::copy(extra.clone(), to.clone())
                            .await
                            .map_err(CheckpointError::WriteExtraFile)?;
                        local.push(to);
                    }

                    let Some(HubUploadInfo {
                        hub_repo,
                        hub_token,
                    }) = hub_upload
                    else {
                        return Ok::<(), CheckpointError>(());
                    };

                    info!(repo = hub_repo, "Uploading checkpoint to HuggingFace");
                    let revision = match upload_model_repo_async(
                        hub_repo.clone(),
                        local,
                        hub_token.clone(),
                        Some(format!("step {step}")),
                        None,
                    )
                    .await
                    {
                        Ok(revision) => {
                            info!(repo = hub_repo, "Upload to HuggingFace complete");
                            revision
                        }
                        Err(err) => {
                            error!(repo = hub_repo, "Error uploading to HuggingFace: {err:#}");
                            return Err(err.into());
                        }
                    };

                    tx_checkpoint
                        .send(HubRepo {
                            repo_id: FixedString::from_str_truncated(&hub_repo),
                            revision: Some(FixedString::from_str_truncated(&revision)),
                        })
                        .map_err(|_| CheckpointError::SendCheckpoint)?;

                    // we put the cleanup step at the end, so that if keep_steps == Some(0) the logic will still work
                    // we'll just delete the dir after we've uploaded it
                    if let Some(keep_steps) = keep_steps {
                        // in the happy case this could be an if but if previous iterations failed somewhere
                        // then we may have more than 1 dir to clean up
                        let mut delete_queue_guard = delete_queue.lock().await;
                        while delete_queue_guard.len() > keep_steps as usize {
                            let delete_step = delete_queue_guard.pop().unwrap().0;
                            let delete_path =
                                checkpoint_dir.join(format!("{run_id}-step{delete_step}"));
                            if let Err(err) = tokio::fs::remove_dir_all(delete_path.clone()).await {
                                warn!("Error removing {} : {}", delete_path.display(), err);
                            }
                        }
                    }

                    Ok(())
                });

                Ok(evals)
            }
            .instrument(info_span!("checkpointing")),
        );
        Ok(CooldownStep {
            checkpointing_and_evals,
            doing_checkpoint,
        })
    }
}

#[derive(Debug)]
pub struct CooldownStep {
    checkpointing_and_evals: JoinHandle<Result<RunningEvals, CheckpointError>>,
    doing_checkpoint: bool,
}

impl CooldownStep {
    pub async fn finish(self) -> Result<RunningEvals, CooldownError> {
        let running_evals = self
            .checkpointing_and_evals
            .await
            .map_err(|_| CooldownError::CheckpointThreadCrashed)??;

        Ok(running_evals)
    }

    pub fn doing_checkpoint(&self) -> bool {
        self.doing_checkpoint
    }
}
