use anyhow::Result;
use clap::Parser;
use psyche_core::RunningAverage;
use psyche_data_provider::download_model_repo_sync;
use psyche_eval::{ALL_TASK_NAMES, EvalTaskOptions, Task, tasktype_from_name};
use psyche_modeling::{CausalLM, auto_model_for_causal_lm_from_pretrained, auto_tokenizer};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
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

    if args.data_parallelism > 1 {
        #[cfg(feature = "parallelism")]
        {
            if !tch::utils::has_cuda() {
                anyhow::bail!("CUDA not available but data parallelism requested");
            }
            let available_gpus = tch::Cuda::device_count() as usize;
            if available_gpus < args.data_parallelism {
                anyhow::bail!(
                    "Requested {} GPUs but only {} available",
                    args.data_parallelism,
                    available_gpus
                );
            }
        }
        #[cfg(not(feature = "parallelism"))]
        {
            anyhow::bail!(
                "Data parallelism > 1 requested but 'parallelism' feature not enabled. Use --features parallelism"
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

    // Case with no parallelism is the same code just with DP=1
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
    Ok(())
}

fn run_data_parallel(
    tasks: Vec<Task>,
    repo: Vec<PathBuf>,
    tokenizer: Tokenizer,
    mut data_parallelism: usize,
    quiet: bool,
    num_fewshot: usize,
    seed: u64,
    python: bool,
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

    let mut batch_size = 1;
    if python && data_parallelism > 1 {
        batch_size = data_parallelism;
        data_parallelism = 1;
    }

    let mut gpu_handles: Vec<JoinHandle<Result<()>>> = vec![];
    for gpu_id in 0..data_parallelism {
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
                        psyche_modeling::ParallelismConfig {
                            dp: batch_size,
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

                let _ = task.prepare(&tokenizer, None).run(
                    EvalTaskOptions {
                        model: model.as_mut(),
                        skip_and_step_by: Some((gpu_id, data_parallelism)),
                        live_results: Some(shared_results[task_idx].clone()),
                        cancel: None,
                        limit: None,
                        batch_size,
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
        println!("{}: {:?}", task_name, final_scores);
    }

    Ok(())
}
