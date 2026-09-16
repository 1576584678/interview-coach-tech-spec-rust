//! 设置:大模型配置的读写、连通性测试,以及前端需要的枚举元数据。

use axum::extract::State;
use axum::Json;
use serde::{Deserialize, Serialize};

use crate::api::ApiBody;
use crate::config::AppConfig;
use crate::error::AppResult;
use crate::state::SharedState;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigView {
    pub base_url: String,
    pub api_key_masked: String,
    pub api_key_set: bool,
    pub model: String,
    pub temperature: f32,
    pub max_tokens: u32,
    pub timeout_seconds: u64,
    pub host: String,
    pub port: u16,
    pub auto_open_browser: bool,
    pub data_dir: String,
    pub config_file: String,
    pub total_questions: usize,
    pub llm_configured: bool,
    pub chat_completions_url: String,
}

fn to_view(state: &SharedState, config: &AppConfig) -> ConfigView {
    ConfigView {
        base_url: config.llm.base_url.clone(),
        api_key_masked: config.llm.masked_api_key(),
        api_key_set: !config.llm.api_key.trim().is_empty(),
        model: config.llm.model.clone(),
        temperature: config.llm.temperature,
        max_tokens: config.llm.max_tokens,
        timeout_seconds: config.llm.timeout_seconds,
        host: config.server.host.clone(),
        port: config.server.port,
        auto_open_browser: config.server.auto_open_browser,
        data_dir: config.server.data_dir.clone(),
        config_file: state.config_path(),
        total_questions: config.interview.total_questions,
        llm_configured: config.llm.configured(),
        chat_completions_url: config.llm.chat_completions_url(),
    }
}

pub async fn get_config(State(state): State<SharedState>) -> Json<ApiBody<ConfigView>> {
    let config = state.config_snapshot();
    Json(ApiBody::ok(to_view(&state, &config)))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateConfigRequest {
    pub base_url: Option<String>,
    /// 传空字符串表示清空;不传表示保持原值。
    pub api_key: Option<String>,
    pub model: Option<String>,
    pub temperature: Option<f32>,
    pub max_tokens: Option<u32>,
    pub timeout_seconds: Option<u64>,
    pub total_questions: Option<usize>,
    pub auto_open_browser: Option<bool>,
}

pub async fn update_config(
    State(state): State<SharedState>,
    Json(req): Json<UpdateConfigRequest>,
) -> AppResult<Json<ApiBody<ConfigView>>> {
    let mut config = state.config_snapshot();
    if let Some(value) = req.base_url {
        config.llm.base_url = value.trim().to_string();
    }
    if let Some(value) = req.api_key {
        config.llm.api_key = value.trim().to_string();
    }
    if let Some(value) = req.model {
        config.llm.model = value.trim().to_string();
    }
    if let Some(value) = req.temperature {
        config.llm.temperature = value;
    }
    if let Some(value) = req.max_tokens {
        config.llm.max_tokens = value;
    }
    if let Some(value) = req.timeout_seconds {
        config.llm.timeout_seconds = value;
    }
    if let Some(value) = req.total_questions {
        config.interview.total_questions = value;
    }
    if let Some(value) = req.auto_open_browser {
        config.server.auto_open_browser = value;
    }
    state.update_config(config)?;
    let saved = state.config_snapshot();
    Ok(Json(ApiBody::ok(to_view(&state, &saved))))
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TestLlmResponse {
    pub ok: bool,
    pub message: String,
    pub model: String,
}

/// 测试大模型连通性(不修改配置)。
pub async fn test_llm(State(state): State<SharedState>) -> AppResult<Json<ApiBody<TestLlmResponse>>> {
    let llm = state.llm()?;
    let model = llm.config().model.clone();
    let reply = llm.ping().await?;
    Ok(Json(ApiBody::ok(TestLlmResponse {
        ok: true,
        message: format!("连接成功,模型回复: {}", crate::llm::abbreviate(&reply, 80)),
        model,
    })))
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct OptionItem {
    value: &'static str,
    label: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    hint: Option<&'static str>,
}

pub async fn meta(State(state): State<SharedState>) -> Json<ApiBody<serde_json::Value>> {
    let categories = vec![
        OptionItem { value: "backend", label: "后端开发", hint: Some("语言基础 / 数据库 / 分布式 / 高并发") },
        OptionItem { value: "frontend", label: "前端开发", hint: Some("JS 原理 / 浏览器 / 性能优化 / 工程化") },
        OptionItem { value: "agent", label: "AI / Agent 开发", hint: Some("LLM 原理 / RAG / 工具调用 / Prompt 工程") },
        OptionItem { value: "data", label: "数据分析", hint: Some("SQL / 统计 / 业务分析") },
        OptionItem { value: "product", label: "产品经理", hint: Some("需求分析 / 数据驱动 / 项目推进") },
        OptionItem { value: "design", label: "设计师", hint: Some("设计方法论 / 用户体验 / 作品集") },
        OptionItem { value: "operation", label: "运营", hint: Some("增长 / 活动策划 / 用户运营") },
        OptionItem { value: "devops", label: "运维 / SRE", hint: Some("Linux / 容器 / CI-CD / 监控") },
        OptionItem { value: "web3", label: "Web3 / 区块链", hint: Some("智能合约 / 密码学 / 链上数据") },
        OptionItem { value: "general", label: "通用 / 其他", hint: Some("按岗位自行出题") },
    ];
    let difficulties = vec![
        OptionItem { value: "easy", label: "简单", hint: Some("适合练习手感") },
        OptionItem { value: "normal", label: "标准", hint: Some("贴近真实面试") },
        OptionItem { value: "hard", label: "困难", hint: Some("连环追问,强度高") },
    ];
    let styles = vec![
        OptionItem { value: "friendly", label: "友好亲和", hint: Some("以鼓励为主,循序渐进") },
        OptionItem { value: "strict", label: "严谨挑剔", hint: Some("追着漏洞问") },
        OptionItem { value: "stress", label: "压力面试", hint: Some("语气直接,连环追问") },
    ];
    let modes = vec![
        OptionItem { value: "normal", label: "标准训练", hint: Some("可以慢慢想") },
        OptionItem { value: "real", label: "真实模拟", hint: Some("可以跳过题目,节奏更快") },
    ];
    let resume_styles = vec![
        OptionItem { value: "general", label: "通用求职", hint: None },
        OptionItem { value: "bigtech", label: "大厂技术岗", hint: None },
        OptionItem { value: "foreign", label: "外企", hint: None },
        OptionItem { value: "state-owned", label: "国企 / 央企", hint: None },
    ];

    let mut by_category: std::collections::BTreeMap<String, usize> = std::collections::BTreeMap::new();
    for question in state.question_bank.questions_iter() {
        *by_category.entry(question.category.clone()).or_insert(0) += 1;
    }

    Json(ApiBody::ok(serde_json::json!({
        "positionCategories": categories,
        "difficulties": difficulties,
        "styles": styles,
        "modes": modes,
        "resumeStyles": resume_styles,
        "questionBank": {
            "total": state.question_bank.len(),
            "byCategory": by_category,
        },
        "salaryBenchmarkCount": state.salary.benchmark_count(),
        "version": env!("CARGO_PKG_VERSION"),
        "supportsFileTypes": ["txt", "md", "docx", "pdf"],
    })))
}
