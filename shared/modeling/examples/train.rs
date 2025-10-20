use anyhow::{Context, Result};
use clap::{Parser, ValueEnum};
use psyche_core::{
    Barrier, BatchId, CancellableBarrier, ClosedInterval, CosineLR, OptimizerDefinition, Shuffle,
};
use psyche_data_provider::{
    DataProvider, LengthKnownDataProvider, LocalDataProvider, PreprocessedDataProvider, Split,
    TokenizedDataProvider, download_model_repo_sync,
};
use psyche_modeling::{
    AttentionImplementation, Batch, BatchData, BatchDataCPU, CausalLM, CommunicatorId,
    DataParallel, Devices, LocalTrainer, ModelLoadError, ParallelModels, Trainer,
    auto_model_for_causal_lm_from_pretrained,
};
use psyche_network::AuthenticatableIdentity;
use psyche_tui::{logging, setup_ctrl_c};
use std::{sync::Arc, thread::JoinHandle, time::SystemTime};
use tch::Kind;
use tracing::info;

#[derive(ValueEnum, Clone, Copy, Debug)]
enum AttnImpl {
    Eager,
    Sdpa,
    #[cfg(feature = "parallelism")]
    FlashAttention2,
}

impl From<AttnImpl> for AttentionImplementation {
    fn from(val: AttnImpl) -> Self {
        match val {
            AttnImpl::Eager => AttentionImplementation::Eager,
            AttnImpl::Sdpa => AttentionImplementation::Sdpa,
            #[cfg(feature = "parallelism")]
            AttnImpl::FlashAttention2 => AttentionImplementation::FlashAttention2,
        }
    }
}

#[derive(Clone, Debug, Hash, PartialEq, Eq, Default, Copy)]
struct DummyNodeIdentity(());

impl AuthenticatableIdentity for DummyNodeIdentity {
    type PrivateKey = ();

    fn from_signed_challenge_bytes(
        _bytes: &[u8],
        _challenge: [u8; 32],
    ) -> std::result::Result<Self, psyche_network::FromSignedBytesError> {
        unimplemented!()
    }

    fn to_signed_challenge_bytes(
        &self,
        _private_key: &Self::PrivateKey,
        _challenge: [u8; 32],
    ) -> Vec<u8> {
        unimplemented!()
    }

    fn get_p2p_public_key(&self) -> &[u8; 32] {
        unimplemented!()
    }

    fn raw_p2p_sign(&self, _private_key: &Self::PrivateKey, _bytes: &[u8]) -> [u8; 64] {
        unimplemented!()
    }
}

impl std::fmt::Display for DummyNodeIdentity {
    fn fmt(&self, _f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        unimplemented!()
    }
}

#[derive(Parser, Debug, Clone)]
struct Args {
    #[arg(long, default_value = "emozilla/llama2-215m-init")]
    model: String,

    #[arg(long, default_value = "data")]
    data_path: String,

    #[arg(long, default_value_t = 2048)]
    sequence_length: usize,

    #[arg(long, default_value_t = 2)]
    token_size: usize,

    #[arg(long, default_value_t = 8)]
    micro_batch: usize,

    #[arg(long, default_value_t = 256)]
    total_batch: usize,

    #[arg(long, default_value_t = 0.9)]
    beta1: f32,

    #[arg(long, default_value_t = 0.95)]
    beta2: f32,

    #[arg(long, default_value_t = 0.1)]
    weight_decay: f32,

    #[arg(long, default_value_t = 1e-8)]
    eps: f32,

    #[arg(long, default_value_t = 4e-4)]
    learning_rate: f64,

    #[arg(long, default_value_t = 500)]
    warmup_steps: u32,

    #[arg(long, default_value_t = 25000)]
    total_steps: u32,

    #[arg(long, default_value_t = 1.0)]
    max_grad_norm: f32,

    #[arg(long)]
    tensor_parallelism: Option<usize>,

    #[arg(long)]
    data_parallelism: Option<usize>,

    #[arg(long, default_value_t = false)]
    optim_stats: bool,

