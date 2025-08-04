use anyhow::{bail, Error, Result};
use clap::Parser;
use psyche_data_provider::download_model_repo_sync;
use psyche_eval::dataset_from_name;
use psyche_modeling::{
    auto_model_for_causal_lm_from_pretrained, auto_tokenizer, CausalLM, CommunicatorId,
    LogitsProcessor, Sampling, TokenOutputStream,
};
use serde_json::json;
use std::{
    io::Write,
    path::PathBuf,
    sync::{Arc, Barrier},
};
use tch::{Device, Kind, Tensor};
use tokenizers::Tokenizer;

const DEFAULT_PROMPT: &str = r"
EDWARD:
I wonder how our princely father 'scaped,
Or whether he be 'scaped away or no
From Clifford's and Northumberland's pursuit:
Had he been ta'en, we should have heard the news;
Had he been slain, we should have heard the news;
Or had he 'scaped, methinks we should have heard
The happy tidings of his good escape.
How fares my brother? why is he so sad?

RICHARD:
I cannot joy, until I be resolved
Where our right valiant father is become.
I saw him in the battle range about;
And watch'd him how he singled Clifford forth.
Methought he bore him in the thickest troop
As doth a lion in a herd of neat;
Or as a bear, encompass'd round with dogs,
Who having pinch'd a few and made them cry,
The rest stand all aloof, and bark at him.
So fared our father with his enemies;
So fled his enemies my warlike father:
Methinks, 'tis prize enough to be his son.
See how the morning opes her golden gates,
And takes her farewell of the glorious sun!
How well resembles it the prime of youth,
Trimm'd like a younker prancing to his love!

EDWARD:
Dazzle mine eyes, or do I see three suns?

RICHARD:
Three glorious suns, each one a perfect sun;
Not separated with the racking clouds,
But sever'd in a pale clear-shining sky.
See, see! they join, embrace, and seem to kiss,
As if they vow'd some league inviolable:
Now are they but one lamp, one light, one sun.
In this the heaven figures some event.

EDWARD:
'Tis wondrous strange, the like yet never heard of.
I think it cites us, brother, to the field,
That we, the sons of brave Plantagenet,
Each one already blazing by our meeds,
Should notwithstanding join our lights together
And over-shine the earth as this the world.
Whate'er it bodes, henceforward will I bear
Upon my target three fair-shining suns.
";

#[derive(Parser, Debug, Clone)]
struct Args {
    #[arg(long, default_value = "NousResearch/Llama-2-7b-hf")]
    model: String,

    #[arg(long)]
    revision: Option<String>,

    #[arg(long, default_value_t = 0.6)]
    temperature: f64,

    #[arg(long)]
    top_p: Option<f64>,

    #[arg(long)]
    top_k: Option<usize>,

    #[arg(long)]
    max_tokens: Option<usize>,

    #[arg(long)]
    seed: Option<u64>,

    #[arg(long)]
    tensor_parallelism: Option<usize>,

    #[cfg(feature = "python")]
    #[clap(long)]
    python: bool,

    #[arg(long)]
    prompt: Option<String>,

    #[arg(long)]
    tasks: Option<String>,

    /// Eval index from where to start
    #[arg(long, default_value_t = 0)]
    eval_index: usize,

    /// How many evals in a task to run
    #[arg(long, default_value_t = 1)]
    eval_limit: usize,

    /// Return the output in JSON format
    #[arg(long, default_value_t = false)]
    json: bool,
}

