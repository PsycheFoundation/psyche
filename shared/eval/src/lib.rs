use anyhow::{bail, Result};
use psyche_data_provider::{Dataset, Row, Split};

mod harness;
mod tasks;
mod traits;

pub use harness::{EvalTaskOptions, PreparedTask, PreparedTaskResult, Task, TaskType};
pub use tasks::{
    arc::Arc, ArcChallenge, ArcEasy, BoolQ, Hellaswag, MMLUPro, OpenbookQA, MMLU, PIQA,
};
use traits::Document;

pub const ASCII_UPPERCASE: [&str; 26] = [
    "A", "B", "C", "D", "E", "F", "G", "H", "I", "J", "K", "L", "M", "N", "O", "P", "Q", "R", "S",
    "T", "U", "V", "W", "X", "Y", "Z",
];

pub const ALL_TASK_NAMES: [&str; 8] = [
    ArcChallenge::name(),
    ArcEasy::name(),
    BoolQ::name(),
    Hellaswag::name(),
    MMLUPro::name(),
    MMLU::name(),
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

fn load_eval_dataset<F>(
    repo_id: &str,
    revision: Option<String>,
    split: Split,
    subset: Option<String>,
    row_to_document: F,
) -> Result<Vec<String>>
where
    F: Fn(&Dataset, Row) -> Document,
{
    let eval_dataset = load_dataset(repo_id, revision, split, subset)?;
    let eval_dataset_text = eval_dataset
        .iter()
        .map(|row| row_to_document(&eval_dataset, row))
        .map(|doc| doc.text)
        .collect();

    Ok(eval_dataset_text)
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
        "hellaswag" => Hellaswag::load(),
        "mmlu_pro" => MMLUPro::load(),
        "mmlu" => MMLU::load(),
        "openbookqa" => OpenbookQA::load(),
        "piqa" => PIQA::load(),
        _ => bail!("Unknown task {name}"),
    }
}

pub fn dataset_from_name(eval_name: &str) -> Result<Vec<String>> {
    match eval_name
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect::<String>()
        .as_str()
    {
        "arc_easy" => load_eval_dataset(
            "allenai/ai2_arc",
            None,
            Split::Test,
            Some(String::from("ARC-Easy")),
            Arc::row_to_document,
        ),
        "arc_challenge" => load_eval_dataset(
            "allenai/ai2_arc",
            None,
            Split::Test,
            Some(String::from("ARC-Challenge")),
            Arc::row_to_document,
        ),
        "boolq" => load_eval_dataset(
            "aps/super_glue",
            None,
            Split::Validation,
            Some(String::from("boolq")),
            BoolQ::row_to_document,
        ),
        "hellaswag" => load_eval_dataset(
            "Rowan/hellaswag",
            None,
            Split::Validation,
            None,
            Hellaswag::row_to_document,
        ),
        "mmlu_pro" => load_eval_dataset(
            "TIGER-Lab/MMLU-Pro",
            None,
            Split::Test,
            None,
            MMLUPro::row_to_document,
        ),
        "mmlu" => load_eval_dataset(
            "cais/mmlu",
            None,
            Split::Test,
            Some("all".to_owned()),
            MMLU::row_to_document,
        ),
        "openbookqa" => load_eval_dataset(
            "allenai/openbookqa",
            None,
            Split::Test,
            Some("main".to_string()),
            OpenbookQA::row_to_document,
        ),
        "piqa" => load_eval_dataset(
            "ybisk/piqa",
            None,
            Split::Validation,
            None,
            PIQA::row_to_document,
        ),
        _ => bail!("Unknown task {eval_name}"),
    }
}
