use crate::UploadInfo;
use psyche_coordinator::{
    Coordinator,
    model::{self},
};
use psyche_core::NodeIdentity;
use psyche_data_provider::{GcsManifestMetadata, UploadError, upload_to_gcs, upload_to_hub};
use psyche_event_sourcing::event;
#[cfg(feature = "python")]
use psyche_modeling::CausalLM;
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
use tracing::{Instrument, info, info_span, warn};

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
    tx_checkpoint: mpsc::UnboundedSender<model::Checkpoint>,
    tx_model: mpsc::UnboundedSender<HashMap<String, Tensor>>,
    checkpoint_info: Option<CheckpointConfig>,
    checkpoint_extra_files: Vec<PathBuf>,

    model_task_runner: ModelTaskRunner,
    // use a heap here as a best-effort attempt to ensure we get rid of the lowest step number dir even if we spawn multiple tasks
    // which may not finish writing their dirs in order. We note that even if we were to take the more complicated
    // route of actually enumerating the checkpoint_dir there would still be a race condition, unless we took a lockfile
    // or the like on the entire checkpoint_dir which probably isn't worth it just to support disk cleanup
    // we don't really expect there to be contention on this lock or real race conditions in practice though
    // as by the time one task spawns after a training round the previous write/upload task(s) should (hopefully) be long done
    delete_queue: Arc<Mutex<BinaryHeap<Reverse<u32>>>>,
}

