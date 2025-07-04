/**
    hf (pretrained=meta-llama/Meta-Llama-3.1-8B,dtype=bfloat16), gen_kwargs: (None), limit: None, num_fewshot: None, batch_size: 1
    |       Tasks        |Version|Filter|n-shot|Metric|   |Value |   |Stderr|
    |--------------------|------:|------|-----:|------|---|-----:|---|-----:|
    |leaderboard_mmlu_pro|    0.1|none  |     5|acc   |↑  |0.3268|±  |0.0043|

    MMLU Pro: {"acc": 0.32646278, "acc_norm": 0.32646278}
*/
use crate::{
    ASCII_UPPERCASE, TaskType, load_dataset,
    traits::{Document, LogLikelihoodTask},
};
use anyhow::Result;
use psyche_data_provider::{Dataset, ListAccessor, Row, RowAccessor, Split};
use std::fmt::Display;

pub struct MMLUPro {
    test_dataset: Dataset,
    validation_dataset: Dataset,
}

impl MMLUPro {
    pub fn load() -> Result<TaskType> {
        let ret = Self {
            test_dataset: load_dataset("TIGER-Lab/MMLU-Pro", None, Split::Test, None)?,
            validation_dataset: load_dataset("TIGER-Lab/MMLU-Pro", None, Split::Validation, None)?,
        };
        Ok(TaskType::LogLikelihood(Box::new(ret)))
    }

    pub const fn name() -> &'static str {
        "MMLU Pro"
    }

    fn row_to_document(dataset: &Dataset, row: Row) -> Document {
        let text = row
            .get_string(dataset.get_column_id("question").unwrap())
            .unwrap()
            .to_owned();
        let options = row
            .get_list(dataset.get_column_id("options").unwrap())
            .unwrap();
        let options = (0..options.len())
            .map(|i| format!("{}. {}", ASCII_UPPERCASE[i], options.get_string(i).unwrap()))
            .collect::<Vec<_>>();
        let choices = (0..options.len())
            .map(|i| ASCII_UPPERCASE[i].to_owned())
            .collect::<Vec<_>>();
        let text = format!("{}\n{}\nAnswer: ", text, options.join("\n"));
        let answer = row
            .get_string(dataset.get_column_id("answer").unwrap())
            .unwrap();
        let answer = ASCII_UPPERCASE.iter().position(|x| x == answer).unwrap();
        Document {
            text,
            choices,
            answer,
        }
    }
}

impl LogLikelihoodTask for MMLUPro {
    fn get_documents(&self) -> Vec<Document> {
        self.test_dataset
            .iter()
            .map(|row| MMLUPro::row_to_document(&self.test_dataset, row))
            .collect()
    }

    fn get_fewshot_documents(&self) -> Vec<Document> {
        self.validation_dataset
            .iter()
            .map(|row| MMLUPro::row_to_document(&self.validation_dataset, row))
            .collect()
    }
}

impl Display for MMLUPro {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", Self::name())
    }
}
