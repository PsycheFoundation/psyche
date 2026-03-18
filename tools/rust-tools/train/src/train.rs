use anyhow::Result;
use psyche_coordinator::model::LLMArchitecture;
use psyche_core::{Barrier, BatchId, CancellableBarrier, ClosedInterval, OptimizerDefinition};
use psyche_data_provider::{DataProvider, TokenizedDataProvider, download_model_repo_sync};
use psyche_modeling::{
    Batch, BatchData, BatchDataCPU, CausalLM, CommunicatorId, DataParallel, LocalTrainer,
    ModelLoadError, ParallelModels, Trainer, auto_model_for_causal_lm_from_pretrained,
    save_tensors_into_safetensors,
};
use std::{path::Path, sync::Arc, thread::JoinHandle, time::SystemTime};
use tch::Kind;
use tracing::info;

use crate::config::TrainParams;

pub async fn run(
    args: TrainParams,
    dataset: &mut DataProvider,
    cancel: tokio_util::sync::CancellationToken,
) -> Result<()> {
    let repo_files = if Path::new(&args.model).exists() {
        std::fs::read_dir(&args.model)?
            .map(|x| x.unwrap().path())
            .collect()
    } else {
        download_model_repo_sync(
            &args.model,
            None,
            None,
            std::env::var("HF_TOKEN").ok(),
            true,
        )?
    };

    info!("Loading model from fileset {:?}", repo_files);

    let is_distro = matches!(args.optimizer, OptimizerDefinition::Distro { .. });

    let dp_world_size = args.data_parallelism.unwrap_or(1);
    let tp_world_size = args.tensor_parallelism.unwrap_or(1);

    let mut trainers: Vec<JoinHandle<Result<Trainer, anyhow::Error>>> = vec![];

    match args.architecture {
        LLMArchitecture::HfAuto | LLMArchitecture::Torchtitan => {
            #[cfg(feature = "python")]
            {
                psyche_python_extension_impl::init_embedded_python()?;

                let source = psyche_modeling::PretrainedSource::RepoFiles(repo_files);
                let attn_implementation = args.attn_implementation;
                let sequence_length = args.sequence_length;
                let arch = args.architecture.to_python_model_string();
                let lr_schedule = args.lr_schedule;
                let optimizer = args.optimizer;
                let micro_batch = args.micro_batch;
                let grad_accum_in_fp32 = args.grad_accum_in_fp32;

                let trainer_load_handle: JoinHandle<std::result::Result<Trainer, anyhow::Error>> =
                    std::thread::spawn(move || {
                        if dp_world_size != 1 || tp_world_size != 1 {
                            let device = args.device.device_for_rank(0).unwrap();
                            let model = psyche_modeling::PythonDistributedCausalLM::new(
                                arch,
                                source,
                                device,
                                attn_implementation.unwrap_or_default(),
                                psyche_modeling::ParallelismConfig {
                                    dp: dp_world_size,
                                    tp: tp_world_size,
                                },
                                Some(sequence_length),
                                None,
                                Some(args.device.size() as i64),
                            )?;

                            Ok(psyche_modeling::PythonDistributedTrainer::new(
                                model,
                                lr_schedule,
                                optimizer,
                                micro_batch,
                                None,
                                grad_accum_in_fp32,
                            )?
                            .into())
                        } else {
                            let device = args.device.device_for_rank(0).unwrap();
                            let models = vec![Box::new(psyche_modeling::PythonCausalLM::new(
                                &arch,
                                &source,
                                device,
                                attn_implementation.unwrap_or_default(),
                                None,
                                Some(sequence_length),
                            )?) as Box<dyn CausalLM>];
                            Ok(LocalTrainer::new(
                                ParallelModels {
                                    models,
                                    barrier: Arc::new(CancellableBarrier::new(1))
                                        as Arc<dyn Barrier>,
                                    data_parallel: None,
                                },
                                lr_schedule,
                                optimizer,
                                micro_batch,
                                None,
                                grad_accum_in_fp32,
                            )
                            .into())
                        }
                    });

                trainers.push(trainer_load_handle);
            }

            #[cfg(not(feature = "python"))]
            {
                anyhow::bail!("Unsupported architecture: {}", args.architecture);
            }
        }

        _ => {
            let data_parallel: Option<Vec<(CommunicatorId, Arc<dyn Barrier>)>> =
                if args.data_parallelism.is_some() {
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

            let barrier = Arc::new(CancellableBarrier::new(tp_world_size)) as Arc<dyn Barrier>;
            let attn_implementation = args.attn_implementation;
            let sequence_length = args.sequence_length;
            let lr_schedule = args.lr_schedule;
            let optimizer = args.optimizer;
            let micro_batch = args.micro_batch;
            let grad_accum_in_fp32 = args.grad_accum_in_fp32;

            for dp in 0..dp_world_size {
                let repo_files = repo_files.clone();
                let data_parallel = data_parallel.clone();
                let barrier = barrier.clone();
                let device = args.device.clone();
                let trainer_load_handle: JoinHandle<std::result::Result<Trainer, anyhow::Error>> =
                    std::thread::spawn(move || {
                        let id: Option<CommunicatorId> = match tp_world_size {
                            0 | 1 => None,
                            #[cfg(feature = "parallelism")]
                            _ => Some(tch::CStore::new().into()),
                            #[cfg(not(feature = "parallelism"))]
                            _ => anyhow::bail!("Parallelism set but feature off"),
                        };

                        let results = (0..tp_world_size)
                            .map(|tp| {
                                let rank = (dp * tp_world_size) + tp;
                                let device = device
                                    .device_for_rank(rank)
                                    .unwrap_or_else(|| panic!("no device for rank {rank}"));
                                let id = id.clone();
                                let repo_files = repo_files.clone();

                                std::thread::spawn(move || {
                                    let model: Box<dyn CausalLM> =
                                        auto_model_for_causal_lm_from_pretrained(
                                            repo_files,
                                            Some(Kind::BFloat16),
                                            attn_implementation,
                                            Some(device),
                                            id.map(|id| (id, tp, tp_world_size)),
                                            Some(sequence_length),
                                        )?;
                                    model.prepare_for_training();
                                    Ok(model)
                                })
                            })
                            .collect::<Vec<JoinHandle<Result<Box<dyn CausalLM>, ModelLoadError>>>>(
                            );

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
                            lr_schedule,
                            optimizer,
                            micro_batch,
                            None,
                            grad_accum_in_fp32,
                        )
                        .into())
                    });

                trainers.push(trainer_load_handle);
            }
        }
    }

    let trainers = trainers
        .into_iter()
        .map(|x| x.join().unwrap())
        .collect::<Result<Vec<_>, _>>();
    let mut trainers = trainers?;

    info!("Done loading, starting training.");

    let mut prev_distro_results = if is_distro { Some(vec![]) } else { None };
    let distro_quantization = matches!(
        args.optimizer,
        OptimizerDefinition::Distro {
            quantize_1bit: true,
            ..
        }
    );

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
                                id: batch_id,
                                data: BatchData::CPU(data),
                            },
                            None,
                            false,
                            vec![],
                            prev_distro_results.clone(),
                            cancel.clone(),
                        )
                        .unwrap();
                    if !is_distro || step > args.start_step {
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

    if let Some(save_path) = args.save_path {
        let extracted = trainers[0].extract()?;
        println!("Extracted {} tensors", extracted.len());

        let _ = save_tensors_into_safetensors(
            trainers[0].convert(Some(extracted)),
            save_path.clone().into(),
        )?;
        println!("Saved checkpoint to {}", save_path)
    }

    for trainer in trainers {
        trainer.shutdown();
    }
    Ok(())
}