fn inference(
    repo_files: Vec<PathBuf>,
    tensor_parallelism: Option<(CommunicatorId, usize, usize, Arc<Barrier>)>,
    args: Args,
    mut tokens: Vec<i64>,
    tokenizer: Tokenizer,
) -> Result<(String, String)> {
    let rank = tensor_parallelism
        .as_ref()
        .map(|(_, rank, _, _)| *rank)
        .unwrap_or(0);
    let device = Device::Cuda(rank);
    let seed = args.seed.unwrap_or(rand::random());
    let print_to_stdout = !args.json;

    #[cfg(feature = "python")]
    let python = args.python;
    #[cfg(not(feature = "python"))]
    let python = false;
    let mut model: Box<dyn CausalLM> = if python {
        #[cfg(feature = "python")]
        {
            if args.tensor_parallelism.is_some() {
                anyhow::bail!("Parallelism not supported for inference in python yet");
            }

            psyche_python_extension_impl::init_embedded_python();

            let source = psyche_modeling::PretrainedSource::RepoFiles(repo_files);
            Box::new(psyche_modeling::PythonCausalLM::new(
                "hf-auto", &source, device, None, None,
            )?) as Box<dyn CausalLM>
        }
        #[cfg(not(feature = "python"))]
        unreachable!();
    } else {
        auto_model_for_causal_lm_from_pretrained(
            repo_files,
            Some(Kind::BFloat16),
            None,
            tensor_parallelism.as_ref().map(|_| device),
            tensor_parallelism
                .as_ref()
                .map(|(id, rank, size, _)| (id.clone(), *rank, *size)),
            None,
        )?
    };

    let eos_token_ids = model.eos_token_ids();

    let mut logits_processor = {
        let temperature = args.temperature;
        let sampling = if temperature <= 0. {
            Sampling::ArgMax
        } else {
            match (args.top_k, args.top_p) {
                (None, None) => Sampling::All { temperature },
                (Some(k), None) => Sampling::TopK { k, temperature },
                (None, Some(p)) => Sampling::TopP { p, temperature },
                (Some(k), Some(p)) => Sampling::TopKThenTopP { k, p, temperature },
            }
        };
        LogitsProcessor::from_sampling(seed, sampling)
    };
    let mut tokenizer = TokenOutputStream::new(tokenizer);
    let mut token_generated = 0;
    let mut generated_text = String::new();
    let prompt_text = tokenizer
        .tokenizer()
        .decode(
            &tokens.iter().map(|&x| x as u32).collect::<Vec<u32>>(),
            false,
        )
        .unwrap();

    loop {
        if let Some(max_tokens) = args.max_tokens {
            if token_generated >= max_tokens {
                break;
            }
        }
        let input = Tensor::from_slice(&tokens).to(device).unsqueeze(0);
        if let Some((_, _, _, barrier)) = tensor_parallelism.as_ref() {
            barrier.wait();
        }
        let (logits, _) = model.forward(&input, None, Some(1), None);
        if let Some((_, _, _, barrier)) = tensor_parallelism.as_ref() {
            barrier.wait();
        }
        let logits = logits.squeeze();
        let next_token = logits_processor.sample(&logits)?;
        token_generated += 1;
        tokens.push(next_token as i64);

        if let Some(eos_token_ids) = &eos_token_ids {
            if eos_token_ids.contains(next_token as i64) {
                let token_text = tokenizer
                    .tokenizer()
                    .decode(&[next_token as u32], false)
                    .unwrap();
                generated_text.push_str(&token_text);
                if print_to_stdout && rank == 0 {
                    print!("{}", token_text);
                }
                break;
            };
        }

        if let Some(t) = tokenizer.next_token(next_token)? {
            generated_text.push_str(&t);
            if print_to_stdout && rank == 0 {
                print!("{t}");
                std::io::stdout().flush()?;
            }
        }
    }
    Ok((prompt_text, generated_text))
}

