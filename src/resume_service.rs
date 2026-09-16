//! 简历诊断与优化(Prompt 与 Java 版一致)。

use std::collections::HashMap;

use crate::error::{AppError, AppResult};
use crate::models::{ResumeDiagnosis, ResumeOptimization};
use crate::prompt::{self, name as prompt_name};
use crate::state::SharedState;
use crate::util::{now_iso, truncate_chars};

const JSON_SYSTEM_PROMPT: &str = "你是一位资深简历顾问,请严格按照要求返回 JSON。";
/// 简历原文进入 prompt 前的截断长度。
const RESUME_TEXT_LIMIT: usize = 12_000;

fn resume_text(state: &SharedState, resume_id: u64) -> AppResult<String> {
    state
        .store
        .read(|db| {
            db.resumes
                .iter()
                .find(|r| r.id == resume_id)
                .map(|r| truncate_chars(&r.raw_text, RESUME_TEXT_LIMIT))
        })
        .ok_or_else(|| AppError::not_found("简历不存在"))
}

/// 简历诊断:只找问题,不改写。
pub async fn diagnose(
    state: &SharedState,
    resume_id: u64,
    target_position: Option<String>,
    jd_text: Option<String>,
) -> AppResult<ResumeDiagnosis> {
    let text = resume_text(state, resume_id)?;
    let position = target_position.unwrap_or_default();
    let mut vars: HashMap<&str, String> = HashMap::new();
    vars.insert("target_position", if position.trim().is_empty() { "未指定(按通用技术岗)".to_string() } else { position.clone() });
    vars.insert("jd_text", jd_text.unwrap_or_default());
    vars.insert("resume_text", text);
    let user_prompt = prompt::render(prompt_name::RESUME_DIAGNOSIS, &vars)?;

    let llm = state.llm()?;
    let raw = llm.chat_json(JSON_SYSTEM_PROMPT, &user_prompt, &[]).await?;
    let value = crate::llm::extract_json(&raw)?;
    let mut diagnosis: ResumeDiagnosis = serde_json::from_value(value)
        .map_err(|e| AppError::internal(format!("简历诊断 JSON 结构不合法: {e}")))?;
    diagnosis.match_score = diagnosis.match_score.clamp(0, 100);

    let position_for_store = if position.trim().is_empty() { None } else { Some(position) };
    let stored = diagnosis.clone();
    state.store.write(move |db| {
        let resume = db
            .resumes
            .iter_mut()
            .find(|r| r.id == resume_id)
            .ok_or_else(|| AppError::not_found("简历不存在"))?;
        resume.diagnosis = Some(stored.clone());
        resume.diagnosis_at = Some(now_iso());
        resume.match_score = Some(stored.match_score);
        if let Some(position) = position_for_store {
            resume.target_position = Some(position);
        }
        resume.updated_at = now_iso();
        Ok(())
    })?;
    Ok(diagnosis)
}

/// 简历优化:输出可执行的改写建议与优化后的完整简历。
pub async fn optimize(
    state: &SharedState,
    resume_id: u64,
    target_position: Option<String>,
    jd_text: Option<String>,
    style: Option<String>,
) -> AppResult<ResumeOptimization> {
    let text = resume_text(state, resume_id)?;
    let position = target_position.unwrap_or_default();
    let style_key = style.unwrap_or_default();
    let mut vars: HashMap<&str, String> = HashMap::new();
    vars.insert("target_position", if position.trim().is_empty() { "未指定(按通用技术岗)".to_string() } else { position.clone() });
    vars.insert("jd_text", jd_text.unwrap_or_default());
    vars.insert("style_instruction", prompt::resume_style_hint(&style_key));
    vars.insert("resume_text", text);
    let user_prompt = prompt::render(prompt_name::RESUME_OPTIMIZER, &vars)?;

    let llm = state.llm()?;
    let raw = llm.chat_json(JSON_SYSTEM_PROMPT, &user_prompt, &[]).await?;
    let value = crate::llm::extract_json(&raw)?;
    let mut optimization: ResumeOptimization = serde_json::from_value(value)
        .map_err(|e| AppError::internal(format!("简历优化 JSON 结构不合法: {e}")))?;
    optimization.match_score = optimization.match_score.clamp(0, 100);
    for (index, suggestion) in optimization.suggestions.iter_mut().enumerate() {
        if suggestion.id.trim().is_empty() {
            suggestion.id = format!("s{}", index + 1);
        }
        if suggestion.impact.trim().is_empty() {
            suggestion.impact = "medium".to_string();
        }
    }
    if optimization.optimized_resume.trim().is_empty() {
        return Err(AppError::internal("简历优化结果为空,请重试"));
    }

    let position_for_store = if position.trim().is_empty() { None } else { Some(position) };
    let stored = optimization.clone();
    state.store.write(move |db| {
        let resume = db
            .resumes
            .iter_mut()
            .find(|r| r.id == resume_id)
            .ok_or_else(|| AppError::not_found("简历不存在"))?;
        resume.optimization = Some(stored.clone());
        resume.optimized_at = Some(now_iso());
        resume.match_score = Some(stored.match_score);
        if let Some(position) = position_for_store {
            resume.target_position = Some(position);
        }
        resume.updated_at = now_iso();
        Ok(())
    })?;
    Ok(optimization)
}

/// 项目经历 STAR 口述改写(输出 Markdown)。
pub async fn star(
    state: &SharedState,
    project_text: String,
    target_position: Option<String>,
    jd_text: Option<String>,
) -> AppResult<String> {
    if project_text.trim().is_empty() {
        return Err(AppError::bad_request("请先填写项目经历"));
    }
    let mut vars: HashMap<&str, String> = HashMap::new();
    vars.insert(
        "target_position",
        target_position.filter(|p| !p.trim().is_empty()).unwrap_or_else(|| "未指定(按通用技术岗)".to_string()),
    );
    vars.insert("jd_text", jd_text.unwrap_or_default());
    vars.insert("project_text", truncate_chars(&project_text, 6000));
    let user_prompt = prompt::render(prompt_name::STAR_PROJECT, &vars)?;

    let llm = state.llm()?;
    llm.chat("你是一位资深技术面试教练,请按要求输出 Markdown。", &user_prompt, &[])
        .await
        .map(|text| text.trim().to_string())
}
