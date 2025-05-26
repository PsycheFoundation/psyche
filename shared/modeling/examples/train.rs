use anyhow::Result;
use clap::Parser;
use psyche_core::{BatchId, CancellableBarrier, CosineLR, OptimizerDefinition, Shuffle};
use psyche_data_provider::{download_model_repo_sync, LocalDataProvider};
use psyche_modeling::{
    auto_model_for_causal_lm_from_pretrained, Batch, BatchData, CausalLM, CommunicatorId,
    DataParallel, ModelLoadError, Trainer,
};
use psyche_tui::{init_logging, LogOutput};
use std::{sync::Arc, thread::JoinHandle, time::SystemTime};
use tch::{Device, Kind};
use tokio_util::sync::CancellationToken;
use tracing::{info, Level};

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

    #[arg(long, default_value_t = 64)]
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
}

fn main() -> Result<()> {
    let logger = init_logging(LogOutput::Console, Level::INFO, None, false, None)?;
    psyche_modeling::set_suggested_env_vars();

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

    let data_parallel: Option<Vec<(Arc<CommunicatorId>, Arc<CancellableBarrier>)>> =
        if args.data_parallelism.is_some() {
            {
                #[cfg(feature = "parallelism")]
                {
                    Some(
                        (0..tp_world_size)
                            .map(|_| {
                                (
                                    tch::CStore::new().into(),
                                    CancellableBarrier::new(dp_world_size).into(),
                                )
                            })
                            .collect(),
                    )
                }

                #[cfg(not(feature = "parallelism"))]
                {
                    anyhow::bail!("Parallelism set but not feature off")
                }
            }
        } else {
            None
        };

    let mut trainers: Vec<JoinHandle<Result<Trainer, anyhow::Error>>> = vec![];
    for dp in 0..dp_world_size {
        let repo_files = repo_files.clone();
        let data_parallel = data_parallel.clone();
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
                        let device = if args.cpu && tp_world_size <= 1 {
                            Device::Cpu
                        } else {
                            Device::Cuda(rank)
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
                Ok(Trainer::new(
                    models,
                    schedule.into(),
                    optimizer,
                    args.micro_batch,
                    None,
                    args.grad_accum_in_fp32,
                    data_parallel,
                ))
            });

        trainers.push(trainer_load_handle);
    }
    let trainers = trainers
        .into_iter()
        .map(|x| x.join().unwrap())
        .collect::<Result<Vec<_>, _>>();
    let mut trainers = trainers?;

    info!("Done loading, starting training.");

    let cancel = CancellationToken::new();
    let mut dataset = dataset.into_iter();
    let mut prev_distro_results = if args.distro { Some(vec![]) } else { None };
    for step in 1..=args.total_steps {
        let start_time = SystemTime::now();
        let data: Vec<Vec<i32>> = (0..args.total_batch)
            .map(|_| dataset.next().unwrap())
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
                    trainer.data_parallel_barrier();

                    let mut output = trainer
                        .train(
                            step,
                            Batch {
                                id: BatchId((0, 0).into()), // batch id not needed
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
            "step: {}, duration: {:.1}, loss: {:.4}",
            step, duration, loss
        );
    }
    logger.shutdown()?;
    Ok(())
}
