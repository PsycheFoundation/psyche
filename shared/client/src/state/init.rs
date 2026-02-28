use crate::{WandBInfo, fetch_data::DataFetcher};
use psyche_coordinator::{
    Coordinator, HealthChecks,
    model::{self, HttpLLMTrainingDataLocation, LLMTrainingDataLocation},
};
use psyche_core::{
    Barrier, CancellableBarrier, IntegrationTestLogMarker, NodeIdentity, Shuffle, TokenSize,
};
use psyche_data_provider::{
    DataProvider, DataProviderTcpClient, DownloadError, DummyDataProvider,
    PreprocessedDataProvider, Split, WeightedDataProvider, download_dataset_repo_async,
    download_model_from_gcs_async, download_model_repo_async,
    http::{FileURLs, HttpDataProvider},
};
use psyche_metrics::ClientMetrics;
use psyche_modeling::{
    AttentionImplementation, AutoConfig, AutoTokenizerError, CausalLM, CommunicatorId,
    DataParallel, DeepseekForCausalLM, Devices, DummyModel, LlamaConfig, LlamaForCausalLM,
    LocalTrainer, ModelLoadError, ParallelModels, PretrainedSource, Trainer, auto_tokenizer,
};
use psyche_network::{AuthenticatableIdentity, BlobTicket};
use psyche_watcher::OpportunisticData;
use std::{collections::HashMap, path::PathBuf, sync::Arc};
use tch::{Kind, Tensor};
use thiserror::Error;
use tokenizers::{ModelWrapper, Tokenizer, models::wordlevel::WordLevel};
use tokio::{
    io,
    sync::{mpsc::UnboundedSender, oneshot},
    task::{JoinError, JoinHandle},
};
use tracing::{debug, error, info};

use super::{
    CheckpointConfig, FinishedBroadcast, cooldown::CooldownStepMetadata, evals::ModelTaskRunner,
    stats::StatsLogger, steps::StepStateMachine, train::TrainingStepMetadata,
    types::DistroBroadcastAndPayload, warmup::WarmupStepMetadata, witness::WitnessStepMetadata,
};
use iroh_blobs::api::Tag;

pub struct RunInitConfig<T: NodeIdentity, A: AuthenticatableIdentity> {
    // identity for connecting to the data server
    pub identity: T,
    pub network_identity: A,
    pub private_key: A::PrivateKey,

    // p2p model parameters sharing config
    pub max_concurrent_parameter_requests: usize,

    // model & dataload
    pub device: Devices,
    pub hub_read_token: Option<String>,
    pub hub_max_concurrent_downloads: usize,
    pub data_parallelism: usize,
    pub tensor_parallelism: usize,
    pub micro_batch_size: usize,
    pub optim_stats_every_n_steps: Option<u32>,
    pub grad_accum_in_fp32: bool,

    // evaluation
    pub eval_task_max_docs: Option<usize>,
    pub eval_tasks: Vec<psyche_eval::Task>,
    pub prompt_task: bool,

    // logging
    pub wandb_info: Option<WandBInfo>,

    // debugging
    pub write_gradients_dir: Option<PathBuf>,

    // checkpointing
    pub checkpoint_config: Option<CheckpointConfig>,

    // configurable dummy training time (in seconds) for this client - relevant just for testing
    pub dummy_training_delay_secs: Option<u64>,

    pub sidecar_port: Option<u16>,
}

#[derive(Debug, Error)]
pub enum InitRunError {
    #[error("No model provided in Coordinator state, nothing to do.")]
    NoModel,

    #[error("Model is Ephemeral, it's impossible to join this run.")]
    ModelIsEphemeral,

