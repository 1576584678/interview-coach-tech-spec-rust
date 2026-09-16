//! 打分复盘:调用评委 Prompt,落库维度分与逐题反馈。

use serde_json::Value;

use crate::error::{code, AppError, AppResult};
use crate::models::{
    review_status, status, DimensionScores, QaFeedback, ReviewReport,
};
use crate::prompt::{self, name as prompt_name};
use crate::state::SharedState;
use crate::util::now_iso;

const SYSTEM_PROMPT: &str = "你是一位资深面试评委,请严格按照要求返回 JSON。";

/// 触发异步复盘:立即把状态置为 processing,后台线程完成后落库。
pub fn spawn_review(state: &SharedState, session_id: u64) {
    let should_start = state
        .store
        .write(|db| {
            let session = db
                .sessions
                .iter_mut()
                .find(|s| s.id == session_id)
                .ok_or_else(|| AppError::not_found("面试会话不存在"))?;
            if session.review.is_some() || session.review_status.as_deref() == Some(review_status::PROCESSING) {
                return Ok(false);
            }
            session.review_status = Some(review_status::PROCESSING.to_string());
            session.review_error = None;
            Ok(true)
        })
        .unwrap_or(false);
    if !should_start {
        return;
    }
    let state = state.clone();
    tokio::spawn(async move {
        if let Err(err) = generate(&state, session_id).await {
            tracing::error!("复盘生成失败: session_id={}, err={}", session_id, err.message());
            let message = err.message();
            let _ = state.store.write(move |db| {
                if let Some(session) = db.sessions.iter_mut().find(|s| s.id == session_id) {
                    session.review_status = Some(review_status::FAILED.to_string());
                    session.review_error = Some(message);
                }
                Ok(())
            });
        }
    });
}

/// 执行复盘(同步等待大模型返回)。
pub async fn generate(state: &SharedState, session_id: u64) -> AppResult<()> {
    let session = state
        .store
        .read(|db| db.sessions.iter().find(|s| s.id == session_id).cloned())
        .ok_or_else(|| AppError::not_found("面试会话不存在"))?;
    if !session.all_answered() {
        return Err(AppError::bad_request("所有题目完成后才能生成复盘"));
    }

    let mut vars: std::collections::HashMap<&str, String> = std::collections::HashMap::new();
    vars.insert("position", session.position.clone());
    vars.insert("qa_records", build_qa_records(&session));
    let user_prompt = prompt::render(prompt_name::REVIEWER, &vars)?;

    let llm = state.llm()?;
    // 复盘要逐题给反馈,输出很长,用更高的 token/超时预算
    let raw = llm.chat_json_long(SYSTEM_PROMPT, &user_prompt, &[]).await?;
    let value = crate::llm::extract_json(&raw)?;
    let report = parse_report(&value)?;

    let overall = report.overall_score;
    let feedback_by_order: Vec<(i32, QaFeedback)> =
        report.qa_feedback.iter().map(|fb| (fb.question_order, fb.clone())).collect();
    let report_for_store = report.clone();
    state.store.write(move |db| {
        let Some(session) = db.sessions.iter_mut().find(|s| s.id == session_id) else {
            return Err(AppError::not_found("面试会话不存在"));
        };
        for (order, feedback) in &feedback_by_order {
            if let Some(qa) = session.qa_list.iter_mut().find(|qa| qa.question_order == *order) {
                qa.score = Some(feedback.score);
                qa.feedback = Some(feedback.feedback.clone());
                qa.better_answer = Some(feedback.better_answer.clone());
            }
        }
        session.total_score = Some(overall);
        session.review = Some(report_for_store);
        session.review_status = Some(review_status::COMPLETED.to_string());
        session.review_error = None;
        if session.status == status::ONGOING {
            session.status = status::COMPLETED.to_string();
            session.completed_at = Some(now_iso());
        }
        Ok(())
    })
}

