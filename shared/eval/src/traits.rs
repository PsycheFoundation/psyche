use std::fmt::Display;

pub struct Document {
    pub text: String,
    pub choices: Vec<String>,
    pub answer: usize,
}

pub trait LogLikelihoodTask: Send + Display + Sync {
    fn get_documents(&self) -> Vec<Document>;
    fn get_fewshot_documents(&self) -> Vec<Document>;
}
