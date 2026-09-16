//! 迭代追踪:统计、薄弱点分析、个性化提升计划。

use std::collections::HashMap;

use crate::error::AppResult;
use crate::models::{status, DimensionScores, ImprovementPlan, InterviewStats, ScorePoint, WeakPoint};
use crate::prompt::{self, name as prompt_name};
use crate::state::SharedState;
use crate::store::Store;
use crate::util::{now_iso, question_type_label, truncate_chars};

/// 参与薄弱点分析的最大场次与产出上限(与 Java 版一致)。
const MAX_RECENT_SESSIONS: usize = 10;
const MAX_WEAK_POINTS: usize = 3;
const MAX_SAMPLE_QUESTIONS: usize = 3;
const WEAK_SCORE_THRESHOLD: i32 = 70;
const MAX_RECENT_REPORTS: usize = 3;

struct Bucket {
    total: i32,
    count: usize,
    samples: Vec<String>,
}

/// 按题型聚合历史得分,挑出平均分低于阈值且作答次数足够的题型。
pub fn weak_points(store: &Store) -> Vec<WeakPoint> {
    store.read(|db| {
        let mut scored: Vec<_> = db
            .sessions
            .iter()
            .filter(|s| s.status == status::COMPLETED && s.review.is_some())
            .collect();
        scored.sort_by(|a, b| b.completed_at.cmp(&a.completed_at));
        scored.truncate(MAX_RECENT_SESSIONS);

        let mut buckets: HashMap<String, Bucket> = HashMap::new();
        for session in scored {
            let Some(report) = session.review.as_ref() else { continue };
            for feedback in &report.qa_feedback {
                let matched = session
                    .qa_list
                    .iter()
                    .find(|qa| qa.question_order == feedback.question_order);
                let qa_type = matched
                    .map(|qa| qa.question_type.clone())
                    .unwrap_or_else(|| "basic".to_string());
                let entry = buckets
                    .entry(qa_type)
                    .or_insert_with(|| Bucket { total: 0, count: 0, samples: Vec::new() });
                entry.total += feedback.score;
                entry.count += 1;
                if feedback.score < WEAK_SCORE_THRESHOLD && entry.samples.len() < MAX_SAMPLE_QUESTIONS {
                    if let Some(qa) = matched {
                        entry.samples.push(truncate_chars(&qa.question, 40));
                    }
                }
            }
        }

        let mut points: Vec<WeakPoint> = buckets
            .into_iter()
            .filter(|(_, bucket)| bucket.count > 0)
            .map(|(qa_type, bucket)| {
                let average = (bucket.total + bucket.count as i32 / 2) / bucket.count as i32;
                WeakPoint {
                    label: question_type_label(&qa_type).to_string(),
                    question_type: qa_type,
                    average_score: average,
                    count: bucket.count,
                    sample_questions: bucket.samples,
                }
            })
            .filter(|point| point.average_score < WEAK_SCORE_THRESHOLD)
            .collect();
        points.sort_by_key(|point| point.average_score);
        points.truncate(MAX_WEAK_POINTS);
        points
    })
}

pub fn compute_stats(store: &Store) -> InterviewStats {
    // 先算统计主体,再单独算薄弱点(避免在读锁内再次加读锁)
    let stats = store.read(|db| {
        let total_sessions = db.sessions.len();
        let completed_sessions = db.sessions.iter().filter(|s| s.status == status::COMPLETED).count();

        let mut scored: Vec<_> = db
            .sessions
            .iter()
            .filter(|s| s.status == status::COMPLETED && s.total_score.is_some())
            .collect();
        scored.sort_by(|a, b| a.completed_at.cmp(&b.completed_at));

        let trend: Vec<ScorePoint> = scored
            .iter()
            .map(|session| ScorePoint {
                session_id: session.id,
                position: session.position.clone(),
                score: session.total_score.unwrap_or_default(),
                completed_at: session.completed_at.clone().unwrap_or_default(),
            })
            .collect();

        let scores: Vec<i32> = trend.iter().map(|point| point.score).collect();
        let avg_score = if scores.is_empty() {
            None
        } else {
            let sum: i32 = scores.iter().sum();
            Some((sum + scores.len() as i32 / 2) / scores.len() as i32)
        };
        let max_score = scores.iter().copied().max();
        let latest_score = scores.last().copied();

        let reports: Vec<_> = scored
            .iter()
            .rev()
            .take(MAX_RECENT_SESSIONS)
            .filter_map(|session| session.review.as_ref())
            .collect();
        let mut dimension_averages = DimensionScores::default();
        if !reports.is_empty() {
            for key in DimensionScores::KEYS {
                let sum: i32 = reports.iter().map(|report| report.dimension_scores.get(key)).sum();
                dimension_averages.set(key, (sum + reports.len() as i32 / 2) / reports.len() as i32);
            }
        }

        InterviewStats {
            total_sessions,
            completed_sessions,
            avg_score,
            max_score,
            latest_score,
            trend,
            dimension_averages,
            weak_points: Vec::new(),
        }
    });
    InterviewStats { weak_points: weak_points(store), ..stats }
}

