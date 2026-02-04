// TODO: The evaluations differ from those in lm_evals when using Deepseek-like models
// We believe this to be because of a mismatch in which tokenizer we use, we should investigate this further.
// Evals should be very close or the same as those in lm_evals with Llama-like models.

use crate::{
    TaskType, load_dataset,
    traits::{Document, LogLikelihoodTask},
};
use anyhow::Result;
use psyche_data_provider::{Dataset, Row, RowAccessor, Split};
use std::{collections::HashMap, fmt::Display};

pub struct CEval {
    datasets: Vec<(Dataset, Dataset, String)>, // (val, dev, subject)
}

const SUBJECT_MAPPING: &[(&str, &str)] = &[
    ("accountant", "注册会计师"),
    ("advanced_mathematics", "高等数学"),
    ("art_studies", "艺术学"),
    ("basic_medicine", "基础医学"),
    ("business_administration", "工商管理"),
    ("chinese_language_and_literature", "中国语言文学"),
    ("civil_servant", "公务员"),
    ("clinical_medicine", "临床医学"),
    ("college_chemistry", "大学化学"),
    ("college_economics", "大学经济学"),
    ("college_physics", "大学物理"),
    ("college_programming", "大学编程"),
    ("computer_architecture", "计算机组成"),
    ("computer_network", "计算机网络"),
    ("discrete_mathematics", "离散数学"),
    ("education_science", "教育学"),
    ("electrical_engineer", "注册电气工程师"),
    (
        "environmental_impact_assessment_engineer",
        "环境影响评价工程师",
    ),
    ("fire_engineer", "注册消防工程师"),
    ("high_school_biology", "高中生物"),
    ("high_school_chemistry", "高中化学"),
    ("high_school_chinese", "高中语文"),
    ("high_school_geography", "高中地理"),
    ("high_school_history", "高中历史"),
    ("high_school_mathematics", "高中数学"),
    ("high_school_physics", "高中物理"),
    ("high_school_politics", "高中政治"),
    (
        "ideological_and_moral_cultivation",
        "思想道德修养与法律基础",
    ),
    ("law", "法学"),
    ("legal_professional", "法律职业资格"),
    ("logic", "逻辑学"),
    (
        "mao_zedong_thought",
        "毛泽东思想和中国特色社会主义理论体系概论",
    ),
    ("marxism", "马克思主义基本原理"),
    ("metrology_engineer", "注册计量师"),
    ("middle_school_biology", "初中生物"),
    ("middle_school_chemistry", "初中化学"),
    ("middle_school_geography", "初中地理"),
    ("middle_school_history", "初中历史"),
    ("middle_school_mathematics", "初中数学"),
    ("middle_school_physics", "初中物理"),
    ("middle_school_politics", "初中政治"),
    ("modern_chinese_history", "近代史纲要"),
    ("operating_system", "操作系统"),
    ("physician", "医师资格"),
    ("plant_protection", "植物保护"),
    ("probability_and_statistics", "概率统计"),
    ("professional_tour_guide", "导游资格"),
    ("sports_science", "体育学"),
    ("tax_accountant", "税务师"),
    ("teacher_qualification", "教师资格"),
    ("urban_and_rural_planner", "注册城乡规划师"),
    ("veterinary_medicine", "兽医学"),
];

impl CEval {
    pub fn load() -> Result<TaskType> {
        let mut datasets = Vec::new();

        // Load all subjects
        for (subject_en, _) in SUBJECT_MAPPING {
            // Used for evaluation
            let val_dataset = load_dataset(
                "ceval/ceval-exam",
                None,
                Split::Val,
                Some(subject_en.to_string()),
            )?;

            // Used for fewshot examples
            let dev_dataset = load_dataset(
                "ceval/ceval-exam",
                None,
                Split::Dev,
                Some(subject_en.to_string()),
            )?;

            datasets.push((val_dataset, dev_dataset, subject_en.to_string()));
        }

        let ret = Self { datasets };
        Ok(TaskType::LogLikelihood(Box::new(ret)))
    }

    pub const fn name() -> &'static str {
        "CEval-valid"
    }

    fn get_subject_chinese_name(subject_en: &str) -> &str {
        SUBJECT_MAPPING
            .iter()
            .find(|(en, _)| *en == subject_en)
            .map(|(_, ch)| *ch)
            .unwrap_or("未知学科") // "Unknown Subject", should never reach here though.
    }

    fn row_to_document(dataset: &Dataset, row: Row, subject_en: &str) -> Document {
        let question = row
            .get_string(dataset.get_column_id("question").unwrap())
            .unwrap()
            .trim()
            .to_owned();

        let option_a = row.get_string(dataset.get_column_id("A").unwrap()).unwrap();
        let option_b = row.get_string(dataset.get_column_id("B").unwrap()).unwrap();
        let option_c = row.get_string(dataset.get_column_id("C").unwrap()).unwrap();
        let option_d = row.get_string(dataset.get_column_id("D").unwrap()).unwrap();

        let choices = vec![
            "A".to_owned(),
            "B".to_owned(),
            "C".to_owned(),
            "D".to_owned(),
        ];

        // In CEval the name of the subject is present in the description so we have to include it here
        let subject = Self::get_subject_chinese_name(subject_en);
        let description =
            format!("以下是中国关于{subject}的单项选择题，请选出其中的正确答案。\n\n");
        let doc_to_text = format!(
            "{question}\nA. {option_a}\nB. {option_b}\nC. {option_c}\nD. {option_d}\n答案："
        );
        let text = format!("{description}{doc_to_text}");

        let answer_str = row
            .get_string(dataset.get_column_id("answer").unwrap())
            .unwrap();
        let answer = match answer_str.as_str() {
            "A" => 0,
            "B" => 1,
            "C" => 2,
            "D" => 3,
            _ => panic!("Invalid answer: {answer_str}"),
        };

        Document {
            text,
            choices,
            answer,
            category: Some(subject.to_owned()),
            cot_content: None,
            eval_name: CEval::name().to_string(),
        }
    }
}

impl LogLikelihoodTask for CEval {
    fn get_documents(&self) -> Vec<Document> {
        let mut all_documents = Vec::new();

        for (val_dataset, _, subject) in &self.datasets {
            let documents: Vec<Document> = val_dataset
                .iter()
                .map(|row| CEval::row_to_document(val_dataset, row, subject))
                .collect();
            all_documents.extend(documents);
        }

        all_documents
    }

    fn get_fewshot_documents(&self) -> HashMap<String, Vec<Document>> {
        let mut fewshot_documents = HashMap::new();

        for (_, dev_dataset, subject) in &self.datasets {
            let documents: Vec<Document> = dev_dataset
                .iter()
                .map(|row| CEval::row_to_document(dev_dataset, row, subject))
                .collect();

            for doc in documents {
                if let Some(category) = &doc.category {
                    fewshot_documents
                        .entry(category.clone())
                        .or_insert_with(Vec::new)
                        .push(doc);
                }
            }
        }

        fewshot_documents
    }
}

impl Display for CEval {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", Self::name())
    }
}
