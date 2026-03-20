use crate::CheckpointUploader;
use psyche_coordinator::{
    CheckpointerSelection, Coordinator, model::Model, model_extra_data::CheckpointData,
};
use psyche_data_provider::{GcsManifestMetadata, UploadError, upload_to_gcs_signed, upload_to_hub};
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
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};
use tch::Tensor;
use thiserror::Error;
use tokio::{
    sync::{Mutex, mpsc},
    task::JoinHandle,
};
use tracing::error;
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

    #[error("error in cooldown step: {0}")]
    CoordinatorError(#[from] psyche_coordinator::CoordinatorError),
}

pub struct CooldownStepMetadata {
    tx_model: mpsc::UnboundedSender<HashMap<String, Tensor>>,
    checkpoint_info: CheckpointConfig,
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
        tx_model: mpsc::UnboundedSender<HashMap<String, Tensor>>,
        checkpoint_info: CheckpointConfig,
        checkpoint_extra_files: Vec<PathBuf>,
        model_task_runner: ModelTaskRunner,
    ) -> Self {
        Self {
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
    pub fn start(
        &self,
        mut trainers: Vec<Trainer>,
        client_index: u64,
        state: &Coordinator,
    ) -> Result<CooldownStep, CooldownError> {
        let Some(mut trainer) = trainers.pop() else {
            return Err(CooldownError::NoTrainers);
        };

        let step = state.progress.step - 1;
        let run_id = String::from(&state.run_id);
        let epoch = state.progress.epoch as u32;
        let checkpoint_extra_files = self.checkpoint_extra_files.clone();
        let checkpoint_info = self.checkpoint_info.clone();
        let Model::LLM(ref llm) = state.model;
        let checkpoint_data = llm.decode_checkpoint();
        let tx_model = self.tx_model.clone();
        let model_task_runner = self.model_task_runner.clone();
        let delete_queue = self.delete_queue.clone();
        let checkpointer_selection = CheckpointerSelection::from_coordinator(state, 0)?;
        let is_checkpointer = checkpointer_selection
            .is_checkpointer(client_index, state.epoch_state.clients.len() as u64);
        let cancellation_token = tokio_util::sync::CancellationToken::new();
        let checkpoint_completed = Arc::new(AtomicBool::new(false));

        let checkpointing_and_evals: JoinHandle<Result<RunningEvals, CheckpointError>> =
            tokio::task::spawn({
                let cancellation_token = cancellation_token.clone();
                let checkpoint_completed = checkpoint_completed.clone();
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
                    if !is_checkpointer {
                        info!("Skipping checkpoint upload as this node is not a checkpointer for this epoch");
                        return Ok(evals);
                    }

                    let CheckpointConfig {
                        checkpoint_dir,
                        delete_old_steps,
                        keep_steps,
                        hub_token,
                        skip_upload,
                        run_down_client,
                    } = checkpoint_info;

                    // When skip_upload is true (testing), skip all checkpoint saving
                    if skip_upload {
                        info!("Skipping checkpoint save and upload (skip_upload flag is set)");
                        checkpoint_completed.store(true, Ordering::Release);
                        return Ok(evals);
                    }

                    let uploader = match checkpoint_data {
                        Some(CheckpointData::Hub { ref repo_id, .. }) => {
                            if let Some(token) = hub_token {
                                match CheckpointUploader::new_hub(repo_id.clone(), token).await {
                                    Ok(uploader) => Some(uploader),
                                    Err(err) => {
                                        error!("Failed to create HF uploader: {}", err);
                                        None
                                    }
                                }
                            } else {
                                warn!("HF_TOKEN env not provided, skipping upload to HuggingFace Hub");
                                None
                            }
                        }
                        Some(CheckpointData::Gcs { .. }) => {
                            if let Some(client) = run_down_client {
                                Some(CheckpointUploader::new_gcs(client))
                            } else {
                                warn!("RunDownClient not configured, skipping GCS checkpoint upload");
                                None
                            }
                        }
                        _ => None,
                    };

                    let path = checkpoint_dir.join(format!("{run_id}-step{step}"));
                    let local =
                        save_checkpoint_locally(path, variables, checkpoint_extra_files).await?;

                    let upload_succeeded = if let Some(uploader) = uploader {
                        let manifest_metadata = GcsManifestMetadata {
                            epoch,
                            run_id: run_id.clone(),
                        };
                        match upload_checkpoint(uploader, manifest_metadata, local.clone(), step as u64, cancellation_token.clone()).await {
                            Ok(()) => true,
                            Err(err) => {
                                error!("Error uploading checkpoint: {}", err);
                                false
                            }
                        }
                    } else {
                        true
                    };
                    checkpoint_completed.store(upload_succeeded, Ordering::Release);

                    cleanup_dirs(
                        delete_queue,
                        keep_steps,
                        run_id,
                        delete_old_steps,
                        step,
                        checkpoint_dir,
                    )
                    .await;

                    Ok(evals)
                }
                .instrument(info_span!("checkpointing"))
            });

        Ok(CooldownStep {
            checkpointing_and_evals,
            cancellation_token,
            checkpoint_completed,
            sent_witness: false,
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
    uploader: CheckpointUploader,
    manifest_metadata: GcsManifestMetadata,
    local: Vec<PathBuf>,
    step: u64,
    cancellation_token: tokio_util::sync::CancellationToken,
) -> Result<(), CheckpointError> {
    event!(cooldown::CheckpointUploadStarted);
    let result = match uploader {
        CheckpointUploader::Gcs(run_down) => upload_to_gcs_signed(
            &run_down,
            manifest_metadata,
            local,
            step,
            cancellation_token,
        )
        .await
        .map_err(CheckpointError::UploadError),
        CheckpointUploader::Hub(hub_info) => {
            upload_to_hub(hub_info, local, step, cancellation_token)
                .await
                .map_err(CheckpointError::UploadError)
        }
        CheckpointUploader::Dummy => {
            info!("Dummy upload info provided; skipping upload");
            Ok(())
        }
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

#[derive(Debug)]
pub struct CooldownStep {
    checkpointing_and_evals: JoinHandle<Result<RunningEvals, CheckpointError>>,
    cancellation_token: tokio_util::sync::CancellationToken,
    checkpoint_completed: Arc<AtomicBool>,
    pub sent_witness: bool,
}

impl CooldownStep {
    pub async fn finish(self) -> Result<RunningEvals, CooldownError> {
        self.cancellation_token.cancel();
        let running_evals = self
            .checkpointing_and_evals
            .await
            .map_err(|_| CooldownError::CheckpointThreadCrashed)??;

        Ok(running_evals)
    }

    pub fn is_finished(&self) -> bool {
        self.checkpointing_and_evals.is_finished()
    }

    pub fn checkpoint_complete(&self) -> bool {
        self.checkpoint_completed.load(Ordering::Acquire)
    }
}
