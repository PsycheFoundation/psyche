use crate::{IntegrationTestLogMarker, WandBInfo, fetch_data::DataFetcher};
use psyche_coordinator::{
    Coordinator, HealthChecks,
    model::{self, HttpLLMTrainingDataLocation, LLMTrainingDataLocation},
};
use psyche_core::{Barrier, CancellableBarrier, NodeIdentity, TokenSize};
use psyche_data_provider::{
    DataProvider, DataProviderTcpClient, DummyDataProvider, WeightedDataProvider,
    download_model_repo_async,
    http::{FileURLs, HttpDataProvider},
};
use psyche_modeling::{
    AutoConfig, AutoTokenizerError, CausalLM, CommunicatorId, DataParallel, DeepseekForCausalLM,
    DummyModel, LlamaConfig, LlamaForCausalLM, LocalTrainer, ModelConfig, ModelLoadError,
    ParallelModels, PretrainedSource, Trainer, auto_tokenizer,
};
use psyche_network::{AuthenticatableIdentity, BlobTicket};
use psyche_watcher::OpportunisticData;
use std::{collections::HashMap, path::PathBuf, sync::Arc};
use tch::{Device, Kind, Tensor};
use thiserror::Error;
use tokenizers::{ModelWrapper, Tokenizer, models::wordlevel::WordLevel};
use tokio::{
    io,
    sync::{mpsc::UnboundedSender, oneshot},
    task::{JoinError, JoinHandle},
};
use tracing::{debug, error, info};

use super::{
    CheckpointConfig, FinishedBroadcast, cooldown::CooldownStepMetadata, evals::EvalRunner,
    stats::StatsLogger, steps::StepStateMachine, train::TrainingStepMetadata,
    types::DistroBroadcastAndPayload, warmup::WarmupStepMetadata, witness::WitnessStepMetadata,
};

pub struct RunInitConfig<T: NodeIdentity, A: AuthenticatableIdentity> {
    // identity for connecting to the data server
    pub identity: T,
    pub network_identity: A,
    pub private_key: A::PrivateKey,

    // p2p model parameters sharing config
    pub max_concurrent_parameter_requests: usize,

    // model & dataload
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

    // logging
    pub wandb_info: Option<WandBInfo>,

    // debugging
    pub write_gradients_dir: Option<PathBuf>,

    // checkpointing
    pub checkpoint_config: Option<CheckpointConfig>,

    // configurable dummy training time (in seconds) for this client - relevant just for testing
    pub dummy_training_delay_secs: Option<u64>,
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

    #[error("Unsupported architeture: {0}")]
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
    eval_runner: EvalRunner,
    checkpoint_extra_files: Vec<PathBuf>,
}

type OneshotModelParameterSender = oneshot::Sender<HashMap<String, Tensor>>;
type OneShotModelConfigSender = oneshot::Sender<(String, Tokenizer)>;

pub struct RunInitConfigAndIO<T: NodeIdentity, A: AuthenticatableIdentity> {
    pub init_config: RunInitConfig<T, A>,

    pub tx_health_check: UnboundedSender<HealthChecks<T>>,
    pub tx_witness: UnboundedSender<OpportunisticData>,
    pub tx_checkpoint: UnboundedSender<model::HubRepo>,
    pub tx_model: UnboundedSender<HashMap<String, Tensor>>,
    pub tx_parameters_req: UnboundedSender<(Vec<String>, OneshotModelParameterSender)>,
    pub tx_config: UnboundedSender<(String, String)>,
    pub tx_distro_result: UnboundedSender<DistroBroadcastAndPayload>,
    pub tx_request_download: UnboundedSender<(BlobTicket, u32)>,
    pub tx_request_model_config: UnboundedSender<OneShotModelConfigSender>,
    pub tx_broadcast_finished: UnboundedSender<FinishedBroadcast>,
}

