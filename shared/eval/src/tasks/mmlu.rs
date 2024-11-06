use crate::{
    load_dataset,
    traits::{Document, LogLikelihoodTask},
    TaskType, ASCII_UPPERCASE,
};
use anyhow::Result;
use psyche_data_provider::{Dataset, ListAccessor, Row, RowAccessor, Split};
use std::fmt::Display;

pub struct MMLU {
    test_dataset: Dataset,
    validation_dataset: Dataset,
}

impl MMLU {
    pub fn load() -> Result<TaskType> {
        let ret = Self {
            test_dataset: load_dataset(
                "hails/mmlu_no_train",
                Some("main".to_owned()),
                Split::Test,
                None,
            )?,
            validation_dataset: load_dataset(
                "hails/mmlu_no_train",
                Some("main".to_owned()),
                Split::Validation,
                None,
            )?,
        };
        Ok(TaskType::LogLikelihood(Box::new(ret)))
    }

    pub const fn name() -> &'static str {
        "MMLU"
    }

    fn row_to_document(dataset: &Dataset, row: Row) -> Document {
        let text = row
            .get_string(dataset.get_column_id("question").unwrap())
            .unwrap()
            .to_owned();
        let options = row
            .get_list(dataset.get_column_id("choices").unwrap())
            .unwrap();
        let options = (0..options.len())
            .map(|i| format!("{}. {}", ASCII_UPPERCASE[i], options.get_string(i).unwrap()))
            .collect::<Vec<_>>();
        let choices = (0..options.len())
            .map(|i| ASCII_UPPERCASE[i].to_owned())
            .collect::<Vec<_>>();
        let text = format!("{}\n{}\nAnswer: ", text, options.join("\n"));
        let answer = row
            .get_long(dataset.get_column_id("answer").unwrap())
            .unwrap() as usize;
        Document {
            text,
            choices,
            answer,
        }
    }
}

impl LogLikelihoodTask for MMLU {
    fn get_documents(&self) -> Vec<Document> {
        self.test_dataset
            .iter()
            .map(|row| MMLU::row_to_document(&self.test_dataset, row))
            .collect()
    }

    fn get_fewshot_documents(&self) -> Vec<Document> {
        self.validation_dataset
            .iter()
            .map(|row| MMLU::row_to_document(&self.validation_dataset, row))
            .collect()
    }
}

impl Display for MMLU {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", Self::name())
    }
}
