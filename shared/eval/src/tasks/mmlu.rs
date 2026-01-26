use crate::{
    ASCII_UPPERCASE, TaskType, load_dataset,
    traits::{Document, LogLikelihoodTask},
};
use anyhow::Result;
use psyche_data_provider::{Dataset, ListAccessor, Row, RowAccessor, Split};
use std::{collections::HashMap, fmt::Display};

pub struct MMLU {
    test_dataset: Dataset,
    validation_dataset: Dataset,
}

impl MMLU {
    pub fn load() -> Result<TaskType> {
        let ret = Self {
            test_dataset: load_dataset("cais/mmlu", None, Split::Test, Some("all".to_owned()))?,
            validation_dataset: load_dataset(
                "cais/mmlu",
                None,
                Split::Dev,
                Some("all".to_owned()),
            )?,
        };
        Ok(TaskType::LogLikelihood(Box::new(ret)))
    }

    pub const fn name() -> &'static str {
        "MMLU"
    }

    fn row_to_document(dataset: &Dataset, row: Row) -> Document {
        let subject = row
            .get_string(dataset.get_column_id("subject").unwrap())
            .unwrap()
            .replace("_", " ");
        let question = row
            .get_string(dataset.get_column_id("question").unwrap())
            .unwrap()
            .trim_start()
            .trim_end()
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
        let text = format!("{}\n{}\nAnswer:", question, options.join("\n"));
        let answer = row
            .get_long(dataset.get_column_id("answer").unwrap())
            .unwrap() as usize;

        Document {
            text,
            choices,
            answer,
            category: Some(subject),
            cot_content: None,
            eval_name: MMLU::name().to_string(),
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

    fn get_fewshot_documents(&self) -> HashMap<String, Vec<Document>> {
        let mut fewshot_documents = HashMap::new();
        self.validation_dataset.iter().for_each(|row| {
            let doc = MMLU::row_to_document(&self.validation_dataset, row);
            if let Some(category) = &doc.category {
                fewshot_documents
                    .entry(category.clone())
                    .or_insert_with(Vec::new)
                    .push(doc);
            }
        });
        fewshot_documents
    }

    fn get_preamble(&self, category: &str) -> String {
        format!(
            "The following are multiple choice questions (with answers) about {}.\n\n",
            category
        )
    }
}

impl Display for MMLU {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", Self::name())
    }
}
