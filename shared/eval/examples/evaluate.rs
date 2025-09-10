use anyhow::Result;
use clap::Parser;
use psyche_core::RunningAverage;
use psyche_data_provider::download_model_repo_sync;
use psyche_eval::{ALL_TASK_NAMES, EvalTaskOptions, Task, tasktype_from_name};
use psyche_modeling::{CommunicatorId, CausalLM, auto_model_for_causal_lm_from_pretrained, auto_tokenizer};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Barrier};
use std::thread::JoinHandle;
use tch::{Device, Kind};
use tokenizers::Tokenizer;

#[derive(Parser, Debug, Clone)]
struct Args {
    #[arg(long, default_value = "NousResearch/Llama-2-7b-hf")]
    model: String,

    #[arg(long)]
    revision: Option<String>,

    #[arg(long)]
    hf_token: Option<String>,

    #[arg(long, default_value_t = ALL_TASK_NAMES.join(","))]
    tasks: String,

    #[arg(long, default_value_t = 0)]
    num_fewshot: usize,

    #[arg(long, default_value_t = 42)]
    seed: u64,

    #[arg(long, default_value_t = false)]
    quiet: bool,

    #[arg(long, default_value_t = 1)]
    data_parallelism: usize,

    #[arg(long)]
    tensor_parallelism: Option<usize>,

    #[cfg(feature = "python")]
    #[clap(long)]
    python: bool,
}