#[allow(unused_variables)]
fn prompt_inference(
    prompt: &str,
    model_repo_files: Vec<PathBuf>,
    args: Args,
) -> Result<Vec<(String, String)>> {
    let tokenizer = auto_tokenizer(&model_repo_files)?;
    let tokens = tokenizer
        .encode(prompt, true)
        .map_err(Error::msg)?
        .get_ids()
        .iter()
        .map(|x| *x as i64)
        .collect::<Vec<_>>();

    let result = match args.tensor_parallelism {
        Some(0) | Some(1) | None => {
            let (prompt, response) =
                inference(model_repo_files, None, args.clone(), tokens, tokenizer)?;
            vec![(prompt, response)]
        }
        Some(world_size) => {
            #[cfg(feature = "python")]
            let id = match from_python {
                true => CommunicatorId::torch_distributed("nccl", "tcp://127.0.0.1:23456"),
                #[cfg(feature = "parallelism")]
                false => tch::CStore::new().into(),
                #[cfg(not(feature = "parallelism"))]
                false => CommunicatorId::none(),
            };

            #[cfg(not(feature = "python"))]
            let id: CommunicatorId = {
                #[cfg(feature = "parallelism")]
                {
                    tch::CStore::new().into()
                }
                #[cfg(not(feature = "parallelism"))]
                {
                    CommunicatorId::none()
                }
            };

            let barrier = Arc::new(Barrier::new(world_size));
            let threads = (0..world_size)
                .map(|rank| {
                    let repo_files = model_repo_files.clone();
                    let args = args.clone();
                    let tokens = tokens.clone();
                    let tokenizer = tokenizer.clone();
                    let id = id.clone();
                    let barrier = barrier.clone();
                    std::thread::spawn(move || {
                        inference(
                            repo_files,
                            Some((id, rank, world_size, barrier)),
                            args,
                            tokens,
                            tokenizer,
                        )
                    })
                })
                .collect::<Vec<_>>();
            let mut results = Vec::new();
            for thread in threads {
                let (prompt, response) = thread.join().unwrap()?;
                results.push((prompt, response));
            }
            results
        }
    };

    Ok(result)
}

fn evals_inference(
    evals: &str,
    model_repo_files: Vec<PathBuf>,
    args: Args,
) -> Result<Vec<(String, String)>> {
    let eval_datasets: Vec<_> = evals
        .split(",")
        .map(|eval_name| dataset_from_name(eval_name))
        .collect::<Result<Vec<_>>>()?;

    let mut all_results = Vec::new();

    for eval_dataset in eval_datasets {
        if args.eval_index >= eval_dataset.len() {
            bail!(
                "Eval index {} is out of bounds for dataset with {} entries",
                args.eval_index,
                eval_dataset.len()
            );
        }
        for question_prompt in eval_dataset
            .iter()
            .skip(args.eval_index)
            .take(args.eval_limit)
        {
            if !args.json {
                println!("{:?}", question_prompt);
                println!();
            }
            let results =
                prompt_inference(&question_prompt, model_repo_files.clone(), args.clone())?;
            all_results.extend(results);
            if !args.json {
                println!("------------------------------------------------------------");
                println!();
            }
        }
    }

    Ok(all_results)
}

fn main() -> Result<()> {
    psyche_modeling::set_suggested_env_vars();

    let _no_grad = tch::no_grad_guard();
    let args = Args::parse();
    let repo_files = if std::fs::exists(args.model.clone()).unwrap_or_default() {
        std::fs::read_dir(args.model.clone())
            .unwrap()
            .map(|x| x.unwrap().path())
            .collect::<Vec<_>>()
    } else {
        download_model_repo_sync(&args.model.clone(), args.revision.clone(), None, None, true)?
    };

    let results = if let Some(evals) = args.clone().tasks {
        evals_inference(&evals, repo_files, args.clone())
    } else {
        let prompt = args.prompt.as_ref().map_or(DEFAULT_PROMPT, |p| p.as_str());
        prompt_inference(prompt, repo_files, args.clone())
    };

    let results = results?;
    if args.json {
        let json_output = results
            .into_iter()
            .map(|(prompt, response)| {
                json!({
                    "prompt": prompt,
                    "response": response
                })
            })
            .collect::<Vec<_>>();
        println!("{}", serde_json::to_string_pretty(&json_output)?);
    }

    Ok(())
}
