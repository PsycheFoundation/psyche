/**
   OpenBookQA is a question-answering dataset modeled after open book exams for assessing
   understanding of elementary-level science.


   hf (pretrained=NousResearch/Llama-2-7b-hf), gen_kwargs: (None), limit: None, num_fewshot: None, batch_size: 1
   |  Tasks   |Version|Filter|n-shot| Metric |   |Value|   |Stderr|
   |----------|------:|------|-----:|--------|---|----:|---|-----:|
   |openbookqa|      1|none  |     0|acc     |↑  |0.314|±  |0.0208|
   |          |       |none  |     0|acc_norm|↑  |0.442|±  |0.0222|

*/
use crate::{
    TaskType, load_dataset,
    traits::{Document, LogLikelihoodTask},
};
use anyhow::Result;
use psyche_data_provider::{Dataset, ListAccessor, Row, RowAccessor, Split};
use std::{collections::HashMap, fmt::Display};

pub struct OpenbookQA {
    test_dataset: Dataset,
    validation_dataset: Dataset,
}

impl OpenbookQA {
    pub fn load() -> Result<TaskType> {
        let ret = Self {
            test_dataset: load_dataset(
                "allenai/openbookqa",
                None,
                Split::Test,
                Some("main".to_string()),
            )?,
            validation_dataset: load_dataset(
                "allenai/openbookqa",
                None,
                Split::Validation,
                Some("main".to_string()),
            )?,
        };
        Ok(TaskType::LogLikelihood(Box::new(ret)))
    }

    pub const fn name() -> &'static str {
        "OpenBookQA"
    }

    fn row_to_document(dataset: &Dataset, row: Row) -> Document {
        let question_stem = row
            .get_string(dataset.get_column_id("question_stem").unwrap())
            .unwrap()
            .to_owned();
        let text = question_stem.to_string();

        let choices_group = row
            .get_group(dataset.get_column_id("choices").unwrap())
            .unwrap();
        let choice_texts = choices_group.get_list(0).unwrap();

        let choices = (0..choice_texts.len())
            .map(|i| choice_texts.get_string(i).unwrap().to_owned())
            .collect::<Vec<_>>();

        let answer_key = row
            .get_string(dataset.get_column_id("answerKey").unwrap())
            .unwrap();

        let answer = match answer_key.to_string().as_str() {
            "A" => 0,
            "B" => 1,
            "C" => 2,
            "D" => 3,
            _ => panic!("Invalid answer key"),
        };

        Document {
            text,
            choices,
            answer,
            category: None,
            cot_content: None,
            eval_name: OpenbookQA::name().to_string(),
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

    fn get_fewshot_documents(&self) -> HashMap<String, Vec<Document>> {
        let mut fewshot_documents = HashMap::new();
        let docs: Vec<Document> = self
            .validation_dataset
            .iter()
            .map(|row| OpenbookQA::row_to_document(&self.validation_dataset, row))
            .collect();
        fewshot_documents.insert("default".to_string(), docs);
        fewshot_documents
    }
}

impl Display for OpenbookQA {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", Self::name())
    }
}
