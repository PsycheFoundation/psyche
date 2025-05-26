use crate::{
    load_dataset,
    traits::{Document, LogLikelihoodTask},
    TaskType,
};
use anyhow::Result;
use psyche_data_provider::{Dataset, ListAccessor, Row, RowAccessor, Split};
use std::fmt::Display;

/**
   OpenBookQA is a question-answering dataset modeled after open book exams for assessing
   understanding of elementary-level science.

*/
pub struct OpenbookQA {
    test_dataset: Dataset,
    validation_dataset: Dataset,
}

impl OpenbookQA {
    pub fn load() -> Result<TaskType> {
        let ret = Self {
            test_dataset: load_dataset("allenai/openbookqa", None, Split::Test, None)?,
            validation_dataset: load_dataset("allenai/openbookqa", None, Split::Validation, None)?,
        };
        Ok(TaskType::LogLikelihood(Box::new(ret)))
    }

    pub const fn name() -> &'static str {
        "OpenbookQA"
    }

    fn row_to_document(dataset: &Dataset, row: Row) -> Document {
        let question_stem = row
            .get_string(dataset.get_column_id("question_stem").unwrap())
            .unwrap()
            .to_owned();

        let choices_group = row
            .get_group(dataset.get_column_id("choices").unwrap())
            .unwrap();
        let choice_texts = choices_group.get_list(0).unwrap();
        let choice_labels = choices_group.get_list(1).unwrap();

        let options = (0..choice_texts.len())
            .map(|i| {
                format!(
                    "{}. {}",
                    choice_labels.get_string(i).unwrap(),
                    choice_texts.get_string(i).unwrap()
                )
            })
            .collect::<Vec<_>>();

        let choices = (0..choice_labels.len())
            .map(|i| choice_labels.get_string(i).unwrap().to_owned())
            .collect::<Vec<_>>();

        let text = format!("{question_stem}");

        let answer_key = row
            .get_string(dataset.get_column_id("answerKey").unwrap())
            .unwrap();

        let answer = choices
            .iter()
            .position(|choice| choice == answer_key)
            .unwrap();

        Document {
            text,
            choices,
            answer,
        }
    }
}

impl LogLikelihoodTask for OpenbookQA {
    fn get_documents(&self) -> Vec<Document> {
        self.test_dataset
            .iter()
            .map(|row| OpenbookQA::row_to_document(&self.test_dataset, row))
            .collect()
    }

    fn get_fewshot_documents(&self) -> Vec<Document> {
        self.validation_dataset
            .iter()
            .map(|row| OpenbookQA::row_to_document(&self.validation_dataset, row))
            .collect()
    }
}

impl Display for OpenbookQA {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", Self::name())
    }
}
