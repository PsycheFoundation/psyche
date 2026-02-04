use std::{collections::HashMap, fmt::Display};

#[derive(Clone)]
pub struct Document {
    pub text: String,
    pub choices: Vec<String>,
    pub answer: usize,
    pub category: Option<String>,
    pub cot_content: Option<String>,
    pub eval_name: String,
}

pub trait LogLikelihoodTask: Send + Display {
    fn get_documents(&self) -> Vec<Document>;
    fn get_fewshot_documents(&self) -> HashMap<String, Vec<Document>>;
    fn get_preamble(&self, _category: &str) -> String {
        String::new()
    }
}

pub trait GenerateUntilTask: Send + Display {
    fn get_documents(&self) -> Vec<Document>;
    fn get_fewshot_documents(&self) -> HashMap<String, Vec<Document>>;
    fn get_stop_string(&self) -> Vec<String>;
    fn get_answer_extraction_regex(&self) -> String;
}
