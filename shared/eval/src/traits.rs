use std::{collections::HashMap, fmt::Display};

#[derive(Clone)]
pub struct Document {
    pub text: String,
    pub choices: Vec<String>,
    pub answer: usize,
    pub category: Option<String>,
    pub cot_content: Option<String>,
}

pub trait LogLikelihoodTask: Send + Display {
    fn get_documents(&self) -> Vec<Document>;
    fn get_fewshot_documents(&self) -> HashMap<String, Vec<Document>>;
}

pub trait GenerateUntilTask: Send + Display {
    fn get_documents(&self) -> Vec<Document>;
    fn get_fewshot_documents(&self) -> HashMap<String, Vec<Document>>;
}