    #[arg(
        long,
        help = "Device(s) to use: auto, cpu, mps, cuda, cuda:N, cuda:X,Y,Z",
        default_value = "auto"
    )]
    device: Devices,

    #[arg(long, default_value_t = false)]
    grad_accum_in_fp32: bool,

    #[arg(long, default_value_t = 64)]
    compression_chunk: u16,

    #[arg(long, default_value_t = 4)]
    compression_topk: u16,

    #[arg(long, default_value_t = 0.999)]
    compression_decay: f32,

    #[arg(long, default_value_t = false)]
    distro: bool,

    #[arg(long, default_value_t = false)]
    distro_quantization: bool,

    #[arg(long)]
    attn_implementation: Option<AttnImpl>,

    #[arg(long, default_value_t = 1)]
    start_step: u32,

    #[cfg(feature = "python")]
    #[clap(long)]
    python: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    let logger = logging().init()?;
    psyche_modeling::set_suggested_env_vars();

    // For ctrl-c handling
    let cancel = setup_ctrl_c();

    let args = Args::parse();

    let target_device = args.device.device_for_rank(0).unwrap();

    let repo_files = if std::fs::exists(args.model.clone()).is_ok_and(|x| x) {
        std::fs::read_dir(args.model.clone())?
            .map(|x| x.unwrap().path())
            .collect()
    } else {
        download_model_repo_sync(
            &args.model.clone(),
            None,
            None,
            std::env::var("HF_TOKEN").ok(),
            true,
        )?
    };

    let mut dataset: DataProvider<DummyNodeIdentity> = match LocalDataProvider::new_from_directory(
        &args.data_path,
        args.token_size.try_into()?,
        args.sequence_length,
        Shuffle::DontShuffle,
    )
    .with_context(|| "Failed to load data with local data provider.")
    {
        Ok(dataset) => {
            info!(
                "Loaded local dataset with {} samples",
                dataset.num_sequences()
            );
            DataProvider::Local(dataset)
        }
        Err(err) => {
            println!(
                "Failed to load with local data provider. {err:?} Trying preprocessed data provider instead"
            );
            let dataset = PreprocessedDataProvider::new_from_directory(
                &args.data_path,
                args.sequence_length,
                Shuffle::DontShuffle,
                Some(Split::Train),
                None,
            )
            .with_context(|| "Failed to load preprocessed data")?;
            info!(
                "Loaded preprocessed dataset with {} samples",
                dataset.num_sequences()
            );
            DataProvider::Preprocessed(dataset)
        }
    };

    let schedule = CosineLR::new(
        args.learning_rate,
        args.warmup_steps,
        0.0,
        args.total_steps,
        args.learning_rate / 10.0,
    );

    let clip_grad_norm = match args.max_grad_norm {
        0. => None,
        x => Some(x),
    };

    let optimizer = match args.distro {
        true => OptimizerDefinition::Distro {
            clip_grad_norm,
            compression_decay: args.compression_decay,
            compression_topk: args.compression_topk,
            compression_chunk: args.compression_chunk,
            quantize_1bit: args.distro_quantization,
            weight_decay: Some(args.weight_decay),
        },
        false => OptimizerDefinition::AdamW {
            betas: [args.beta1, args.beta2],
            weight_decay: args.weight_decay,
            eps: args.eps,
            clip_grad_norm,
        },
    };

    let dp_world_size = args.data_parallelism.unwrap_or(1);
    let tp_world_size = args.tensor_parallelism.unwrap_or(1);

    #[cfg(feature = "python")]
    let python = args.python;
    #[cfg(not(feature = "python"))]
    let python = false;

    let data_parallel: Option<Vec<(CommunicatorId, Arc<dyn Barrier>)>> =
        if args.data_parallelism.is_some() && !python {
            {
                #[cfg(feature = "parallelism")]
                {
                    Some(
                        (0..tp_world_size)
                            .map(|_| {
                                (
                                    tch::CStore::new().into(),
                                    Arc::new(CancellableBarrier::new(tp_world_size))
                                        as Arc<dyn Barrier>,
                                )
                            })
                            .collect(),
                    )
                }

                #[cfg(not(feature = "parallelism"))]
                {
                    anyhow::bail!("Parallelism set but feature off")
                }
            }
        } else {
            None
        };

    let mut trainers: Vec<JoinHandle<Result<Trainer, anyhow::Error>>> = vec![];

    if python {
        #[cfg(feature = "python")]
        {
            psyche_python_extension_impl::init_embedded_python();

            let source = psyche_modeling::PretrainedSource::RepoFiles(repo_files);
            let dp = args.data_parallelism.unwrap_or(1);
            let tp = args.tensor_parallelism.unwrap_or(1);

            let trainer_load_handle: JoinHandle<std::result::Result<Trainer, anyhow::Error>> =
                std::thread::spawn(move || {
                    if dp != 1 || tp != 1 {
                        let model = psyche_modeling::PythonDistributedCausalLM::new(
                            "hf-auto".to_string(),
                            source,
                            target_device,
                            args.attn_implementation.map(Into::into).unwrap_or_default(),
                            psyche_modeling::ParallelismConfig { dp, tp },
                            Some(args.sequence_length),
                            None,
                            None,
                        )?;

                        Ok(psyche_modeling::PythonDistributedTrainer::new(
                            model,
                            schedule.into(),
                            optimizer,
                            args.micro_batch,
                            None,
                            args.grad_accum_in_fp32,
                        )?
                        .into())
                    } else {
                        let models = vec![Box::new(psyche_modeling::PythonCausalLM::new(
                            "hf-auto",
                            &source,
                            target_device,
                            args.attn_implementation.map(Into::into).unwrap_or_default(),
                            None,
                            Some(args.sequence_length),
                        )?) as Box<dyn CausalLM>];
                        Ok(LocalTrainer::new(
                            ParallelModels {
                                models,
                                barrier: Arc::new(CancellableBarrier::new(1)) as Arc<dyn Barrier>,
                                data_parallel: None,
                            },
                            schedule.into(),
                            optimizer,
                            args.micro_batch,
                            None,
                            args.grad_accum_in_fp32,
                        )
                        .into())
                    }
                });

            trainers.push(trainer_load_handle);
        }
    } else {
        let barrier = Arc::new(CancellableBarrier::new(tp_world_size)) as Arc<dyn Barrier>;
        for dp in 0..dp_world_size {
            let repo_files = repo_files.clone();
            let data_parallel = data_parallel.clone();
            let barrier = barrier.clone();
            let device = args.device.clone();
            let trainer_load_handle: JoinHandle<std::result::Result<Trainer, anyhow::Error>> =
                std::thread::spawn(move || {
                    let id = if tp_world_size > 1 {
                        #[cfg(feature = "parallelism")]
                        {
                            Some(tch::CStore::new().into())
                        }

                        #[cfg(not(feature = "parallelism"))]
                        {
                            anyhow::bail!("Parallelism set but not feature off")
                        }
                    } else {
                        None
                    };

                    let results = (0..tp_world_size)
                        .map(|tp| {
                            let rank = (dp * tp_world_size) + tp;
                            let device = device
                                .device_for_rank(rank)
                                .unwrap_or_else(|| panic!("no device for rank {rank}"));
                            let id = id.clone();
                            let repo_files = repo_files.clone();
                            let attn_implemention = args.attn_implementation.map(|x| x.into());

                            std::thread::spawn(move || {
                                let model: Box<dyn CausalLM> =
                                    auto_model_for_causal_lm_from_pretrained(
                                        repo_files,
                                        Some(Kind::BFloat16),
                                        attn_implemention,
                                        Some(device),
                                        id.map(|id| (id, tp, tp_world_size)),
                                        Some(args.sequence_length),
                                    )?;
                                model.prepare_for_training();
                                Ok(model)
                            })
                        })
                        .collect::<Vec<JoinHandle<Result<Box<dyn CausalLM>, ModelLoadError>>>>();
                    let results: Result<Vec<_>, _> =
                        results.into_iter().map(|x| x.join().unwrap()).collect();
                    let models = results?;
                    let data_parallel = data_parallel.map(|data_parallel| {
                        data_parallel
                            .iter()
                            .map(|(id, barrier)| DataParallel {
                                id: id.clone(),
                                barrier: barrier.clone(),
                                rank: dp,
                                world_size: dp_world_size,
                            })
                            .collect()
                    });
                    Ok(LocalTrainer::new(
                        ParallelModels {
                            models,
                            barrier,
                            data_parallel,
                        },
                        schedule.into(),
                        optimizer,
                        args.micro_batch,
                        None,
                        args.grad_accum_in_fp32,
                    )
                    .into())
                });

            trainers.push(trainer_load_handle);
        }
    }

    let trainers = trainers
        .into_iter()
        .map(|x| x.join().unwrap())
        .collect::<Result<Vec<_>, _>>();
    let mut trainers = trainers?;

    info!("Done loading, starting training.");

    let mut prev_distro_results = if args.distro { Some(vec![]) } else { None };
    for step in args.start_step..=args.total_steps {
        let start_time = SystemTime::now();
        let batch_id = BatchId(ClosedInterval::new(
            (step as u64 - 1) * args.total_batch as u64,
            (step as u64 * args.total_batch as u64) - 1,
        ));
        let data: Vec<BatchDataCPU> = dataset
            .get_samples(batch_id)
            .await?
            .into_iter()
            .map(|x| BatchDataCPU {
                input_ids: x.input_ids,
                labels: x.labels,
                position_ids: x.position_ids,
                sequence_lengths: x.sequence_lengths,
            })
            .collect();

        let trainings = data
            .chunks(data.len() / trainers.len())
            .zip(trainers)
            .map(|(data, trainer)| {
                let data = data.to_vec();
                let cancel = cancel.clone();
                let distro = args.distro;
                let distro_quantization = args.distro_quantization;
                let prev_distro_results = prev_distro_results.clone();
                std::thread::spawn(move || {
                    #[allow(irrefutable_let_patterns)]
                    if let Trainer::Local(trainer) = &trainer {
                        trainer.data_parallel_barrier();
                    }

                    let mut output = trainer
                        .train(
                            step,
                            Batch {
                                id: BatchId((step as u64, step as u64).into()),
                                data: BatchData::CPU(data),
                            },
                            None,
                            false,
                            vec![],
                            prev_distro_results.clone(),
                            cancel.clone(),
                        )
                        .unwrap();
                    if !distro || step > args.start_step {
                        output.trainer = output
                            .trainer
                            .optimize(
                                step,
                                None,
                                prev_distro_results.map(|x| {
                                    if distro_quantization {
                                        x.into_iter()
                                            .map(|y| Trainer::quantize_results(&y))
                                            .collect()
                                    } else {
                                        x
                                    }
                                }),
                            )
                            .unwrap()
                    }
                    output
                })
            })
            .collect::<Vec<_>>();

        let mut loss = 0.;
        let joined_trainers = trainings
            .into_iter()
            .map(|x| x.join().unwrap())
            .collect::<Vec<_>>();
        trainers = joined_trainers
            .into_iter()
            .enumerate()
            .map(|(index, output)| {
                // take the first index -- all outputs should be identical after dp/tp reduction
                if index == 0 {
                    prev_distro_results = output.distro_results.map(|x| vec![x]);
                    loss = output.loss;
                }
                output.trainer
            })
            .collect();

        let duration = SystemTime::now()
            .duration_since(start_time)
            .unwrap()
            .as_secs_f32();

        info!(
            "step: {}, duration: {:.2}, batch: {}, loss: {:.4}",
            step, duration, batch_id, loss
        );
        if cancel.is_cancelled() {
            break;
        }
    }
    for trainer in trainers {
        trainer.shutdown();
    }
    logger.shutdown()?;
    Ok(())
}
