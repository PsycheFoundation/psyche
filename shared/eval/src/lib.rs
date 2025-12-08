use anyhow::{Result, bail};
use psyche_data_provider::{Dataset, Split};

mod harness;
mod tasks;
mod traits;

pub use harness::{
    EvalTaskOptions, PROGRESS_BAR_TEMPLATE, PreparedTask, PreparedTaskResult, Task, TaskType,
    progress_bar_template_with_task,
};
pub use tasks::{
    ArcChallenge, ArcEasy, BoolQ, CEval, Hellaswag, MMLU, MMLUCF, MMLUPro, OpenbookQA, PIQA,
};

pub const ASCII_UPPERCASE: [&str; 26] = [
    "A", "B", "C", "D", "E", "F", "G", "H", "I", "J", "K", "L", "M", "N", "O", "P", "Q", "R", "S",
    "T", "U", "V", "W", "X", "Y", "Z",
];

pub const ALL_TASK_NAMES: [&str; 10] = [
    ArcChallenge::name(),
    ArcEasy::name(),
    BoolQ::name(),
    CEval::name(),
    Hellaswag::name(),
    MMLUPro::name(),
    MMLU::name(),
    MMLUCF::name(),
    OpenbookQA::name(),
    PIQA::name(),
];

pub fn load_dataset(
    repo_id: &str,
    revision: Option<String>,
    split: Split,
    subset: Option<String>,
) -> Result<Dataset> {
    let repo_files = psyche_data_provider::download_dataset_repo_sync(
        repo_id,
        Some(revision.unwrap_or("refs/convert/parquet".to_owned())),
        None,
        None,
        true,
    )?;
    Dataset::load_dataset(&repo_files, Some(split), subset)
}

pub fn tasktype_from_name(name: &str) -> Result<TaskType> {
    match name
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect::<String>()
        .as_str()
    {
        "arc_challenge" => ArcChallenge::load(),
        "arc_easy" => ArcEasy::load(),
        "boolq" => BoolQ::load(),
        "ceval_valid" => CEval::load(),
        "hellaswag" => Hellaswag::load(),
        "mmlu_pro" => MMLUPro::load(),
        "mmlu" => MMLU::load(),
        "mmlu_cf" => MMLUCF::load(),
        "openbookqa" => OpenbookQA::load(),
        "piqa" => PIQA::load(),
        _ => bail!("Unknown task {name}"),
    }
}
