use anyhow::Result;
use clap::Parser;
use indicatif::{ProgressBar, ProgressStyle};
use psyche_core::RunningAverage;
use psyche_data_provider::{download_model_from_gcs_sync, download_model_repo_sync};
use psyche_eval::{
    ALL_TASK_NAMES, EvalTaskOptions, Task, progress_bar_template_with_task, tasktype_from_name,
};
use psyche_modeling::{CausalLM, auto_model_for_causal_lm_from_pretrained, auto_tokenizer};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::thread::JoinHandle;
use tch::{Device, Kind};
use tokenizers::Tokenizer;
use tracing::Level;

#[derive(Parser, Debug, Clone)]
struct Args {
    #[arg(long, default_value = "NousResearch/Llama-2-7b-hf")]
    model: String,

    #[arg(long)]
    revision: Option<String>,

    #[arg(long)]
    hf_token: Option<String>,

    /// GCS bucket name (use instead of HuggingFace model)
    #[arg(long)]
    gcs_bucket: Option<String>,

    /// GCS prefix/directory within the bucket
    #[arg(long)]
    gcs_prefix: Option<String>,

    #[arg(long, default_value_t = ALL_TASK_NAMES.join(","))]
    tasks: String,

    #[arg(long)]
    num_fewshot: Option<usize>,

    #[arg(long, default_value_t = 42)]
    seed: u64,

    #[arg(long, default_value_t = false)]
    quiet: bool,

    #[arg(long)]
    limit: Option<usize>,

    #[arg(long, default_value_t = 1)]
    data_parallelism: usize,

    #[cfg(feature = "python")]
    #[clap(long)]
    python: bool,

    #[cfg(feature = "python")]
    #[clap(long, default_value = "HfAuto")]
    python_arch: String,
}

fn main() -> Result<()> {
    tracing_subscriber::fmt().with_max_level(Level::INFO).init();
    let args = Args::parse();
    let tasks: Result<Vec<Task>> = args
        .tasks
        .split(",")
        .map(|x| {
            tasktype_from_name(x).map(|y| {
                let num_fewshot = args.num_fewshot.unwrap_or(match x {
                    "mmlu_pro" => 5,
                    _ => 0,
                });
                Task::new(y, num_fewshot, args.seed)
            })
        })
        .collect();
    let tasks = tasks?;

    if !args.quiet {
        let limit_str = if let Some(limit) = args.limit {
            format!(", limit={limit}")
        } else {
            "".to_string()
        };
        let model_source = if let Some(ref bucket) = args.gcs_bucket {
            format!(
                "gs://{}/{}",
                bucket,
                args.gcs_prefix.as_deref().unwrap_or("")
            )
        } else {
            args.model.clone()
        };
        println!(
            "\n\n Running tasks with model {}, seed: {}, DP={}{}",
            model_source, args.seed, args.data_parallelism, limit_str
        );
        for task in &tasks {
            println!("  - {}: {} few-shot examples", task, task.num_fewshot);
        }
        println!("\n\n");
    }

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

    let repo = if let Some(ref bucket) = args.gcs_bucket {
        download_model_from_gcs_sync(bucket, args.gcs_prefix.as_deref())?
    } else {
        download_model_repo_sync(&args.model, args.revision, None, args.hf_token, true)?
    };
    let tokenizer = auto_tokenizer(&repo)?;

    let (python, python_arch) = {
        #[cfg(feature = "python")]
        {
            (args.python, args.python_arch)
        }

        #[cfg(not(feature = "python"))]
        {
            (false, String::new())
        }
    };

    // Case with no parallelism is the same code just with DP=1
    run_data_parallel(
        tasks,
        repo,
        tokenizer,
        args.data_parallelism,
        args.quiet,
        args.seed,
        args.limit,
        python,
        python_arch,
    )?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn run_data_parallel(
    tasks: Vec<Task>,
    repo: Vec<PathBuf>,
    tokenizer: Tokenizer,
    data_parallelism: usize,
    quiet: bool,
    seed: u64,
    limit: Option<usize>,
    python: bool,
    python_arch: String,
) -> Result<()> {
    let task_info: Vec<(String, usize, u64)> = tasks
        .iter()
        .map(|task| {
            (format!("{task}"), task.num_fewshot, seed) // task_name, num_fewshot, seed
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

    let shared_progress_bars: Vec<Option<Arc<ProgressBar>>> = if !quiet && threads > 1 {
        task_info
            .iter()
            .map(|(task_name, num_fewshot, _)| {
                // Get the prepared task to determine total number of items
                let task_type = tasktype_from_name(task_name).ok()?;
                let task = Task::new(task_type, *num_fewshot, seed);
                let prepared_task = task.prepare(&tokenizer, None);

                println!("Running {} with {} parallel threads", task_name, threads);
                let pbar = ProgressBar::new(prepared_task.num as u64);
                pbar.set_style(
                    ProgressStyle::default_bar()
                        .template(&progress_bar_template_with_task(task_name))
                        .unwrap()
                        .progress_chars("#>-"),
                );
                Some(Arc::new(pbar))
            })
            .collect()
    } else {
        task_info.iter().map(|_| None).collect()
    };

    let mut gpu_handles: Vec<JoinHandle<Result<()>>> = vec![];
    for gpu_id in 0..threads {
        let repo = repo.clone();
        let tokenizer = tokenizer.clone();
        let shared_results = shared_results.clone();
        let shared_progress_bars = shared_progress_bars.clone();
        let task_info = task_info.clone();
        #[allow(unused)]
        let python_arch = python_arch.clone();

        let handle = std::thread::spawn(move || -> Result<()> {
            let device = if data_parallelism == 1 {
                Device::cuda_if_available()
            } else {
                Device::Cuda(gpu_id)
            };

            let mut model: Box<dyn CausalLM> = if python {
                #[cfg(feature = "python")]
                {
                    psyche_python_extension_impl::init_embedded_python()?;

                    Box::new(psyche_modeling::PythonDistributedCausalLM::new(
                        python_arch,
                        psyche_modeling::PretrainedSource::RepoFiles(repo),
                        device,
                        psyche_modeling::AttentionImplementation::default(),
                        psyche_modeling::ParallelismConfig {
                            dp: data_parallelism,
                            tp: 1,
                        },
                        None,
                        None,
                        Some(data_parallelism as i64),
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
                        skip_and_step_by: Some((gpu_id, threads)),
                        live_results: Some(shared_results[task_idx].clone()),
                        cancel: None,
                        limit,
                        shared_progress_bar: shared_progress_bars[task_idx].clone(),
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