fn build_qa_records(session: &crate::models::InterviewSession) -> String {
    let mut out = String::new();
    for qa in &session.qa_list {
        out.push_str(&format!(
            "第{}问({}): [问题] {}\n",
            qa.question_order, qa.question_type, qa.question
        ));
        out.push_str(&format!("回答: {}\n\n", qa.answer.clone().unwrap_or_default()));
    }
    out
}

/// 解析评委返回的 JSON(对字段缺失/类型不规范尽量容错)。
fn parse_report(value: &Value) -> AppResult<ReviewReport> {
    let mut dimension_scores = DimensionScores::default();
    if let Some(dims) = value.get("dimensionScores") {
        for key in DimensionScores::KEYS {
            dimension_scores.set(key, to_int(dims.get(key)));
        }
    }
    if dimension_scores.values().iter().all(|v| *v == 0) {
        return Err(AppError::business(code::LLM_EMPTY, "复盘生成失败(评分缺失),请重试"));
    }

    let mut qa_feedback: Vec<QaFeedback> = Vec::new();
    if let Some(items) = value.get("qaFeedback").and_then(|v| v.as_array()) {
        for item in items {
            let order = to_int(item.get("questionOrder"));
            if order <= 0 {
                continue;
            }
            qa_feedback.push(QaFeedback {
                question_order: order,
                score: to_int(item.get("score")),
                feedback: to_text(item.get("feedback")),
                better_answer: to_text(item.get("betterAnswer")),
            });
        }
    }
    qa_feedback.sort_by_key(|fb| fb.question_order);

    let overall_score = dimension_scores.average();
    Ok(ReviewReport {
        overall_score,
        dimension_scores,
        strengths: to_text(value.get("strengths")),
        weaknesses: to_text(value.get("weaknesses")),
        improvement_plan: to_text(value.get("improvementPlan")),
        risk_warnings: to_text(value.get("riskWarnings")),
        created_at: now_iso(),
        qa_feedback,
    })
}

/// 数字字段容错:支持数字、数字字符串、浮点。
fn to_int(value: Option<&Value>) -> i32 {
    match value {
        Some(Value::Number(num)) => num.as_f64().map(|v| v.round() as i32).unwrap_or(0),
        Some(Value::String(text)) => text.trim().parse::<f64>().map(|v| v.round() as i32).unwrap_or(0),
        _ => 0,
    }
}

/// 文本字段容错:字符串直接返回,数组用换行拼接,其余类型转字符串。
fn to_text(value: Option<&Value>) -> String {
    match value {
        None | Some(Value::Null) => String::new(),
        Some(Value::String(text)) => text.clone(),
        Some(Value::Array(items)) => items
            .iter()
            .map(|item| match item {
                Value::String(text) => text.clone(),
                other => other.to_string(),
            })
            .collect::<Vec<_>>()
            .join("\n"),
        Some(other) => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_report_and_averages_score() {
        let value = json!({
            "dimensionScores": { "professional": 80, "expression": "70", "logic": 75, "communication": 72, "stress": 65 },
            "qaFeedback": [
                { "questionOrder": 2, "score": 60, "feedback": "太笼统", "betterAnswer": "用 STAR 展开" },
                { "questionOrder": 1, "score": 80, "feedback": "不错" }
            ],
            "strengths": "表达清晰",
            "weaknesses": ["缺少量化", "技术深度不足"],
            "improvementPlan": "每天复盘一题",
            "riskWarnings": ""
        });
        let report = parse_report(&value).unwrap();
        assert_eq!(report.overall_score, 72);
        assert_eq!(report.qa_feedback[0].question_order, 1);
        assert_eq!(report.qa_feedback[1].score, 60);
        assert!(report.weaknesses.contains("缺少量化"));
    }

    #[test]
    fn rejects_report_without_scores() {
        let value = json!({ "strengths": "无" });
        assert!(parse_report(&value).is_err());
    }
}
