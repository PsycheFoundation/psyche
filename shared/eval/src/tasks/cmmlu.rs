use crate::{
    TaskType, load_dataset,
    traits::{Document, LogLikelihoodTask},
};
use anyhow::Result;
use psyche_data_provider::{Dataset, Row, RowAccessor, Split};
use std::fmt::Display;

pub struct CMMLU {
    datasets: Vec<(Dataset, Dataset, String)>, // (test, dev, subject)
}

const SUBJECT_MAPPING: &[(&str, &str)] = &[
    ("agronomy", "农学"),
    ("anatomy", "解剖学"),
    ("ancient_chinese", "古汉语"),
    ("arts", "艺术学"),
    ("astronomy", "天文学"),
    ("business_ethics", "商业伦理"),
    ("chinese_civil_service_exam", "中国公务员考试"),
    ("chinese_driving_rule", "中国驾驶规则"),
    ("chinese_food_culture", "中国饮食文化"),
    ("chinese_foreign_policy", "中国外交政策"),
    ("chinese_history", "中国历史"),
    ("chinese_literature", "中国文学"),
    ("chinese_teacher_qualification", "中国教师资格"),
    ("clinical_knowledge", "临床知识"),
    ("college_actuarial_science", "大学精算学"),
    ("college_education", "大学教育学"),
    ("college_engineering_hydrology", "大学工程水文学"),
    ("college_law", "大学法律"),
    ("college_mathematics", "大学数学"),
    ("college_medical_statistics", "大学医学统计"),
    ("college_medicine", "大学医学"),
    ("computer_science", "计算机科学"),
    ("computer_security", "计算机安全"),
    ("conceptual_physics", "概念物理学"),
    ("construction_project_management", "建设工程管理"),
    ("economics", "经济学"),
    ("education", "教育学"),
    ("electrical_engineering", "电气工程"),
    ("elementary_chinese", "小学语文"),
    ("elementary_commonsense", "小学常识"),
    ("elementary_information_and_technology", "小学信息技术"),
    ("elementary_mathematics", "初等数学"),
    ("ethnology", "民族学"),
    ("food_science", "食品科学"),
    ("genetics", "遗传学"),
    ("global_facts", "全球事实"),
    ("high_school_biology", "高中生物"),
    ("high_school_chemistry", "高中化学"),
    ("high_school_geography", "高中地理"),
    ("high_school_mathematics", "高中数学"),
    ("high_school_physics", "高中物理学"),
    ("high_school_politics", "高中政治"),
    ("human_sexuality", "人类性行为"),
    ("international_law", "国际法学"),
    ("journalism", "新闻学"),
    ("jurisprudence", "法理学"),
    ("legal_and_moral_basis", "法律与道德基础"),
    ("logical", "逻辑学"),
    ("machine_learning", "机器学习"),
    ("management", "管理学"),
    ("marketing", "市场营销"),
    ("marxist_theory", "马克思主义理论"),
    ("modern_chinese", "现代汉语"),
    ("nutrition", "营养学"),
    ("philosophy", "哲学"),
    ("professional_accounting", "专业会计"),
    ("professional_law", "专业法学"),
    ("professional_medicine", "专业医学"),
    ("professional_psychology", "专业心理学"),
    ("public_relations", "公共关系"),
    ("security_study", "安全研究"),
    ("sociology", "社会学"),
    ("sports_science", "体育学"),
    ("traditional_chinese_medicine", "中医中药"),
    ("virology", "病毒学"),
    ("world_history", "世界历史"),
    ("world_religions", "世界宗教"),
];

impl CMMLU {
    pub fn load() -> Result<TaskType> {
        let mut datasets = Vec::new();

        // Load all subjects
        for (subject_en, _) in SUBJECT_MAPPING {
            let test_dataset = load_dataset(
                "haonan-li/cmmlu",
                Some("refs/convert/parquet".to_owned()),
                Split::Test,
                Some(subject_en.to_string()),
            )?;

            // cmmlu does not have a "validation" split, so we use "dev" split as few-shot examples
            let dev_dataset = load_dataset(
                "haonan-li/cmmlu",
                Some("refs/convert/parquet".to_owned()),
                Split::Dev,
                Some(subject_en.to_string()),
            )?;

            datasets.push((test_dataset, dev_dataset, subject_en.to_string()));
        }

        let ret = Self { datasets };
        Ok(TaskType::LogLikelihood(Box::new(ret)))
    }

    pub const fn name() -> &'static str {
        "CMMLU"
    }

    fn get_subject_chinese_name(subject_en: &str) -> &str {
        SUBJECT_MAPPING
            .iter()
            .find(|(en, _)| *en == subject_en)
            .map(|(_, zh)| *zh)
            .unwrap_or("未知学科") // "Unknown Subject"
    }

    fn row_to_document(dataset: &Dataset, row: Row, subject_en: &str) -> Document {
        let question = row
            .get_string(dataset.get_column_id("Question").unwrap())
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

        // In CMMLU the name of the subject is present in the description so we have to include it here
        // "Here are some multiple-choice question about {Computer Science}, please directly give the correct answer option."
        let subject_zh = Self::get_subject_chinese_name(subject_en);
        let description =
            format!("以下是关于{subject_zh}的单项选择题，请直接给出正确答案的选项。\n\n");
        let doc_to_text = format!(
            "{question}\nA. {option_a}\nB. {option_b}\nC. {option_c}\nD. {option_d}\n答案："
        );
        let text = format!("{description}{doc_to_text}");

        let answer_str = row
            .get_string(dataset.get_column_id("Answer").unwrap())
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
        }
    }
}

impl LogLikelihoodTask for CMMLU {
    fn get_documents(&self) -> Vec<Document> {
        let mut all_documents = Vec::new();

        for (test_dataset, _, subject) in &self.datasets {
            let documents: Vec<Document> = test_dataset
                .iter()
                .map(|row| CMMLU::row_to_document(test_dataset, row, subject))
                .collect();
            all_documents.extend(documents);
        }

        all_documents
    }

    fn get_fewshot_documents(&self) -> Vec<Document> {
        let mut all_documents = Vec::new();

        for (_, dev_dataset, subject) in &self.datasets {
            let documents: Vec<Document> = dev_dataset
                .iter()
                .map(|row| CMMLU::row_to_document(dev_dataset, row, subject))
                .collect();
            all_documents.extend(documents);
        }

        all_documents
    }
}

impl Display for CMMLU {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", Self::name())
    }
}
