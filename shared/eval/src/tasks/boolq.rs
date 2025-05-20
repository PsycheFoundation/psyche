use crate::{
    load_dataset,
    traits::{Document, LogLikelihoodTask},
    TaskType,
};
use anyhow::Result;
use psyche_data_provider::{Dataset, Row, RowAccessor, Split};
use std::fmt::Display;

pub struct BoolQ {
    test_dataset: Dataset,
    validation_dataset: Dataset,
}

impl BoolQ {
    pub fn load() -> Result<TaskType> {
        let ret = Self {
            test_dataset: load_dataset("google/boolq", None, Split::Train, None)?,
            validation_dataset: load_dataset("google/boolq", None, Split::Validation, None)?,
        };
        Ok(TaskType::LogLikelihood(Box::new(ret)))
    }

    pub const fn name() -> &'static str {
        "bool_q"
    }

    fn row_to_document(dataset: &Dataset, row: Row) -> Document {
        let question = row
            .get_string(dataset.get_column_id("question").unwrap())
            .unwrap()
            .to_owned();

        let passage = row
            .get_string(dataset.get_column_id("passage").unwrap())
            .unwrap()
            .to_owned();

        let choices = vec!["true".to_string(), "false".to_string()];

        let text = format!("Passage: {}\nQuestion: {}\nAnswer: ", passage, question);

        let answer = row
            .get_bool(dataset.get_column_id("answer").unwrap())
            .unwrap();

        let answer = choices
            .iter()
            .position(|choice_str| choice_str == &answer.to_string())
            .unwrap();

        Document {
            text,
            choices,
            answer,
        }
    }
}

impl LogLikelihoodTask for BoolQ {
    fn get_documents(&self) -> Vec<Document> {
        self.test_dataset
            .iter()
            .map(|row| BoolQ::row_to_document(&self.test_dataset, row))
            .collect()
    }

    fn get_fewshot_documents(&self) -> Vec<Document> {
        self.validation_dataset
            .iter()
            .map(|row| BoolQ::row_to_document(&self.validation_dataset, row))
            .collect()
    }
}

impl Display for BoolQ {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", Self::name())
    }
}
