//! 本地岗位题库(由 Java 版 question-bank.sql 种子数据转换而来,离线可用)。

use serde::Deserialize;

use crate::models::WeakPoint;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BankQuestion {
    pub category: String,
    pub question_type: String,
    pub question: String,
    #[serde(default)]
    pub answer_points: String,
    #[serde(default)]
    pub difficulty: String,
}

pub struct QuestionBank {
    questions: Vec<BankQuestion>,
}

impl QuestionBank {
    pub fn embedded() -> Self {
        let raw = include_str!("data/question_bank.json");
        let questions: Vec<BankQuestion> =
            serde_json::from_str(raw).expect("内置题库数据损坏,请检查 src/data/question_bank.json");
        Self { questions }
    }

    pub fn len(&self) -> usize {
        self.questions.len()
    }

    pub fn is_empty(&self) -> bool {
        self.questions.is_empty()
    }

    /// 只读遍历题库(用于统计等场景)。
    pub fn questions_iter(&self) -> impl Iterator<Item = &BankQuestion> {
        self.questions.iter()
    }

    /// 构建题库提示片段(按题型排序,已在面试中问过的题目不再重复给出)。
    pub fn build_hint(&self, category: &str, asked: &[String]) -> String {
        let category = category.trim().to_lowercase();
        if category.is_empty() {
            return "当前岗位未指定类别,请结合岗位自行出题。".to_string();
        }
        let mut matched: Vec<&BankQuestion> = self
            .questions
            .iter()
            .filter(|q| q.category == category && !asked.iter().any(|a| a.contains(&q.question)))
            .collect();
        if matched.is_empty() {
            return "当前岗位类别暂无题库数据,请结合岗位自行出题。".to_string();
        }
        matched.sort_by_key(|q| type_rank(&q.question_type));

        let mut out = String::from("以下是该岗位的高频题库(按题型分组)。出题时优先从中选取,可在题目基础上结合候选人简历追问细节;题库未覆盖的考察点再自行出题:\n");
        for q in matched {
            out.push_str(&format!("- [{}] {}", q.question_type, q.question));
            if !q.answer_points.trim().is_empty() {
                out.push_str(&format!("(考点: {})", q.answer_points.trim()));
            }
            out.push('\n');
        }
        out
    }

    /// 追加历史薄弱点提示(单机版直接从本地复盘结果统计得出)。
    pub fn build_weak_point_hint(weak_points: &[WeakPoint]) -> String {
        if weak_points.is_empty() {
            return String::new();
        }
        let mut out = String::from("\n候选人历史薄弱点(出题时优先针对这些考察点,可基于样例题目变换角度追问):\n");
        for point in weak_points {
            out.push_str(&format!(
                "- 题型[{}] 平均分 {}, 共答 {} 次",
                point.question_type, point.average_score, point.count
            ));
            if !point.sample_questions.is_empty() {
                out.push_str(&format!(",历史低分题: {}", point.sample_questions.join(" / ")));
            }
            out.push('\n');
        }
        out
    }
}

/// 题型在面试流程中的顺序: basic -> project -> deep -> open
fn type_rank(question_type: &str) -> u8 {
    match question_type {
        "basic" => 0,
        "project" => 1,
        "deep" => 2,
        "open" => 3,
        _ => 4,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_bank_is_loaded() {
        let bank = QuestionBank::embedded();
        assert!(bank.len() >= 80, "题库数量异常: {}", bank.len());
        let hint = bank.build_hint("backend", &[]);
        assert!(hint.contains("JVM") || hint.contains("HashMap"));
    }

    #[test]
    fn asked_questions_are_excluded() {
        let bank = QuestionBank::embedded();
        let first = bank
            .questions
            .iter()
            .find(|q| q.category == "backend")
            .map(|q| q.question.clone())
            .unwrap();
        let hint = bank.build_hint("backend", &[first.clone()]);
        assert!(!hint.contains(&first));
    }
}
