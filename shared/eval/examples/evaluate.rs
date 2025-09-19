use anyhow::Result;
use clap::Parser;
use psyche_core::RunningAverage;
use psyche_data_provider::download_model_repo_sync;
use psyche_eval::{ALL_TASK_NAMES, EvalTaskOptions, Task, tasktype_from_name};
use psyche_modeling::{
    TokenizerConfig, auto_model_for_causal_lm_from_pretrained, auto_tokenizer,
    auto_tokenizer_config,
};
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
}

fn main() -> Result<()> {
    let args = Args::parse();
    let tasks: Result<Vec<Task>> = args
        .tasks
        .split(",")
        .map(|x| {
            tasktype_from_name(x).map(|y| {
                let num_fewshot = args.num_fewshot.unwrap_or_else(|| match x {
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
        println!(
            "Running tasks with model {}, seed: {}, DP={}{}",
            args.model, args.seed, args.data_parallelism, limit_str
        );
        for task in &tasks {
            println!("  - {}: {} few-shot examples", task, task.num_fewshot);
        }
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

    let repo = download_model_repo_sync(
        &args.model,
        args.revision.clone(),
        None,
        args.hf_token,
        true,
    )?;
    let tokenizer_config = auto_tokenizer_config(&repo)?;
    let tokenizer = &tokenizer_config.tokenizer;

    // Log tokenizer information for debugging
    if !args.quiet {
        println!("\n=== Tokenizer Information ===");

        // Find which tokenizer file was loaded
        if let Some(tokenizer_path) = repo.iter().find(|x| x.ends_with("tokenizer.json")) {
            println!("Tokenizer loaded from: {}", tokenizer_path.display());
        }

        // Log vocabulary size
        let vocab_size = tokenizer.get_vocab_size(false);
        println!("Tokenizer vocab size: {}", vocab_size);

        // Log BOS token configuration
        if let Some(ref bos_token) = tokenizer_config.bos_token {
            println!("BOS token from config: {}", bos_token);
            if let Some(bos_id) = tokenizer.token_to_id(bos_token) {
                println!("BOS token ID: {}", bos_id);
            }
        } else {
            println!("No BOS token found in config");
        }
        println!("add_bos_token setting: {}", tokenizer_config.add_bos_token);

        // Removed general tokenization strategy comparison to focus only on target question debugging
    }

    // Log model information for debugging
    if !args.quiet {
        println!("=== Model Information ===");
        println!("Model: {}", args.model);
        if let Some(ref revision) = args.revision {
            println!("Revision: {}", revision);
        }
        println!("=========================\n");
    }

    // Case with no parallelism is the same code just with DP=1
    run_data_parallel(
        tasks,
        repo,
        tokenizer_config,
        args.data_parallelism,
        args.quiet,
        args.seed,
        args.limit,
    )?;
    Ok(())
}

fn run_data_parallel(
    tasks: Vec<Task>,
    repo: Vec<PathBuf>,
    tokenizer_config: TokenizerConfig,
    data_parallelism: usize,
    quiet: bool,
    seed: u64,
    limit: Option<usize>,
) -> Result<()> {
    let task_info: Vec<(String, usize, u64)> = tasks
        .iter()
        .enumerate()
        .map(|(_i, task)| {
            (format!("{task}"), task.num_fewshot, seed) // task_name, num_fewshot, seed
        })
        .collect();

    let shared_results: Vec<Arc<RunningAverage>> = task_info
        .iter()
        .map(|_| Arc::new(RunningAverage::new()))
        .collect();

    let mut gpu_handles: Vec<JoinHandle<Result<()>>> = vec![];
    for gpu_id in 0..data_parallelism {
        let repo = repo.clone();
        let tokenizer_config = tokenizer_config.clone();
        let shared_results = shared_results.clone();
        let task_info = task_info.clone();

        let handle = std::thread::spawn(move || -> Result<()> {
            let device = if data_parallelism == 1 {
                Device::cuda_if_available()
            } else {
                Device::Cuda(gpu_id)
            };

            let mut model = auto_model_for_causal_lm_from_pretrained(
                repo,
                Some(Kind::BFloat16),
                None,
                Some(device),
                None,
                None,
            )?;

            for (task_idx, (task_name, num_fewshot, seed)) in task_info.into_iter().enumerate() {
                let task_type = tasktype_from_name(&task_name)?;
                let task = Task::new(task_type, num_fewshot, seed + task_idx as u64);

                let _result = task.prepare(&tokenizer_config.tokenizer, None).run(
                    EvalTaskOptions {
                        model: model.as_mut(),
                        skip_and_step_by: Some((gpu_id, data_parallelism)),
                        live_results: Some(shared_results[task_idx].clone()),
                        cancel: None,
                        limit: limit,
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