impl CooldownStepMetadata {
    pub fn new(
        tx_checkpoint: mpsc::UnboundedSender<model::Checkpoint>,
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

    #[error("Couldn't upload model to huggingface or GCS: {0}")]
    UploadError(#[from] UploadError),

    #[error("Couldn't send checkpoint - channel closed")]
    SendCheckpoint,
}

async fn cleanup_dirs(
    delete_queue: Arc<Mutex<BinaryHeap<Reverse<u32>>>>,
    keep_steps: u32,
    run_id: String,
    delete_old_steps: bool,
    step: u32,
    checkpoint_dir: PathBuf,
) {
    if delete_old_steps {
        let mut delete_queue_guard = delete_queue.lock().await;
        delete_queue_guard.push(Reverse(step));
        // in the happy case this could be an if but if previous iterations failed somewhere
        // then we may have more than 1 dir to clean up
        while delete_queue_guard.len() > keep_steps as usize {
            let delete_step = delete_queue_guard.pop().unwrap().0;
            let delete_path = checkpoint_dir.join(format!("{run_id}-step{delete_step}"));
            if let Err(err) = tokio::fs::remove_dir_all(delete_path.clone()).await {
                warn!("Error removing {} : {}", delete_path.display(), err);
            } else {
                info!("Successfully removed {}", delete_path.display());
            }
        }
    }
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
        let epoch = state.progress.epoch as u32;
        let checkpoint_extra_files = self.checkpoint_extra_files.clone();
        let checkpoint_info = self.checkpoint_info.clone();
        let tx_checkpoint = self.tx_checkpoint.clone();
        let tx_model = self.tx_model.clone();
        let model_task_runner = self.model_task_runner.clone();
        let delete_queue = self.delete_queue.clone();

        let checkpointing_and_evals: CheckpointAndEvalsHandle = tokio::task::spawn(
            async move {
                info!("Extracting full model...");
                event!(cooldown::ModelSerializationStarted);
                let (variables, trainer) =
                    tokio::task::spawn_blocking::<_, Result<_, CheckpointError>>(|| {
                        let variables = trainer.extract()?;
                        info!("Model extracted; {} parameters", variables.len());
                        Ok((variables, trainer))
                    })
                    .await
                    .map_err(|_| {
                        event!(cooldown::ModelSerializationFinished {
                            success: false,
                            error_string: Some("extract thread crashed".to_string())
                        });
                        CheckpointError::ExtractThreadCrashed
                    })??;
                event!(cooldown::ModelSerializationFinished {
                    success: true,
                    error_string: None
                });

                let variables_clone: HashMap<String, Tensor> = variables
                    .iter()
                    .map(|(name, var)| (name.clone(), var.shallow_clone()))
                    .collect();

                // for p2p model sharing we use the native trainer shape
                tx_model
                    .send(variables_clone)
                    .map_err(|_| CheckpointError::SendCheckpoint)?;

                // convert from internal shape to serialized shape (e.g. torchtitan to hf)
                let (variables, trainer) = match trainer {
                    #[cfg(feature = "python")]
                    Trainer::PythonDistributed(_) => {
                        info!("Converting distributed trainer variables for checkpointing...");
                        tokio::task::spawn_blocking(|| (trainer.convert(Some(variables)), trainer))
                            .await
                            .map_err(|_| CheckpointError::ExtractThreadCrashed)?
                    }
                    _ => (variables, trainer),
                };

                trainers.push(trainer);
                let evals = model_task_runner.start(trainers);

                let Some(CheckpointConfig {
                    upload_info,
                    checkpoint_dir,
                    delete_old_steps,
                    keep_steps,
                }) = checkpoint_info
                else {
                    return Ok((evals, None));
                };

                let upload_handle = tokio::task::spawn(async move {
                    let path = checkpoint_dir.join(format!("{run_id}-step{step}"));
                    let local =
                        save_checkpoint_locally(path, variables, checkpoint_extra_files).await?;

                    if let Some(upload_info) = upload_info {
                        let manifest_metadata = GcsManifestMetadata {
                            epoch,
                            run_id: run_id.clone(),
                        };
                        upload_checkpoint(
                            upload_info,
                            manifest_metadata,
                            local.clone(),
                            step as u64,
                            tx_checkpoint,
                        )
                        .await?;
                    }

                    cleanup_dirs(
                        delete_queue,
                        keep_steps,
                        run_id,
                        delete_old_steps,
                        step,
                        checkpoint_dir,
                    )
                    .await;

                    Ok(())
                });

                Ok((evals, Some(upload_handle)))
            }
            .instrument(info_span!("checkpointing")),
        );

        Ok(CooldownStep {
            checkpointing_and_evals,
        })
    }
}

async fn save_checkpoint_locally(
    path: PathBuf,
    variables: HashMap<String, Tensor>,
    checkpoint_extra_files: Vec<PathBuf>,
) -> Result<Vec<PathBuf>, CheckpointError> {
    info!("Saving to {}", path.display());
    event!(cooldown::CheckpointWriteStarted);
    let mut local = tokio::task::spawn_blocking({
        let path = path.clone();
        move || save_tensors_into_safetensors(variables, path)
    })
    .await
    .map_err(|_| {
        event!(cooldown::CheckpointWriteFinished {
            success: false,
            error_string: Some("write thread crashed".to_string())
        });
        CheckpointError::WriteThreadCrashed
    })??;

    for extra in checkpoint_extra_files {
        let to = path.join(extra.file_name().unwrap());
        tokio::fs::copy(extra.clone(), to.clone())
            .await
            .map_err(|e| {
                event!(cooldown::CheckpointWriteFinished {
                    success: false,
                    error_string: Some(e.to_string())
                });
                CheckpointError::WriteExtraFile(e)
            })?;
        local.push(to);
    }

    event!(cooldown::CheckpointWriteFinished {
        success: true,
        error_string: None
    });
    Ok(local)
}

async fn upload_checkpoint(
    upload_info: UploadInfo,
    manifest_metadata: GcsManifestMetadata,
    local: Vec<PathBuf>,
    step: u64,
    tx_checkpoint: mpsc::UnboundedSender<model::Checkpoint>,
) -> Result<(), CheckpointError> {
    event!(cooldown::CheckpointUploadStarted);
    let result = match upload_info {
        UploadInfo::Gcs(gcs_info) => {
            upload_to_gcs(gcs_info, manifest_metadata, local, step, tx_checkpoint)
                .await
                .map_err(CheckpointError::UploadError)
        }
        UploadInfo::Hub(hub_info) => upload_to_hub(hub_info, local, step, tx_checkpoint)
            .await
            .map_err(CheckpointError::UploadError),
    };
    match &result {
        Ok(()) => event!(cooldown::CheckpointUploadFinished {
            success: true,
            error_string: None
        }),
        Err(e) => event!(cooldown::CheckpointUploadFinished {
            success: false,
            error_string: Some(e.to_string())
        }),
    }
    result
}

type CheckpointAndEvalsHandle = JoinHandle<
    Result<
        (
            RunningEvals,
            Option<JoinHandle<Result<(), CheckpointError>>>,
        ),
        CheckpointError,
    >,
>;

#[derive(Debug)]
pub struct CooldownStep {
    checkpointing_and_evals: CheckpointAndEvalsHandle,
}

impl CooldownStep {
    pub async fn finish(
        self,
    ) -> Result<
        (
            RunningEvals,
            Option<JoinHandle<Result<(), CheckpointError>>>,
        ),
        CooldownError,
    > {
        let (running_evals, upload_handle) = self
            .checkpointing_and_evals
            .await
            .map_err(|_| CooldownError::CheckpointThreadCrashed)??;

        Ok((running_evals, upload_handle))
    }
}