fn main() -> Result<()> {
    let args = Args::parse();
    let tasks: Result<Vec<Task>> = args
        .tasks
        .split(",")
        .map(|x| tasktype_from_name(x).map(|y| Task::new(y, args.num_fewshot, args.seed)))
        .collect();
    let tasks = tasks?;

    let tp_world_size = args.tensor_parallelism.unwrap_or(1);
    let total_gpus = args.data_parallelism * tp_world_size;

    if total_gpus > 1 {
        #[cfg(feature = "parallelism")]
        {
            if !tch::utils::has_cuda() {
                anyhow::bail!("CUDA not available but parallelism requested");
            }
            let available_gpus = tch::Cuda::device_count() as usize;
            if available_gpus < total_gpus {
                anyhow::bail!(
                    "Requested {} GPUs ({}x DP * {}x TP) but only {} available",
                    total_gpus,
                    args.data_parallelism,
                    tp_world_size,
                    available_gpus
                );
            }
        }
        #[cfg(not(feature = "parallelism"))]
        {
            anyhow::bail!(
                "Parallelism > 1 requested but 'parallelism' feature not enabled. Use --features parallelism"
            );
        }
    }

    let repo = download_model_repo_sync(&args.model, args.revision, None, args.hf_token, true)?;
    let tokenizer = auto_tokenizer(&repo)?;

    let python = {
        #[cfg(feature = "python")]
        {
            args.python
        }

        #[cfg(not(feature = "python"))]
        {
            false
        }
    };

    match tp_world_size {
        1 => {
            // Data parallelism only
            run_data_parallel(
                tasks,
                repo,
                tokenizer,
                args.data_parallelism,
                args.quiet,
                args.num_fewshot,
                args.seed,
                python,
            )?;
        }
        _ => {
            // Tensor parallelism (with optional data parallelism)
            run_with_tensor_parallelism(
                tasks,
                repo,
                tokenizer,
                args.data_parallelism,
                tp_world_size,
                args.quiet,
                args.num_fewshot,
                args.seed,
                python,
            )?;
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn run_data_parallel(
    tasks: Vec<Task>,
    repo: Vec<PathBuf>,
    tokenizer: Tokenizer,
    data_parallelism: usize,
    quiet: bool,
    num_fewshot: usize,
    seed: u64,
    python: bool,
) -> Result<()> {
    let task_info: Vec<(String, usize, u64)> = tasks
        .iter()
        .map(|task| {
            (format!("{task}"), num_fewshot, seed) // task_name, num_fewshot, seed
        })
        .collect();

    let shared_results: Vec<Arc<RunningAverage>> = task_info
        .iter()
        .map(|_| Arc::new(RunningAverage::new()))
        .collect();

    let threads = if python && data_parallelism > 1 {
        1
    } else {
        data_parallelism
    };

    let mut gpu_handles: Vec<JoinHandle<Result<()>>> = vec![];
    for gpu_id in 0..threads {
        let repo = repo.clone();
        let tokenizer = tokenizer.clone();
        let shared_results = shared_results.clone();
        let task_info = task_info.clone();

        let handle = std::thread::spawn(move || -> Result<()> {
            let device = if data_parallelism == 1 {
                Device::cuda_if_available()
            } else {
                Device::Cuda(gpu_id)
            };

            let mut model: Box<dyn CausalLM> = if python {
                #[cfg(feature = "python")]
                {
                    psyche_python_extension_impl::init_embedded_python();

                    Box::new(psyche_modeling::PythonDistributedCausalLM::new(
                        "hf-auto".to_string(),
                        psyche_modeling::PretrainedSource::RepoFiles(repo),
                        device,
                        psyche_modeling::AttentionImplementation::default(),
                        psyche_modeling::ParallelismConfig {
                            dp: data_parallelism,
                            tp: 1,
                        },
                        None,
                    )?) as Box<dyn CausalLM>
                }

                #[cfg(not(feature = "python"))]
                {
                    unreachable!();
                }
            } else {
                auto_model_for_causal_lm_from_pretrained(
                    repo,
                    Some(Kind::BFloat16),
                    None,
                    Some(device),
                    None,
                    None,
                )? as Box<dyn CausalLM>
            };

            for (task_idx, (task_name, num_fewshot, seed)) in task_info.into_iter().enumerate() {
                let task_type = tasktype_from_name(&task_name)?;
                let task = Task::new(task_type, num_fewshot, seed + task_idx as u64);

                let _res = task.prepare(&tokenizer, None).run(
                    EvalTaskOptions {
                        model: model.as_mut(),
                        skip_and_step_by: Some((gpu_id, threads)),
                        live_results: Some(shared_results[task_idx].clone()),
                        cancel: None,
                        limit: None,
                    },
                    !quiet,
                );
            }

            Ok(())
        });

        gpu_handles.push(handle);
    }

    // Wait for all GPU workers to complete
    for handle in gpu_handles {
        handle
            .join()
            .map_err(|_| anyhow::anyhow!("GPU worker thread panicked"))??;
    }

    for ((task_name, _, _), running_avg) in task_info.into_iter().zip(shared_results) {
        let final_scores = running_avg
            .get_all_averages()
            .into_iter()
            .map(|(key, value)| (key, value.unwrap_or(0.0)))
            .collect::<HashMap<String, f64>>();
        println!("{task_name}: {final_scores:?}");
    }

    Ok(())
}

fn run_with_tensor_parallelism(
    tasks: Vec<Task>,
    repo: Vec<PathBuf>,
    tokenizer: Tokenizer,
    data_parallelism: usize,
    tp_world_size: usize,
    quiet: bool,
    num_fewshot: usize,
    seed: u64,
) -> Result<()> {
    let task_info: Vec<(String, usize, u64)> = tasks
        .iter()
        .enumerate()
        .map(|(_i, task)| {
            (format!("{task}"), num_fewshot, seed) // task_name, num_fewshot, seed
        })
        .collect();

    let shared_results: Vec<Arc<RunningAverage>> = task_info
        .iter()
        .map(|_| Arc::new(RunningAverage::new()))
        .collect();

    // Create communication store for tensor parallelism
    #[cfg(feature = "parallelism")]
    let comm_id = CommunicatorId::from(tch::CStore::new());
    #[cfg(not(feature = "parallelism"))]
    let comm_id = CommunicatorId::none();

    let mut dp_handles: Vec<JoinHandle<Result<()>>> = vec![];

    // For each data parallel replica
    for dp_rank in 0..data_parallelism {
        let repo = repo.clone();
        let tokenizer = tokenizer.clone();
        let shared_results = shared_results.clone();
        let task_info = task_info.clone();
        let comm_id = comm_id.clone();

        let handle = std::thread::spawn(move || -> Result<()> {
            // Create communication store for this DP replica if TP > 1
            let comm_store = if tp_world_size > 1 {
                #[cfg(feature = "parallelism")]
                {
                    Some(comm_id)
                }
                #[cfg(not(feature = "parallelism"))]
                {
                    anyhow::bail!("Tensor parallelism set but parallelism feature not enabled")
                }
            } else {
                None
            };

            // Load all TP models for this DP replica (following train.rs pattern)
            let model_load_handles: Vec<JoinHandle<Result<Box<dyn psyche_modeling::CausalLM>>>> =
                (0..tp_world_size)
                    .map(|tp_rank| {
                        let rank = (dp_rank * tp_world_size) + tp_rank;
                        let device = Device::Cuda(rank);
                        let comm_store = comm_store.clone();
                        let repo = repo.clone();

                        std::thread::spawn(move || {
                            auto_model_for_causal_lm_from_pretrained(
                                repo,
                                Some(Kind::BFloat16),
                                None,
                                Some(device),
                                comm_store.map(|id| (id, tp_rank, tp_world_size)),
                                None,
                            )
                        })
                    })
                    .collect();

            // Wait for all TP models to load
            let models: Result<Vec<_>, _> = model_load_handles
                .into_iter()
                .map(|handle| handle.join().unwrap())
                .collect();
            let mut models = models?;

            // Run evaluation using the first model (rank 0 of TP group)
            // Other models participate automatically through tensor parallelism
            for (task_idx, (task_name, num_fewshot, seed)) in task_info.into_iter().enumerate() {
                let task_type = tasktype_from_name(&task_name)?;
                let task = Task::new(task_type, num_fewshot, seed + task_idx as u64);

                task.prepare(&tokenizer, None).run(
                    EvalTaskOptions {
                        model: models[0].as_mut(), // Use TP rank 0 model
                        skip_and_step_by: Some((dp_rank, data_parallelism)),
                        live_results: Some(shared_results[task_idx].clone()),
                        cancel: None,
                        limit: None,
                    },
                    !quiet,
                );
            }

            Ok(())
        });

        dp_handles.push(handle);
    }

    // Wait for all DP replicas to complete
    for dp_handle in dp_handles {
        dp_handle
            .join()
            .map_err(|_| anyhow::anyhow!("DP worker thread panicked"))??;
    }

    for ((task_name, _, _), running_avg) in task_info.into_iter().zip(shared_results) {
        let final_scores = running_avg
            .get_all_averages()
            .into_iter()
            .map(|(key, value)| (key, value.unwrap_or(0.0)))
            .collect::<HashMap<String, f64>>();
        println!("{}: {:?}", task_name, final_scores);
    }

    Ok(())
}