    #[error("failed to read local model info: {0}")]
    LocalModelLoad(#[from] io::Error),

    #[error("failed to read HF model info: {0}")]
    HfModelLoad(#[from] hf_hub::api::tokio::ApiError),

    #[error("failed to download model from GCS: {0}")]
    GcsModelLoad(#[from] DownloadError),

    #[error("model loading thread crashed")]
    ModelLoadingThreadCrashed(JoinError),

    #[error("failed to load model: {0}")]
    ModelLoad(#[from] ModelLoadError),

    #[error("Couldn't load tokenizer: {0}")]
    TokenizerLoad(#[from] AutoTokenizerError),

    // TODO refactor data provider for real errors
    #[error("Couldn't initialize data provider: {0}")]
    DataProviderConnect(anyhow::Error),

    #[error("wandb setup thread crashed")]
    WandbThreadCrashed(JoinError),

    #[error("wandb failed to create run: {0}")]
    WandbLoad(#[from] wandb::ApiError),

    #[error("could not parse config: {0}")]
    FailedToParseConfig(#[from] serde_json::Error),

    #[error("Unsupported architecture: {0}")]
    UnsupportedArchitecture(String),

    #[cfg(feature = "python")]
    #[error("Python distributed error: {0}")]
    PythonDistributedError(#[from] psyche_modeling::PythonDistributedCausalLMError),

    #[cfg(feature = "python")]
    #[error("Python model error: {0}")]
    PythonModelError(#[from] psyche_modeling::PythonCausalLMError),

    #[cfg(feature = "python")]
    #[error("Python distributed trainer error: {0}")]
    PythonDistributedTrainerError(#[from] psyche_modeling::PythonDistributedTrainerError),
}

enum RawLoadedModelType {
    ParallelNativeModels(Vec<Box<dyn CausalLM>>),
    #[cfg(feature = "python")]
    Python(psyche_modeling::PythonCausalLM),
    #[cfg(feature = "python")]
    PythonDistributed(psyche_modeling::PythonDistributedCausalLM),
}

struct RawLoadedModel {
    models: RawLoadedModelType,
    tokenizer: Arc<Tokenizer>,
    model_task_runner: ModelTaskRunner,
    checkpoint_extra_files: Vec<PathBuf>,
}

type OneshotModelParameterSender = oneshot::Sender<HashMap<String, Tensor>>;
type OneShotModelConfigSender = oneshot::Sender<(String, Tokenizer, Vec<String>)>;

pub struct RunInitConfigAndIO<T: NodeIdentity, A: AuthenticatableIdentity> {
    pub init_config: RunInitConfig<T, A>,

    pub tx_health_check: UnboundedSender<HealthChecks<T>>,
    pub tx_witness: UnboundedSender<OpportunisticData>,
    pub tx_checkpoint: UnboundedSender<model::Checkpoint>,
    pub tx_model: UnboundedSender<HashMap<String, Tensor>>,
    pub tx_parameters_req: UnboundedSender<(Vec<String>, OneshotModelParameterSender)>,
    pub tx_config: UnboundedSender<(String, String)>,
    pub tx_distro_result: UnboundedSender<DistroBroadcastAndPayload>,
    pub tx_request_download: UnboundedSender<(BlobTicket, Tag)>,
    pub tx_request_model_config: UnboundedSender<OneShotModelConfigSender>,
    pub tx_broadcast_finished: UnboundedSender<FinishedBroadcast>,

    pub metrics: Arc<ClientMetrics>,
}

impl<T: NodeIdentity, A: AuthenticatableIdentity + 'static> RunInitConfigAndIO<T, A> {
    /// Call this on first warmup - when we need to enter the run, we have to load the model, connect to the data server, etc
    pub async fn init_run(
        self,
        state: Coordinator<T>,
    ) -> Result<StepStateMachine<T, A>, InitRunError> {
        let Self {
            init_config,
            tx_witness,
            tx_health_check,
            tx_checkpoint,
            tx_model,
            tx_config,
            tx_parameters_req,
            tx_distro_result,
            tx_request_download,
            tx_request_model_config,
            tx_broadcast_finished,
            metrics,
        } = self;

        tch::manual_seed(1337);

        // Check device availability early
        if !init_config.device.is_probably_available() {
            return Err(InitRunError::ModelLoad(
                psyche_modeling::ModelLoadError::UnavailbleDevice(init_config.device),
            ));
        }

        let model::Model::LLM(llm) = state.model;

        let hub_read_token = init_config.hub_read_token.clone();
        let hub_max_concurrent_downloads = init_config.hub_max_concurrent_downloads;
        let data_future = async {
            debug!("Setting up data provider from {:?}", llm.data_location);
            let data_provider = match llm.data_location {
                LLMTrainingDataLocation::Server(data_server) => DataProvider::Server(
                    DataProviderTcpClient::connect(
                        (&data_server).into(),
                        init_config.network_identity,
                        init_config.private_key,
                    )
                    .await?,
                ),
                LLMTrainingDataLocation::Local(_) => todo!(),
                LLMTrainingDataLocation::Dummy => {
                    DataProvider::Dummy(DummyDataProvider::new(TokenSize::TwoBytes, 2048, u64::MAX))
                }
                LLMTrainingDataLocation::Http(HttpLLMTrainingDataLocation {
                    location,
                    token_size_in_bytes,
                    shuffle,
                }) => {
                    let file_urls = FileURLs::from_location(&location).await?;
                    DataProvider::Http(HttpDataProvider::new(
                        file_urls,
                        token_size_in_bytes,
                        llm.max_seq_len,
                        shuffle,
                    )?)
                }
                LLMTrainingDataLocation::WeightedHttp(config_url) => DataProvider::WeightedHttp(
                    WeightedDataProvider::<HttpDataProvider>::from_config_url(
                        &String::from(&config_url),
                        llm.max_seq_len,
                    )
                    .await?,
                ),
                LLMTrainingDataLocation::Preprocessed(url) => {
                    let url: String = (&url).into();
                    let dir = if std::fs::exists(&url).unwrap_or_default() {
                        PathBuf::from(url)
                    } else {
                        download_dataset_repo_async(
                            url.clone(),
                            None,
                            None,
                            hub_read_token,
                            Some(hub_max_concurrent_downloads),
                            false,
                        )
                        .await?
                        .first()
                        .ok_or(anyhow::anyhow!("No files downloaded for {url}"))?
                        .parent()
                        .unwrap()
                        .into()
                    };
                    DataProvider::Preprocessed(PreprocessedDataProvider::new_from_directory(
                        dir,
                        llm.max_seq_len as usize,
                        Shuffle::DontShuffle,
                        Some(Split::Train),
                        None,
                    )?)
                }
            };
            Ok(data_provider)
        };

        let model_future: JoinHandle<Result<RawLoadedModel, InitRunError>> = match &llm.architecture
        {
            model::LLMArchitecture::HfLlama
            | model::LLMArchitecture::HfDeepseek
            | model::LLMArchitecture::HfAuto
            | model::LLMArchitecture::Torchtitan => match &llm.checkpoint {
                model::Checkpoint::Dummy(_) => tokio::spawn(async move {
                    let tokenizer = Arc::new(Tokenizer::new(ModelWrapper::WordLevel(
                        WordLevel::builder().build().unwrap(),
                    )));

                    let model = RawLoadedModel {
                        models: RawLoadedModelType::ParallelNativeModels(
                            (0..(init_config.data_parallelism * init_config.tensor_parallelism))
                                .map(|_| {
                                    if let Some(training_delay) =
                                        init_config.dummy_training_delay_secs
                                    {
                                        Box::new(DummyModel::new(training_delay))
                                            as Box<dyn CausalLM>
                                    } else {
                                        Box::new(DummyModel::default()) as Box<dyn CausalLM>
                                    }
                                })
                                .collect(),
                        ),
                        tokenizer: tokenizer.clone(),
                        checkpoint_extra_files: vec![],
                        model_task_runner: ModelTaskRunner::new(
                            vec![],
                            false,
                            tokenizer.clone(),
                            None,
                            0,
                        ),
                    };
                    #[allow(clippy::arc_with_non_send_sync)]
                    let config = &PretrainedSource::ConfigAndTensors(
                        AutoConfig::Llama(LlamaConfig::dummy()),
                        Arc::new(psyche_modeling::get_dummy_parameters()),
                    )
                    .serialize_config()?;
                    let tokenizer = tokenizer.to_string(false).unwrap();
                    info!("Config Uploaded: {}", config);
                    tx_config.send((config.to_string(), tokenizer)).unwrap();
                    Ok(model)
                }),
                model::Checkpoint::Hub(_)
                | model::Checkpoint::P2P(_)
                | model::Checkpoint::P2PGcs(_)
                | model::Checkpoint::Gcs(_) => {
                    let checkpoint = llm.checkpoint;
                    tokio::spawn(async move {
                        let (source, tokenizer, checkpoint_extra_files) = match checkpoint {
                            model::Checkpoint::Hub(hub_repo) => {
                                let repo_id: String = (&hub_repo.repo_id).into();
                                let potential_local_path = PathBuf::from(repo_id.clone());
                                let revision = hub_repo.revision.map(|bytes| (&bytes).into());

                                let model_is_local = if revision.is_none()
                                    && tokio::fs::try_exists(potential_local_path.clone())
                                        .await
                                        .unwrap_or_default()
                                {
                                    let mut ret = Vec::new();
                                    let mut read_dir =
                                        tokio::fs::read_dir(potential_local_path).await?;
                                    while let Some(dir_entry) = read_dir.next_entry().await? {
                                        ret.push(dir_entry.path())
                                    }
                                    ret
                                } else {
                                    info!(
                                        "Downloading {}, revision: {:?} (if needed)",
                                        hub_repo.repo_id, revision
                                    );
                                    download_model_repo_async(
                                        &repo_id,
                                        revision,
                                        None,
                                        init_config.hub_read_token,
                                        Some(init_config.hub_max_concurrent_downloads),
                                        false,
                                    )
                                    .await?
                                };
                                let repo_files = model_is_local;
                                let checkpoint_extra_files = repo_files
                                    .iter()
                                    .filter(|file| {
                                        file.ends_with("config.json")
                                            || file.ends_with("tokenizer.json")
                                            || file.ends_with("tokenizer_config.json")
                                            || file.ends_with("special_tokens_map.json")
                                            || file.ends_with("generation_config.json")
                                            || file.ends_with(".py")
                                    })
                                    .cloned()
                                    .collect();
                                let tokenizer = Arc::new(auto_tokenizer(&repo_files)?);
                                (
                                    PretrainedSource::<AutoConfig>::RepoFiles(repo_files),
                                    tokenizer,
                                    checkpoint_extra_files,
                                )
                            }
                            model::Checkpoint::P2P(_) | model::Checkpoint::P2PGcs(_) => {
                                let (tx_model_config_response, rx_model_config_response) =
                                    oneshot::channel();
                                info!("Checkpoint is p2p, requesting model config over network");

                                tx_request_model_config
                                    .send(tx_model_config_response)
                                    .unwrap();

                                let (model_config, tokenizer, parameter_names) =
                                    rx_model_config_response.await.unwrap();
                                debug!("Got p2p info, model_config: {}", model_config);

                                let model_config = match llm.architecture {
                                    model::LLMArchitecture::HfLlama => {
                                        AutoConfig::Llama(serde_json::from_str(&model_config)?)
                                    }
                                    model::LLMArchitecture::HfDeepseek => {
                                        AutoConfig::Deepseek(serde_json::from_str(&model_config)?)
                                    }
                                    model::LLMArchitecture::HfAuto
                                    | model::LLMArchitecture::Torchtitan => {
                                        #[cfg(feature = "python")]
                                        {
                                            AutoConfig::Auto(serde_json::from_str::<
                                                psyche_modeling::PythonModelConfig,
                                            >(
                                                &model_config
                                            )?)
                                        }

                                        #[cfg(not(feature = "python"))]
                                        {
                                            return Err(InitRunError::UnsupportedArchitecture(
                                                llm.architecture.to_string(),
                                            ));
                                        }
                                    }
                                };
                                info!(
                                    "Requesting {} parameters over p2p network",
                                    parameter_names.len()
                                );

                                let (tx_params_response, rx_params_response) = oneshot::channel();
                                tx_parameters_req
                                    .send((parameter_names, tx_params_response))
                                    .unwrap();
                                #[allow(clippy::arc_with_non_send_sync)]
                                let parameters = Arc::new(rx_params_response.await.unwrap());

                                (
                                    PretrainedSource::<AutoConfig>::ConfigAndTensors(
                                        model_config,
                                        parameters,
                                    ),
                                    Arc::new(tokenizer),
                                    vec![],
                                )
                            }
                            model::Checkpoint::Gcs(gcs_repo) => {
                                let bucket: String = (&gcs_repo.bucket).into();
                                let prefix: Option<String> = gcs_repo.prefix.map(|p| (&p).into());

                                info!(
                                    "Downloading model from gs://{}/{}",
                                    bucket,
                                    prefix.as_deref().unwrap_or("")
                                );

                                let repo_files =
                                    download_model_from_gcs_async(&bucket, prefix.as_deref())
                                        .await?;

                                let checkpoint_extra_files = repo_files
                                    .iter()
                                    .filter(|file| {
                                        file.ends_with("config.json")
                                            || file.ends_with("tokenizer.json")
                                            || file.ends_with("tokenizer_config.json")
                                            || file.ends_with("special_tokens_map.json")
                                            || file.ends_with("generation_config.json")
                                            || file.ends_with(".py")
                                    })
                                    .cloned()
                                    .collect();
                                let tokenizer = Arc::new(auto_tokenizer(&repo_files)?);
                                (
                                    PretrainedSource::<AutoConfig>::RepoFiles(repo_files),
                                    tokenizer,
                                    checkpoint_extra_files,
                                )
                            }
                            _ => unreachable!(),
                        };

                        info!("Loading model...");

                        let model_task_runner = ModelTaskRunner::new(
                            init_config.eval_tasks,
                            init_config.prompt_task,
                            tokenizer.clone(),
                            init_config.eval_task_max_docs,
                            // if doing python fsdp we only have one effective dp rank for inference
                            if init_config.data_parallelism > 1
                                && llm.architecture == model::LLMArchitecture::HfAuto
                            {
                                1
                            } else {
                                init_config.data_parallelism
                            },
                        );

                        let serialized_config = source.serialize_config()?;
                        let attn_implementation: Option<AttentionImplementation> =
                            match llm.data_type {
                                model::LLMTrainingDataType::Finetuning => {
                                    #[cfg(feature = "parallelism")]
                                    {
                                        // use varlen backend if available
                                        Some(AttentionImplementation::FlashAttention2)
                                    }

                                    #[cfg(not(feature = "parallelism"))]
                                    None
                                }
                                model::LLMTrainingDataType::Pretraining => None,
                            };

                        let raw_loaded_model_type: RawLoadedModelType = match llm.architecture {
                            model::LLMArchitecture::HfAuto | model::LLMArchitecture::Torchtitan => {
                                #[cfg(feature = "python")]
                                {
                                    let dp = init_config.data_parallelism;
                                    let tp = init_config.tensor_parallelism;
                                    let num_local_ranks = init_config.device.size() as i64;

                                    tokio::task::spawn_blocking(move || {
                                        if tp != 1 || dp != 1 {
                                            psyche_modeling::PythonDistributedCausalLM::new(
                                                llm.architecture.to_string(),
                                                source.try_into()?,
                                                tch::Device::cuda_if_available(),
                                                attn_implementation.unwrap_or_default(),
                                                psyche_modeling::ParallelismConfig { dp, tp },
                                                Some(llm.max_seq_len as usize),
                                                init_config.sidecar_port,
                                                Some(num_local_ranks),
                                            )
                                            .map(RawLoadedModelType::PythonDistributed)
                                            .map_err(InitRunError::PythonDistributedError)
                                        } else {
                                            let device = init_config
                                                .device
                                                .device_for_rank(0)
                                                .ok_or_else(|| {
                                                    ModelLoadError::NoDeviceForRank(
                                                        0,
                                                        init_config.device,
                                                    )
                                                })?;
                                            psyche_modeling::PythonCausalLM::new(
                                                &llm.architecture.to_string(),
                                                &source.try_into()?,
                                                device,
                                                attn_implementation.unwrap_or_default(),
                                                None,
                                                Some(llm.max_seq_len as usize),
                                            )
                                            .map(RawLoadedModelType::Python)
                                            .map_err(InitRunError::PythonModelError)
                                        }
                                    })
                                    .await
                                    .map_err(InitRunError::ModelLoadingThreadCrashed)??
                                }

                                #[cfg(not(feature = "python"))]
                                {
                                    return Err(InitRunError::UnsupportedArchitecture(
                                        llm.architecture.to_string(),
                                    ));
                                }
                            }
                            architecture => {
                                let mut futures: Vec<
                                    JoinHandle<Result<Box<dyn CausalLM>, ModelLoadError>>,
                                > = Vec::with_capacity(
                                    init_config.data_parallelism * init_config.tensor_parallelism,
                                );
                                let devices = init_config.device.clone();

                                for dp in 0..init_config.data_parallelism {
                                    let communicator_id: Option<CommunicatorId> =
                                        match init_config.tensor_parallelism {
                                            0 | 1 => None,
                                            #[cfg(feature = "parallelism")]
                                            _ => Some(tch::CStore::new().into()),
                                            #[cfg(not(feature = "parallelism"))]
                                            _ => unimplemented!(),
                                        };
                                    for tp in 0..init_config.tensor_parallelism {
                                        let tensor_parallelism_world =
                                            communicator_id.as_ref().map(|communicator_id| {
                                                (
                                                    communicator_id.clone(),
                                                    tp,
                                                    init_config.tensor_parallelism,
                                                )
                                            });
                                        let source = source.clone();
                                        let rank = dp * init_config.tensor_parallelism + tp;
                                        let devices = devices.clone();
                                        let device = devices.device_for_rank(rank);
                                        futures.push(tokio::task::spawn_blocking(move || {
                                            let device = device.ok_or_else(|| {
                                                ModelLoadError::NoDeviceForRank(rank, devices)
                                            })?;
                                            match architecture {
                                                model::LLMArchitecture::HfLlama => {
                                                    LlamaForCausalLM::from_pretrained(
                                                        &source.try_into()?,
                                                        Some(Kind::BFloat16),
                                                        attn_implementation,
                                                        Some(device),
                                                        tensor_parallelism_world,
                                                        Some(llm.max_seq_len as usize),
                                                    )
                                                    .map(|x| Box::new(x) as Box<dyn CausalLM>)
                                                }
                                                model::LLMArchitecture::HfDeepseek => {
                                                    DeepseekForCausalLM::from_pretrained(
                                                        &source.try_into()?,
                                                        Some(Kind::BFloat16),
                                                        attn_implementation,
                                                        Some(device),
                                                        tensor_parallelism_world,
                                                        Some(llm.max_seq_len as usize),
                                                    )
                                                    .map(|x| Box::new(x) as Box<dyn CausalLM>)
                                                }
                                                model::LLMArchitecture::HfAuto
                                                | model::LLMArchitecture::Torchtitan => {
                                                    unreachable!()
                                                }
                                            }
                                        }));
                                    }
                                }

                                let mut models: Vec<Box<dyn CausalLM>> = Vec::new();
                                for future in futures {
                                    let model = future
                                        .await
                                        .map_err(InitRunError::ModelLoadingThreadCrashed)??;
                                    models.push(model);
                                }

                                RawLoadedModelType::ParallelNativeModels(models)
                            }
                        };

                        debug!("Config uploaded: {}", serialized_config);
                        let serialized_tokenizer = tokenizer.to_string(false).unwrap();
                        tx_config
                            .send((serialized_config.clone(), serialized_tokenizer))
                            .unwrap();

                        info!(
                            integration_test_log_marker = %IntegrationTestLogMarker::LoadedModel,
                            checkpoint = %llm.checkpoint,
                            gpus = init_config.data_parallelism * init_config.tensor_parallelism,
                            dp = init_config.data_parallelism,
                            tp = init_config.tensor_parallelism,
                            "loaded_model",
                        );

                        Ok(RawLoadedModel {
                            models: raw_loaded_model_type,
                            tokenizer,
                            model_task_runner,
                            checkpoint_extra_files,
                        })
                    })
                }
                model::Checkpoint::Ephemeral => return Err(InitRunError::ModelIsEphemeral),
            },
        };

        let wandb_future: JoinHandle<Result<Option<wandb::Run>, wandb::ApiError>> = tokio::spawn({
            let run_id = String::from(&state.run_id);
            async move {
                match init_config.wandb_info {
                    Some(wandb_info) => {
                        let wandb =
                            wandb::WandB::new(wandb::BackendOptions::new(wandb_info.api_key));
                        let mut run_info = wandb::RunInfo::new(wandb_info.project)
                            .name(wandb_info.run)
                            .config((
                                (
                                    "global_batch_size_start",
                                    state.config.global_batch_size_start,
                                ),
                                ("global_batch_size_end", state.config.global_batch_size_end),
                                (
                                    "global_batch_size_warmup_tokens",
                                    state.config.global_batch_size_warmup_tokens,
                                ),
                                ("total_steps", state.config.total_steps),
                                ("run_id", run_id),
                            ));
                        if let Some(entity) = wandb_info.entity {
                            run_info = run_info.entity(entity);
                        }
                        if let Some(group) = wandb_info.group {
                            run_info = run_info.group(group);
                        }
                        match wandb.new_run(run_info.build()?).await {
                            Ok(run) => Ok(Some(run)),
                            Err(e) => {
                                error!(
                                    "[init_run] Could not connect to wandb. Will continue training without it."
                                );
                                debug!("[init_run] wandb error: {:?}", e);
                                Ok(None)
                            }
                        }
                    }
                    None => {
                        info!(
                            "[init_run] No wandb info provided. Will continue training without it."
                        );
                        Ok(None)
                    }
                }
            }
        });

        let (data, models, wandb_run) = tokio::join!(data_future, model_future, wandb_future);
        let RawLoadedModel {
            models,
            tokenizer,
            checkpoint_extra_files,
            model_task_runner,
        } = models.map_err(InitRunError::ModelLoadingThreadCrashed)??;

        // TODO add data fetching for verifying, too..
        let data_provider = data.map_err(InitRunError::DataProviderConnect)?;
        let data_fetcher =
            DataFetcher::<T, A>::new(data_provider, init_config.data_parallelism * 2);

        let trainers: Vec<Trainer> = match models {
            RawLoadedModelType::ParallelNativeModels(models) => {
                let mut tp_models: Vec<Vec<Box<dyn CausalLM>>> = Vec::new();
                for model in models {
                    if tp_models
                        .last()
                        .map(|x| x.len() == init_config.tensor_parallelism)
                        .unwrap_or(true)
                    {
                        tp_models.push(Vec::with_capacity(init_config.tensor_parallelism));
                    }
                    tp_models.last_mut().unwrap().push(model);
                }

                let data_parallel: Option<Vec<(CommunicatorId, Arc<dyn Barrier>)>> =
                    if init_config.data_parallelism > 1 {
                        #[cfg(feature = "parallelism")]
                        {
                            Some(
                                (0..init_config.tensor_parallelism)
                                    .map(|_| {
                                        (
                                            tch::CStore::new().into(),
                                            Arc::new(CancellableBarrier::new(
                                                init_config.tensor_parallelism,
                                            ))
                                                as Arc<dyn Barrier>,
                                        )
                                    })
                                    .collect(),
                            )
                        }

                        #[cfg(not(feature = "parallelism"))]
                        {
                            unimplemented!()
                        }
                    } else {
                        None
                    };

                tp_models
                    .into_iter()
                    .enumerate()
                    .map(|(dp, models)| {
                        let data_parallel = data_parallel.as_ref().map(|data_parallel| {
                            data_parallel
                                .iter()
                                .map(|(id, barrier)| DataParallel {
                                    id: id.clone(),
                                    barrier: barrier.clone(),
                                    rank: dp,
                                    world_size: init_config.data_parallelism,
                                })
                                .collect()
                        });
                        let barrier =
                            Arc::new(CancellableBarrier::new(init_config.tensor_parallelism))
                                as Arc<dyn Barrier>;
                        LocalTrainer::new(
                            ParallelModels {
                                models,
                                barrier,
                                data_parallel,
                            },
                            llm.lr_schedule,
                            llm.optimizer,
                            init_config.micro_batch_size,
                            init_config.optim_stats_every_n_steps,
                            init_config.grad_accum_in_fp32,
                        )
                        .into()
                    })
                    .collect()
            }
            #[cfg(feature = "python")]
            RawLoadedModelType::Python(model) => {
                vec![
                    psyche_modeling::LocalTrainer::new(
                        ParallelModels {
                            models: vec![Box::new(model) as Box<dyn CausalLM>],
                            barrier: Arc::new(psyche_modeling::NopBarrier) as Arc<dyn Barrier>,
                            data_parallel: None,
                        },
                        llm.lr_schedule,
                        llm.optimizer,
                        init_config.micro_batch_size,
                        init_config.optim_stats_every_n_steps,
                        init_config.grad_accum_in_fp32,
                    )
                    .into(),
                ]
            }
            #[cfg(feature = "python")]
            RawLoadedModelType::PythonDistributed(model) => {
                vec![
                    psyche_modeling::PythonDistributedTrainer::new(
                        model,
                        llm.lr_schedule,
                        llm.optimizer,
                        init_config.micro_batch_size,
                        init_config.optim_stats_every_n_steps,
                        init_config.grad_accum_in_fp32,
                    )?
                    .into(),
                ]
            }
        };

        let wandb_run = wandb_run.map_err(InitRunError::WandbThreadCrashed)??;

        let stats_logger = StatsLogger::new(
            tokenizer,
            model_task_runner.clone(),
            llm.lr_schedule,
            wandb_run,
            metrics,
        );

        let warmup = WarmupStepMetadata {
            model_task_runner: model_task_runner.clone(),
        };

        let training = TrainingStepMetadata {
            data_fetcher,
            identity: init_config.identity,
            write_gradients_dir: init_config.write_gradients_dir,
            tx_health_check,
            tx_distro_result,

            model_task_runner: model_task_runner.clone(),
        };

        let witness = WitnessStepMetadata {
            model_task_runner: model_task_runner.clone(),
            identity: init_config.identity,
            tx_witness: tx_witness.clone(),
        };

        let cooldown = CooldownStepMetadata::new(
            tx_checkpoint,
            tx_model,
            init_config.checkpoint_config,
            checkpoint_extra_files,
            model_task_runner,
        );

        Ok(StepStateMachine::new(
            init_config.identity,
            warmup,
            training,
            witness,
            cooldown,
            trainers,
            state,
            tx_request_download,
            tx_witness,
            tx_broadcast_finished,
            stats_logger,
        ))
    }
}