impl<T: NodeIdentity, A: AuthenticatableIdentity + 'static> RunInitConfigAndIO<T, A> {
    /// Call this on first warmup - when we need to enter the run, we have to load the model, conenct to the data server, etc
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
        } = self;

        let model::Model::LLM(llm) = state.model;

        let data_future = async {
            info!("LLM Config: {:?}", llm);
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
            };
            Ok(data_provider)
        };

        let model_future: JoinHandle<Result<RawLoadedModel, InitRunError>> = match &llm.architecture
        {
            model::LLMArchitecture::HfLlama
            | model::LLMArchitecture::HfDeepseek
            | model::LLMArchitecture::HfAuto => match &llm.checkpoint {
                model::Checkpoint::Dummy(_) => tokio::spawn(async move {
                    info!("Checkpoint is Dummy, creating dummy model");
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
                        eval_runner: EvalRunner::new(vec![], tokenizer.clone(), None, 0),
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
                model::Checkpoint::Hub(_) | model::Checkpoint::P2P(_) => {
                    let checkpoint = llm.checkpoint;
                    tokio::spawn(async move {
                        // Track if we detected a dummy model to initialize as DummyModel at the end
                        let mut is_dummy_model = false;

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
                                    info!("Downloading {} (if needed)", hub_repo.repo_id);
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
                            model::Checkpoint::P2P(_) => {
                                let (tx_model_config_response, rx_model_config_response) =
                                    oneshot::channel();
                                info!("Checkpoint is p2p, requesting model config over network");

                                tx_request_model_config
                                    .send(tx_model_config_response)
                                    .unwrap();

                                let (model_config, tokenizer) =
                                    rx_model_config_response.await.unwrap();
                                debug!("Got p2p info, model_config: {}", model_config);

                                let model_config = match llm.architecture {
                                    model::LLMArchitecture::HfLlama => {
                                        let llama_config: psyche_modeling::LlamaConfig =
                                            serde_json::from_str(&model_config)?;
                                        // Check if this is actually a dummy model shared via P2P
                                        if llama_config.is_dummy {
                                            info!(
                                                "Detected dummy model config via P2P, will continue with P2P logic but create DummyModel at the end"
                                            );
                                            is_dummy_model = true;
                                        }
                                        AutoConfig::Llama(llama_config)
                                    }
                                    model::LLMArchitecture::HfDeepseek => {
                                        AutoConfig::Deepseek(serde_json::from_str(&model_config)?)
                                    }
                                    model::LLMArchitecture::HfAuto => {
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
                                                "HfAuto".to_string(),
                                            ));
                                        }
                                    }
                                };
                                let parameter_names = model_config.get_parameter_names();
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
                            _ => unreachable!(),
                        };

                        info!("Loading model...");

                        let serialized_config = source.serialize_config()?;

                        let eval_runner = EvalRunner::new(
                            init_config.eval_tasks,
                            tokenizer.clone(),
                            init_config.eval_task_max_docs,
                            init_config.data_parallelism,
                        );

                        let raw_loaded_model_type: RawLoadedModelType = if llm.architecture
                            == model::LLMArchitecture::HfAuto
                        {
                            #[cfg(feature = "python")]
                            {
                                let dp = init_config.data_parallelism;
                                let tp = init_config.tensor_parallelism;

                                tokio::task::spawn_blocking(move || {
                                    if tp != 1 || dp != 1 {
                                        psyche_modeling::PythonDistributedCausalLM::new(
                                            "hf-auto".to_string(),
                                            source.try_into()?,
                                            Device::cuda_if_available(),
                                            psyche_modeling::ParallelismConfig { dp, tp },
                                            Some(llm.max_seq_len as usize),
                                        )
                                        .map(RawLoadedModelType::PythonDistributed)
                                        .map_err(InitRunError::PythonDistributedError)
                                    } else {
                                        psyche_modeling::PythonCausalLM::new(
                                            "hf-auto",
                                            &source.try_into()?,
                                            Device::cuda_if_available(),
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
                                    "HfAuto".to_string(),
                                ));
                            }
                        } else {
                            let mut futures: Vec<
                                JoinHandle<Result<Box<dyn CausalLM>, ModelLoadError>>,
                            > = Vec::with_capacity(
                                init_config.data_parallelism * init_config.tensor_parallelism,
                            );

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
                                    let is_dummy_model = is_dummy_model;
                                    futures.push(tokio::task::spawn_blocking(move || {
                                        if is_dummy_model {
                                            if let Some(training_delay) =
                                                init_config.dummy_training_delay_secs
                                            {
                                                return Ok(Box::new(
                                                    psyche_modeling::DummyModel::new(
                                                        training_delay,
                                                    ),
                                                )
                                                    as Box<dyn psyche_modeling::CausalLM>);
                                            } else {
                                                return Ok(Box::new(
                                                    psyche_modeling::DummyModel::default(),
                                                )
                                                    as Box<dyn psyche_modeling::CausalLM>);
                                            }
                                        }

                                        // let this run on CPU if tp is 1 and no cuda is available
                                        let device = if init_config.tensor_parallelism == 1 {
                                            if dp == 0 {
                                                Device::cuda_if_available()
                                            } else {
                                                Device::Cuda(dp)
                                            }
                                        } else {
                                            Device::Cuda(dp * init_config.tensor_parallelism + tp)
                                        };
                                        match llm.architecture {
                                            model::LLMArchitecture::HfLlama => {
                                                LlamaForCausalLM::from_pretrained(
                                                    &source.try_into()?,
                                                    Some(Kind::BFloat16),
                                                    None,
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
                                                    None,
                                                    Some(device),
                                                    tensor_parallelism_world,
                                                    Some(llm.max_seq_len as usize),
                                                )
                                                .map(|x| Box::new(x) as Box<dyn CausalLM>)
                                            }
                                            model::LLMArchitecture::HfAuto => unreachable!(),
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
                            eval_runner,
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
                                ("rounds_per_epoch", state.config.rounds_per_epoch),
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
            eval_runner,
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

        let stats_logger =
            StatsLogger::new(tokenizer, eval_runner.clone(), llm.lr_schedule, wandb_run);

        let warmup = WarmupStepMetadata {
            eval_runner: eval_runner.clone(),
        };

        let training = TrainingStepMetadata {
            data_fetcher,
            identity: init_config.identity,
            write_gradients_dir: init_config.write_gradients_dir,
            tx_health_check,
            tx_distro_result,

            eval_runner: eval_runner.clone(),
        };

        let witness = WitnessStepMetadata {
            eval_runner: eval_runner.clone(),
            identity: init_config.identity,
            tx_witness: tx_witness.clone(),
        };

        let cooldown = CooldownStepMetadata::new(
            tx_checkpoint,
            tx_model,
            init_config.checkpoint_config,
            checkpoint_extra_files,
            eval_runner,
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
