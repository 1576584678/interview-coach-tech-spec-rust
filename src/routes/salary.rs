//! 薪资定位:优先本地基准表,未命中时用大模型兜底(标注低置信度)。

use std::collections::HashMap;

use axum::extract::{Query, State};
use axum::Json;
use serde::Deserialize;

use crate::api::ApiBody;
use crate::error::{code, AppError, AppResult};
use crate::models::SalaryRange;
use crate::prompt::{self, name as prompt_name};
use crate::state::SharedState;

#[derive(Debug, Deserialize)]
pub struct EstimateQuery {
    pub position: String,
    #[serde(default)]
    pub city: Option<String>,
    #[serde(default)]
    pub experience: Option<String>,
}

pub async fn estimate(
    State(state): State<SharedState>,
    Query(query): Query<EstimateQuery>,
) -> AppResult<Json<ApiBody<SalaryRange>>> {
    let position = query.position.trim().to_string();
    if position.is_empty() {
        return Err(AppError::bad_request("岗位不能为空"));
    }
    let city = query.city.unwrap_or_default();
    let experience = query.experience.unwrap_or_default();

    if let Some(range) = state.salary.estimate_local(&position, &city, &experience) {
        return Ok(Json(ApiBody::ok(range)));
    }

    // 兜底:交给大模型估计,标注 low 置信度
    let city_tier = crate::salary::resolve_city_tier(&city);
    let experience_level = crate::salary::resolve_experience(&experience);
    let city_label = if city.trim().is_empty() {
        crate::salary::city_tier_label(city_tier).to_string()
    } else {
        format!("{}({})", city.trim(), crate::salary::city_tier_label(city_tier))
    };
    let mut vars: HashMap<&str, String> = HashMap::new();
    vars.insert("position", position.clone());
    vars.insert("city_tier", city_label);
    vars.insert("experience_level", crate::salary::experience_label(experience_level).to_string());
    let user_prompt = prompt::render(prompt_name::SALARY_ESTIMATOR, &vars)?;

    let llm = state.llm()?;
    let raw = llm
        .chat_json("你是一位资深薪酬分析师,请严格按照要求返回 JSON。", &user_prompt, &[])
        .await?;
    let value = crate::llm::extract_json(&raw)?;
    let p25 = value.get("p25").and_then(|v| v.as_i64()).unwrap_or(0);
    let p50 = value.get("p50").and_then(|v| v.as_i64()).unwrap_or(0);
    let p75 = value.get("p75").and_then(|v| v.as_i64()).unwrap_or(0);
    if p25 <= 0 || p50 < p25 || p75 < p50 || p75 > 1_000_000 {
        return Err(AppError::business(code::LLM_FAILED, "薪资估算结果异常,请稍后重试"));
    }
    Ok(Json(ApiBody::ok(SalaryRange {
        position,
        city: city.trim().to_string(),
        city_tier: crate::salary::city_tier_label(city_tier).to_string(),
        experience_level: crate::salary::experience_label(experience_level).to_string(),
        p25,
        p50,
        p75,
        confidence: "low".to_string(),
        source: "llm".to_string(),
        note: Some("该岗位暂无本地基准数据,数值为 AI 估计,置信度较低,仅供参考。".to_string()),
    })))
}
