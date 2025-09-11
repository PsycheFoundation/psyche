use anyhow::{Error, Result};
use clap::{Parser, ValueEnum};
use psyche_data_provider::download_model_repo_sync;
use psyche_modeling::{
    AttentionImplementation, CausalLM, CommunicatorId, LogitsProcessor, Sampling,
    TokenOutputStream, auto_model_for_causal_lm_from_pretrained, auto_tokenizer,
};
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

#[derive(ValueEnum, Clone, Debug)]
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

    #[arg(long)]
    attn_implementation: Option<AttnImpl>,

    #[cfg(feature = "python")]
    #[clap(long)]
    python: bool,

    prompt: Option<String>,
}

fn inference(
    repo_files: Vec<PathBuf>,
    tensor_parallelism: Option<(CommunicatorId, usize, usize, Arc<Barrier>)>,
    args: Args,
    seed: u64,
    mut tokens: Vec<i64>,
    tokenizer: Tokenizer,
) -> Result<()> {
    let rank = tensor_parallelism
        .as_ref()
        .map(|(_, rank, _, _)| *rank)
        .unwrap_or(0);
    let device = Device::Cuda(rank);

    #[cfg(feature = "python")]
    let python = args.python;
    #[cfg(not(feature = "python"))]
    let python = false;
    let mut model: Box<dyn CausalLM> = if python {
        #[cfg(feature = "python")]
        {
            let tp = args.tensor_parallelism.unwrap_or(1);

            psyche_python_extension_impl::init_embedded_python();

            let source = psyche_modeling::PretrainedSource::RepoFiles(repo_files);
            if tp == 1 {
                Box::new(psyche_modeling::PythonCausalLM::new(
                    "hf-auto", &source, device, None, None,
                )?) as Box<dyn CausalLM>
            } else {
                tracing::info!("Faking TP with FSDP");
                Box::new(psyche_modeling::PythonDistributedCausalLM::new(
                    "hf-auto".to_string(),
                    source,
                    device,
                    psyche_modeling::ParallelismConfig { dp: tp, tp: 1 },
                    None,
                )?) as Box<dyn CausalLM>
            }
        }
        #[cfg(not(feature = "python"))]
        unreachable!();
    } else {
        auto_model_for_causal_lm_from_pretrained(
            repo_files,
            Some(Kind::BFloat16),
            args.attn_implementation.map(|x| x.into()),
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
    loop {
        if let Some(max_tokens) = args.max_tokens {
            if max_tokens >= token_generated {
                break;
            }
        }
        let input = Tensor::from_slice(&tokens).to(device).unsqueeze(0);
        if let Some((_, _, _, barrier)) = tensor_parallelism.as_ref() {
            barrier.wait();
        }
        let (logits, _) = model.forward(&input, None, None, None, Some(1), None);
        if let Some((_, _, _, barrier)) = tensor_parallelism.as_ref() {
            barrier.wait();
        }
        let logits = logits.squeeze();
        let next_token = logits_processor.sample(&logits)?;
        token_generated += 1;
        tokens.push(next_token as i64);

        if let Some(eos_token_ids) = &eos_token_ids {
            if eos_token_ids.contains(next_token as i64) {
                if rank == 0 {
                    println!(
                        "{}",
                        tokenizer.tokenizer().decode(&[next_token], false).unwrap()
                    );
                }
                break;
            };
        }

        if let Some(t) = tokenizer.next_token(next_token)? {
            if rank == 0 {
                print!("{t}");
                std::io::stdout().flush()?;
            }
        }
    }
    Ok(())
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
        download_model_repo_sync(
            &args.model.clone(),
            args.revision.clone(),
            None,
            std::env::var("HF_TOKEN").ok(),
            true,
        )?
    };
    let tokenizer = auto_tokenizer(&repo_files)?;

    let prompt = args.prompt.as_ref().map_or(DEFAULT_PROMPT, |p| p.as_str());
    let tokens = tokenizer
        .encode(prompt, true)
        .map_err(Error::msg)?
        .get_ids()
        .iter()
        .map(|x| *x as i64)
        .collect::<Vec<_>>();
    let seed = args.seed.unwrap_or(rand::random());
    match args.tensor_parallelism {
        Some(0) | Some(1) | None => inference(repo_files, None, args, seed, tokens, tokenizer)?,
        Some(world_size) => {
            #[cfg(feature = "python")]
            let id = match args.python {
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
                    let repo_files = repo_files.clone();
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
                            seed,
                            tokens,
                            tokenizer,
                        )
                    })
                })
                .collect::<Vec<_>>();
            for thread in threads {
                thread.join().unwrap()?;
            }
        }
    }
    Ok(())
}
