use anyhow::Result;
use clap::Parser;
use psyche_data_provider::download_model_repo_sync;
use psyche_eval::{tasktype_from_name, EvalTaskOptions, Task, ALL_TASK_NAMES};
use psyche_modeling::{auto_model_for_causal_lm_from_pretrained, auto_tokenizer};
use tch::{Device, Kind};

#[derive(Parser, Debug, Clone)]
struct Args {
    #[arg(long, default_value = "NousResearch/Llama-2-7b-hf")]
    model: String,

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
}

fn main() -> Result<()> {
    let args = Args::parse();
    let tasks: Result<Vec<Task>> = args
        .tasks
        .split(",")
        .map(|x| tasktype_from_name(x).map(|y| Task::new(y, args.num_fewshot, args.seed)))
        .collect();
    let tasks = tasks?;
    let repo = download_model_repo_sync(&args.model, None, None, args.hf_token, true)?;
    let tokenizer = auto_tokenizer(&repo)?;
    let mut model = auto_model_for_causal_lm_from_pretrained(
        repo,
        Some(Kind::BFloat16),
        None,
        Some(Device::cuda_if_available()),
        None,
        None,
    )?;
    let bos_token_id = model.bos_token_id();
    for task in tasks {
        let name = format!("{task}");
        let result = task.prepare(&tokenizer, bos_token_id, None).run(
            EvalTaskOptions {
                model: model.as_mut(),
                skip_and_step_by: None,
                live_results: None,
                cancel: None,
                limit: None,
                loop_if_empty: false,
            },
            !args.quiet,
        );

        println!("{}: {:?}", name, result.scores);
    }
    Ok(())
}
