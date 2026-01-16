use crate::{TaskType, load_dataset, traits::Document, traits::LogLikelihoodTask};
use anyhow::Result;
use psyche_data_provider::{Dataset, Field, Row, RowAccessor, Split};
use std::{collections::HashMap, fmt::Display};

struct Arc {
    test_split: Dataset,
    train_dataset: Dataset,
    name: String,
}

pub struct ArcEasy {
    task: Arc,
}

pub struct ArcChallenge {
    task: Arc,
}

fn field_to_string(field: &Field) -> String {
    match field {
        Field::Str(str) => str.to_owned(),
        _ => panic!("Expected string"),
    }
}

impl Arc {
    pub fn load(subset: &str) -> Result<TaskType> {
        let ret = Self {
            test_split: load_dataset(
                "allenai/ai2_arc",
                None,
                Split::Test,
                Some(subset.to_string()),
            )?,
            train_dataset: load_dataset(
                "allenai/ai2_arc",
                None,
                Split::Train,
                Some(subset.to_string()),
            )?,
            name: subset.to_string(),
        };
        Ok(TaskType::LogLikelihood(Box::new(ret)))
    }

    fn row_to_document(dataset: &Dataset, row: Row, name: &str) -> Document {
        let text = row
            .get_string(dataset.get_column_id("question").unwrap())
            .unwrap()
            .to_owned();
        let choices_and_labels = row
            .get_group(dataset.get_column_id("choices").unwrap())
            .unwrap();
        let choices = choices_and_labels.get_list(0).unwrap();
        let labels = choices_and_labels.get_list(1).unwrap();
        let text = format!("Question: {text}\nAnswer:");
        let answer = row
            .get_string(dataset.get_column_id("answerKey").unwrap())
            .unwrap();
        let choices = choices
            .elements()
            .iter()
            .map(field_to_string)
            .collect::<Vec<_>>();
        let answer = labels
            .elements()
            .iter()
            .position(|x| field_to_string(x) == *answer)
            .unwrap();
        Document {
            text,
            choices,
            answer,
            category: None,
            cot_content: None,
            eval_name: name.to_string(),
        }
    }
}

impl LogLikelihoodTask for Arc {
    fn get_documents(&self) -> Vec<Document> {
        self.test_split
            .iter()
            .map(|row| Arc::row_to_document(&self.test_split, row, &self.name))
            .collect()
    }

    fn get_fewshot_documents(&self) -> HashMap<String, Vec<Document>> {
        let mut fewshot_documents = HashMap::new();
        let docs: Vec<Document> = self
            .train_dataset
            .iter()
            .map(|row| Arc::row_to_document(&self.train_dataset, row, &self.name))
            .collect();
        fewshot_documents.insert("default".to_string(), docs);
        fewshot_documents
    }
}

impl Display for Arc {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name)
    }
}

impl ArcEasy {
    pub fn load() -> Result<TaskType> {
        Arc::load(Self::name())
    }

    pub const fn name() -> &'static str {
        "ARC-Easy"
    }
}

impl LogLikelihoodTask for ArcEasy {
    fn get_documents(&self) -> Vec<Document> {
        self.task.get_documents()
    }

    fn get_fewshot_documents(&self) -> HashMap<String, Vec<Document>> {
        self.task.get_fewshot_documents()
    }
}

impl Display for ArcEasy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.task)
    }
}

impl ArcChallenge {
    pub fn load() -> Result<TaskType> {
        Arc::load(Self::name())
    }

    pub const fn name() -> &'static str {
        "ARC-Challenge"
    }
}

impl LogLikelihoodTask for ArcChallenge {
    fn get_documents(&self) -> Vec<Document> {
        self.task.get_documents()
    }

    fn get_fewshot_documents(&self) -> HashMap<String, Vec<Document>> {
        self.task.get_fewshot_documents()
    }
}

impl Display for ArcChallenge {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.task)
    }
}
