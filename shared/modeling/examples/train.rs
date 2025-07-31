use anyhow::Result;
use clap::Parser;
use psyche_core::{Barrier, BatchId, CancellableBarrier, CosineLR, OptimizerDefinition, Shuffle};
use psyche_data_provider::{LocalDataProvider, download_model_repo_sync};
use psyche_modeling::{
    Batch, BatchData, CausalLM, CommunicatorId, DataParallel, LocalTrainer, ModelLoadError,
    ParallelModels, Trainer, auto_model_for_causal_lm_from_pretrained, get_optimal_device,
    parse_device,
};
use psyche_tui::{logging, setup_ctrl_c};
use std::{sync::Arc, thread::JoinHandle, time::SystemTime};
use tch::{Device, Kind};
use tracing::info;

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

    #[arg(long, default_value_t = false)]
    cpu: bool,

    #[arg(long, help = "Device to use: cpu, mps, cuda, cuda:N")]
    device: Option<String>,

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

    #[cfg(feature = "python")]
    #[clap(long)]
    python: bool,
}

fn get_device(args: &Args) -> Result<Device> {
    if let Some(device_str) = &args.device {
        parse_device(device_str)
    } else if args.cpu {
        Ok(Device::Cpu)
    } else {
        Ok(get_optimal_device())
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let logger = logging().init()?;
    psyche_modeling::set_suggested_env_vars();

    // For ctrl-c handling
    let cancel = setup_ctrl_c();

    let args = Args::parse();
    let repo_files = if std::fs::exists(args.model.clone()).is_ok_and(|x| x) {
        std::fs::read_dir(args.model.clone())?
            .map(|x| x.unwrap().path())
            .collect()
    } else {
        download_model_repo_sync(&args.model.clone(), None, None, None, true)?
    };
    info!(
        "starting training run: model {}, data_path {}, sequence_length {}, token_size {}, micro_batch {}, total_batch {}, beta1 {:.9}, beta2 {:.9}, weight_decay {:.9}, eps {:.9}, learning_rate {:.9}, warmup_steps {}, total_steps {}, max_grad_norm {:.9}, grad_accum_in_fp32 {}, compression_chunk {}, compression_topk {}, compression_decay {}, distro {}, distro quantization {}",
        args.model,
        args.data_path,
        args.sequence_length,
        args.token_size,
        args.micro_batch,
        args.total_batch,
        args.beta1,
        args.beta2,
        args.weight_decay,
        args.eps,
        args.learning_rate,
        args.warmup_steps,
        args.total_steps,
        args.max_grad_norm,
        args.grad_accum_in_fp32,
        args.compression_chunk,
        args.compression_topk,
        args.compression_decay,
        args.distro,
        args.distro_quantization,
    );

    let dataset = LocalDataProvider::new_from_directory(
        &args.data_path,
        args.token_size.try_into()?,
        args.sequence_length,
        Shuffle::DontShuffle,
    )?;

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
    if args.total_batch % dp_world_size != 0 {
        anyhow::bail!("DP world size doesn't divide global batch size");
    }
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
                        let device = get_device(&args)?;
                        let model = psyche_modeling::PythonDistributedCausalLM::new(
                            "hf-auto".to_string(),
                            source,
                            device,
                            psyche_modeling::ParallelismConfig { dp, tp },
                            Some(args.sequence_length),
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
                        let device = get_device(&args)?;
                        let models = vec![Box::new(psyche_modeling::PythonCausalLM::new(
                            "hf-auto",
                            &source,
                            device,
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
            let args_clone = args.clone();
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
                            let device = if tp_world_size > 1 {
                                // Multi-GPU setup requires CUDA
                                Device::Cuda(rank)
                            } else {
                                get_device(&args_clone).unwrap_or(Device::Cpu)
                            };
                            let id = id.clone();
                            let repo_files = repo_files.clone();

                            std::thread::spawn(move || {
                                let mut model = auto_model_for_causal_lm_from_pretrained(
                                    repo_files,
                                    Some(Kind::BFloat16),
                                    None,
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

    let mut dataset = dataset.into_iter();
    let mut prev_distro_results = if args.distro { Some(vec![]) } else { None };
    for step in 1..=args.total_steps {
        let start_time = SystemTime::now();
        let data: Vec<Vec<i32>> = (0..args.total_batch)
            .map(|_| dataset.next().unwrap())
            .collect();

        let trainings = data
            .chunks(data.len() / dp_world_size)
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
                                data: BatchData::CPU(data.to_vec()),
                            },
                            None,
                            false,
                            vec![],
                            prev_distro_results.clone(),
                            cancel.clone(),
                        )
                        .unwrap();
                    if !distro || step > 1 {
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
            "step: {}, duration: {:.2}, loss: {:.4}",
            step, duration, loss
        );
        if cancel.is_cancelled() {
            break;
        }
    }
    logger.shutdown()?;
    Ok(())
}