/// 生成(或读取缓存的)提升计划。
pub async fn improvement_plan(state: &SharedState, force: bool) -> AppResult<ImprovementPlan> {
    let (completed_count, position, score_trend, recent_feedback) = state.store.read(|db| {
        let mut scored: Vec<_> = db
            .sessions
            .iter()
            .filter(|s| s.status == status::COMPLETED && s.review.is_some())
            .collect();
        scored.sort_by(|a, b| a.completed_at.cmp(&b.completed_at));
        let completed_count = scored.len();
        let position = scored
            .last()
            .map(|s| s.position.clone())
            .unwrap_or_else(|| "未指定岗位".to_string());
        let score_trend = scored
            .iter()
            .map(|s| {
                format!(
                    "{} {} 总分 {}",
                    s.completed_at.clone().unwrap_or_default(),
                    s.position,
                    s.total_score.unwrap_or_default()
                )
            })
            .collect::<Vec<_>>()
            .join("; ");
        let recent_feedback = scored
            .iter()
            .rev()
            .take(MAX_RECENT_REPORTS)
            .filter_map(|s| s.review.as_ref())
            .map(|report| {
                format!(
                    "薄弱项: {} | 改进建议: {}",
                    truncate_chars(&report.weaknesses.replace('\n', " "), 160),
                    truncate_chars(&report.improvement_plan.replace('\n', " "), 200)
                )
            })
            .collect::<Vec<_>>()
            .join("\n");
        (completed_count, position, score_trend, recent_feedback)
    });

    if completed_count == 0 {
        return Err(crate::error::AppError::bad_request("还没有已完成的模拟面试,先去做一场面试吧"));
    }
    if !force {
        let cached = state
            .store
            .read(|db| db.improvement_plan.clone())
            .filter(|plan| plan.based_on_sessions == completed_count);
        if let Some(plan) = cached {
            return Ok(plan);
        }
    }

    let weak_points = weak_points(&state.store);
    let weak_text = if weak_points.is_empty() {
        "暂无(各题型平均分均达标)".to_string()
    } else {
        weak_points
            .iter()
            .map(|point| format!("{} 平均分 {}", point.label, point.average_score))
            .collect::<Vec<_>>()
            .join("; ")
    };

    let mut vars: HashMap<&str, String> = HashMap::new();
    vars.insert("position", position);
    vars.insert("score_trend", if score_trend.is_empty() { "无".to_string() } else { score_trend });
    vars.insert("weak_points", weak_text);
    vars.insert(
        "recent_feedback",
        if recent_feedback.is_empty() { "无(近期未生成复盘报告)".to_string() } else { recent_feedback },
    );
    let user_prompt = prompt::render(prompt_name::IMPROVEMENT_PLAN, &vars)?;

    let llm = state.llm()?;
    let raw = llm
        .chat_json("你是一位资深面试教练,请严格按照要求返回 JSON。", &user_prompt, &[])
        .await?;
    let value = crate::llm::extract_json(&raw)?;
    let mut plan: ImprovementPlan = serde_json::from_value(value)
        .map_err(|e| crate::error::AppError::internal(format!("提升计划 JSON 结构不合法: {e}")))?;
    plan.generated_at = now_iso();
    plan.based_on_sessions = completed_count;
    if plan.focus_areas.is_empty() && plan.weekly_plan.is_empty() {
        return Err(crate::error::AppError::internal("提升计划内容为空,请重试"));
    }
    let stored = plan.clone();
    state.store.write(move |db| {
        db.improvement_plan = Some(stored);
        Ok(())
    })?;
    Ok(plan)
}
