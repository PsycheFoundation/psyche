/**
    hf (pretrained=meta-llama/Meta-Llama-3.1-8B,dtype=bfloat16), gen_kwargs: (None), limit: None, num_fewshot: None, batch_size: 1
    |       Tasks        |Version|Filter|n-shot|Metric|   |Value |   |Stderr|
    |--------------------|------:|------|-----:|------|---|-----:|---|-----:|
    |leaderboard_mmlu_pro|    0.1|none  |     5|acc   |↑  |0.3268|±  |0.0043|

    MMLU Pro: {"acc": 0.32646278, "acc_norm": 0.32646278}
*/
use crate::{
    ASCII_UPPERCASE, TaskType, load_dataset,
    traits::{Document, GenerateUntilTask},
};
use anyhow::Result;
use psyche_data_provider::{Dataset, Row, RowAccessor, Split};
use std::{collections::HashMap, fmt::Display};

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
        Ok(TaskType::GenerateUntil(Box::new(ret)))
    }

    pub const fn name() -> &'static str {
        "MMLU Pro"
    }

    fn row_to_document(dataset: &Dataset, row: Row) -> Document {
        let text = row
            .get_string(dataset.get_column_id("question").unwrap())
            .unwrap()
            .to_owned();
        let choices: Vec<String> = row
            .get_list(dataset.get_column_id("options").unwrap())
            .unwrap()
            .elements()
            .iter()
            .map(|field| {
                let mut s = field.to_string();
                // Remove \" at the start and end of the String
                s.remove(0);
                s.pop();
                s
            })
            .collect();
        let answer = row
            .get_string(dataset.get_column_id("answer").unwrap())
            .unwrap();
        let answer = ASCII_UPPERCASE.iter().position(|x| x == answer).unwrap();
        let category = row
            .get_string(dataset.get_column_id("category").unwrap())
            .unwrap()
            .to_owned();
        let cot_content = row
            .get_string(dataset.get_column_id("cot_content").unwrap())
            .unwrap()
            .to_owned();

        Document {
            text,
            choices,
            answer,
            category: Some(category),
            cot_content: Some(cot_content),
            eval_name: MMLUPro::name().to_string(),
        }
    }
}

impl GenerateUntilTask for MMLUPro {
    fn get_documents(&self) -> Vec<Document> {
        self.test_dataset
            .iter()
            .map(|row| MMLUPro::row_to_document(&self.test_dataset, row))
            .collect()
    }

    fn get_fewshot_documents(&self) -> HashMap<String, Vec<Document>> {
        let mut fewshot_documents = HashMap::new();
        self.validation_dataset.iter().for_each(|row| {
            let doc = MMLUPro::row_to_document(&self.validation_dataset, row);
            let category = doc.category.as_ref().unwrap().clone();
            fewshot_documents
                .entry(category)
                .or_insert_with(Vec::new)
                .push(doc);
        });
        fewshot_documents
    }

    fn get_stop_string(&self) -> Vec<String> {
        vec!["Question:".to_string()]
    }

    fn get_answer_extraction_regex(&self) -> String {
        // Matches "answer is A", "answer is B", "answer is (F)" etc.
        r"answer is \(?([ABCDEFGHIJ])\)?".to_string()
    }
}

impl Display for MMLUPro {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", Self::name())
    }
}
