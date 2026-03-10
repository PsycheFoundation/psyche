/**
       hf (pretrained=NousResearch/Llama-2-7b-hf,dtype=bfloat16), gen_kwargs: (None), limit: None, num_fewshot: None, batch_size: 8
       |Tasks|Version|Filter|n-shot|Metric|   |Value|   |Stderr|
       |-----|------:|------|-----:|------|---|----:|---|-----:|
       |boolq|      2|none  |     0|acc   |↑  |0.778|±  |0.0073|

       boolq: {"acc_norm": 0.7842367667338496, "acc": 0.7842367667338496}
*/
use crate::{
    TaskType, load_dataset,
    traits::{Document, LogLikelihoodTask},
};
use anyhow::Result;
use psyche_data_provider::{Dataset, Row, RowAccessor, Split};
use std::{collections::HashMap, fmt::Display};

pub struct BoolQ {
    test_dataset: Dataset,
    validation_dataset: Dataset,
}

impl BoolQ {
    pub fn load() -> Result<TaskType> {
        let ret = Self {
            test_dataset: load_dataset(
                "aps/super_glue",
                None,
                Split::Train,
                Some("boolq".to_string()),
            )?,
            validation_dataset: load_dataset(
                "aps/super_glue",
                None,
                Split::Validation,
                Some("boolq".to_string()),
            )?,
        };
        Ok(TaskType::LogLikelihood(Box::new(ret)))
    }

    pub const fn name() -> &'static str {
        "BoolQ"
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

        let choices = vec!["no".to_string(), "yes".to_string()];

        let text = format!("{passage}\nQuestion: {question}?\nAnswer:");

        let answer = row
            .get_long(dataset.get_column_id("label").unwrap())
            .unwrap() as usize;

        Document {
            text,
            choices,
            answer,
            category: None,
            cot_content: None,
            eval_name: BoolQ::name().to_string(),
        }
    }
}

impl LogLikelihoodTask for BoolQ {
    fn get_documents(&self) -> Vec<Document> {
        self.validation_dataset
            .iter()
            .map(|row| BoolQ::row_to_document(&self.validation_dataset, row))
            .collect()
    }

    fn get_fewshot_documents(&self) -> HashMap<String, Vec<Document>> {
        let mut fewshot_documents = HashMap::new();
        let docs: Vec<Document> = self
            .test_dataset
            .iter()
            .map(|row| BoolQ::row_to_document(&self.test_dataset, row))
            .collect();
        fewshot_documents.insert("default".to_string(), docs);
        fewshot_documents
    }
}

impl Display for BoolQ {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", Self::name())
    }
}
