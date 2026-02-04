use crate::{
    TaskType, load_dataset,
    traits::{Document, LogLikelihoodTask},
};
use anyhow::Result;
use psyche_data_provider::{Dataset, Row, RowAccessor, Split};
use std::{collections::HashMap, fmt::Display};

pub struct PIQA {
    train_dataset: Dataset,
    validation_dataset: Dataset,
}

impl PIQA {
    pub fn load() -> Result<TaskType> {
        let ret = Self {
            train_dataset: load_dataset("ybisk/piqa", None, Split::Train, None)?,
            validation_dataset: load_dataset("ybisk/piqa", None, Split::Validation, None)?,
        };
        Ok(TaskType::LogLikelihood(Box::new(ret)))
    }

    pub const fn name() -> &'static str {
        "PIQA"
    }

    fn row_to_document(dataset: &Dataset, row: Row) -> Document {
        let goal = row
            .get_string(dataset.get_column_id("goal").unwrap())
            .unwrap()
            .to_owned();

        let sol1 = row
            .get_string(dataset.get_column_id("sol1").unwrap())
            .unwrap()
            .to_owned();

        let sol2 = row
            .get_string(dataset.get_column_id("sol2").unwrap())
            .unwrap()
            .to_owned();

        let text = format!("Question: {goal}\nAnswer:");
        let choices = vec![sol1, sol2];

        let answer = row
            .get_long(dataset.get_column_id("label").unwrap())
            .unwrap() as usize;

        Document {
            text,
            choices,
            answer,
            category: None,
            cot_content: None,
            eval_name: PIQA::name().to_string(),
        }
    }
}

impl LogLikelihoodTask for PIQA {
    fn get_documents(&self) -> Vec<Document> {
        self.validation_dataset
            .iter()
            .map(|row| PIQA::row_to_document(&self.validation_dataset, row))
            .collect()
    }

    fn get_fewshot_documents(&self) -> HashMap<String, Vec<Document>> {
        let mut fewshot_documents = HashMap::new();
        let docs: Vec<Document> = self
            .train_dataset
            .iter()
            .map(|row| PIQA::row_to_document(&self.train_dataset, row))
            .collect();
        fewshot_documents.insert("default".to_string(), docs);
        fewshot_documents
    }
}

impl Display for PIQA {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", Self::name())
    }
}
