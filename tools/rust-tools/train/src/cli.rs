use clap::{Args, Parser, Subcommand, ValueEnum};
use psyche_coordinator::model::LLMArchitecture;
use psyche_modeling::AttentionImplementation;

#[derive(ValueEnum, Clone, Copy, Debug)]
pub enum AttnImpl {
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

#[derive(ValueEnum, Clone, Copy, Debug)]
#[value(rename_all = "verbatim")]
pub enum LLMArch {
    HfLlama,
    HfDeepseek,
    HfAuto,
    Torchtitan,
}

impl From<LLMArch> for LLMArchitecture {
    fn from(val: LLMArch) -> Self {
        match val {
            LLMArch::HfLlama => LLMArchitecture::HfLlama,
            LLMArch::HfDeepseek => LLMArchitecture::HfDeepseek,
            LLMArch::HfAuto => LLMArchitecture::HfAuto,
            LLMArch::Torchtitan => LLMArchitecture::Torchtitan,
        }
    }
}

#[derive(Parser, Debug)]
pub struct CliArgs {
    #[command(subcommand)]
    pub command: Option<Commands>,

    #[command(flatten)]
    pub run_args: FlatRunArgs,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// train using a state.toml config file
    Config {
        /// path to the state.toml config file
        config: String,

        #[command(flatten)]
        local: LocalArgs,
    },
    #[clap(hide = true)]
    PrintAllHelp {
        #[arg(long, required = true)]
        markdown: bool,
    },
}

/// runtime args that aren't fixed in the model config itself
#[derive(Args, Debug, Clone)]
pub struct LocalArgs {
    /// override the data_location from the config with a data.toml file.
    /// if the config's data_location is Server and this is not provided,
    /// we automatically search for one called `data.toml` next to the config file.
    #[arg(long)]
    pub data: Option<String>,

    #[arg(long, default_value_t = 8)]
    pub micro_batch: usize,

    #[arg(
        long,
        help = "Device(s) to use: auto, cpu, mps, cuda, cuda:N, cuda:X,Y,Z",
        default_value = "auto"
    )]
    pub device: String,

    #[arg(long, default_value_t = false)]
    pub grad_accum_in_fp32: bool,

    #[arg(long)]
    pub tensor_parallelism: Option<usize>,

    #[arg(long)]
    pub data_parallelism: Option<usize>,

    #[arg(long)]
    pub attn_implementation: Option<AttnImpl>,

    #[arg(long, default_value_t = 1)]
    pub start_step: u32,

    #[arg(long)]
    pub seed: Option<u32>,

    #[arg(long)]
    pub save_path: Option<String>,
}

/// Flat CLI args for running without a config file (local data only)
#[derive(Args, Debug, Clone)]
pub struct FlatRunArgs {
    #[arg(long, default_value = "emozilla/llama2-20m-init")]
    pub model: String,

    #[arg(long, default_value = "data")]
    pub data_path: String,

    #[arg(long, default_value_t = 2048)]
    pub sequence_length: usize,

    #[arg(long, default_value_t = 2)]
    pub token_size: usize,

    #[arg(long, default_value_t = 8)]
    pub micro_batch: usize,

    #[arg(long, default_value_t = 256)]
    pub total_batch: usize,

    #[arg(long, default_value_t = 0.9)]
    pub beta1: f32,

    #[arg(long, default_value_t = 0.95)]
    pub beta2: f32,

    #[arg(long, default_value_t = 0.1)]
    pub weight_decay: f32,

    #[arg(long, default_value_t = 1e-8)]
    pub eps: f32,

    #[arg(long, default_value_t = 4e-4)]
    pub learning_rate: f64,

    #[arg(long, default_value_t = 500)]
    pub warmup_steps: u32,

    #[arg(long, default_value_t = 25000)]
    pub total_steps: u32,

    #[arg(long, default_value_t = 1.0)]
    pub max_grad_norm: f32,

    #[arg(long)]
    pub tensor_parallelism: Option<usize>,

    #[arg(long)]
    pub data_parallelism: Option<usize>,

    #[arg(
        long,
        help = "Device(s) to use: auto, cpu, mps, cuda, cuda:N, cuda:X,Y,Z",
        default_value = "auto"
    )]
    pub device: String,

    #[arg(long, default_value_t = false)]
    pub grad_accum_in_fp32: bool,

    #[arg(long, default_value_t = 64)]
    pub compression_chunk: u16,

    #[arg(long, default_value_t = 4)]
    pub compression_topk: u16,

    #[arg(long, default_value_t = 0.999)]
    pub compression_decay: f32,

    #[arg(long, default_value_t = false)]
    pub distro: bool,

    #[arg(long, default_value_t = false)]
    pub distro_quantization: bool,

    #[arg(long)]
    pub attn_implementation: Option<AttnImpl>,

    #[arg(long, default_value_t = 1)]
    pub start_step: u32,

    #[arg(long, default_value = "HfAuto")]
    pub architecture: LLMArch,

    #[arg(long)]
    pub seed: Option<u32>,

    #[arg(long)]
    pub save_path: Option<String>,
}
