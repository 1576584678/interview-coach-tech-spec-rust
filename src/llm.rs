//! OpenAI 兼容大模型客户端(DeepSeek / 通义 / OpenAI / 本地 Ollama 等都可直接用)。

use std::time::Duration;

use futures_util::StreamExt;
use serde_json::{json, Value};

use crate::config::LlmConfig;
use crate::error::{code, AppError, AppResult};
use crate::models::ChatMessage;

/// 重试退避(毫秒),仅对 5xx / 超时 / 网络抖动重试。
const BACKOFF_MS: [u64; 2] = [3000, 8000];

/// 长输出任务(整份简历改写、面试复盘报告等)的最小预算。
/// 默认配置(2000 tokens / 90 秒)只够短回答:长 JSON 会被截断,或未生成完就超时。
const LONG_MIN_MAX_TOKENS: u32 = 8000;
const LONG_MIN_TIMEOUT_SECS: u64 = 240;

/// 单次调用的预算:token 上限、超时时间、重试次数。
struct CallBudget {
    max_tokens: u32,
    timeout: Duration,
    retries: usize,
    retry_on_timeout: bool,
}

impl CallBudget {
    fn from_config(cfg: &LlmConfig) -> Self {
        Self {
            max_tokens: cfg.max_tokens,
            timeout: Duration::from_secs(cfg.timeout_seconds),
            retries: BACKOFF_MS.len(),
            retry_on_timeout: true,
        }
    }

    /// 长输出任务:token 与超时都取「配置值」和「最小要求」里更大的那个,
    /// 但超时不重试——同一份大 prompt 重发一次大概率还是超时。
    fn long_for(cfg: &LlmConfig) -> Self {
        Self {
            max_tokens: cfg.max_tokens.max(LONG_MIN_MAX_TOKENS),
            timeout: Duration::from_secs(cfg.timeout_seconds.max(LONG_MIN_TIMEOUT_SECS)),
            retries: 1,
            retry_on_timeout: false,
        }
    }
}

pub struct LlmClient {
    http: reqwest::Client,
    cfg: LlmConfig,
}

impl LlmClient {
    pub fn new(cfg: LlmConfig) -> AppResult<Self> {
        if !cfg.configured() {
            return Err(AppError::business(
                code::CONFIG_ERROR,
                "尚未配置大模型:请在「设置」中填写接口地址、API Key 和模型名",
            ));
        }
        let http = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(15))
            .build()?;
        Ok(Self { http, cfg })
    }

    pub fn config(&self) -> &LlmConfig {
        &self.cfg
    }

    /// 普通对话。
    pub async fn chat(
        &self,
        system_prompt: &str,
        user_message: &str,
        history: &[ChatMessage],
    ) -> AppResult<String> {
        self.chat_with_retry(system_prompt, user_message, history, false, CallBudget::from_config(&self.cfg))
            .await
    }

    /// JSON 模式对话(要求响应为 JSON 对象)。
    pub async fn chat_json(
        &self,
        system_prompt: &str,
        user_message: &str,
        history: &[ChatMessage],
    ) -> AppResult<String> {
        self.chat_with_retry(system_prompt, user_message, history, true, CallBudget::from_config(&self.cfg))
            .await
    }

    /// 长输出 JSON 对话:自动抬高 token 与超时预算,超时也不再重试
    /// (同一份大 prompt 重发一次大概率还是超时,只会让用户多等几分钟)。
    pub async fn chat_json_long(
        &self,
        system_prompt: &str,
        user_message: &str,
        history: &[ChatMessage],
    ) -> AppResult<String> {
        let budget = CallBudget::long_for(&self.cfg);
        self.chat_with_retry(system_prompt, user_message, history, true, budget).await
    }

    /// 流式对话:每收到一段增量就回调一次,最后返回完整文本。
    pub async fn chat_stream<F>(
        &self,
        system_prompt: &str,
        user_message: &str,
        history: &[ChatMessage],
        mut on_delta: F,
    ) -> AppResult<String>
    where
        F: FnMut(&str) + Send,
    {
        self.stream_once(system_prompt, user_message, history, &mut on_delta).await
    }

    /// 连通性自检:返回模型回复内容。
    pub async fn ping(&self) -> AppResult<String> {
        let text = self
            .chat("你是一个健康检查助手,只回复四个字:连接正常。", "ping", &[])
            .await?;
        Ok(text.trim().to_string())
    }

    async fn chat_with_retry(
        &self,
        system_prompt: &str,
        user_message: &str,
        history: &[ChatMessage],
        json_mode: bool,
        budget: CallBudget,
    ) -> AppResult<String> {
        let mut attempt = 0usize;
        loop {
            match self
                .chat_once(system_prompt, user_message, history, json_mode, &budget)
                .await
            {
                Ok(text) => return Ok(text),
                Err(err) => {
                    let retryable = (matches!(err.code(), code::LLM_TIMEOUT) && budget.retry_on_timeout)
                        || (err.code() == code::LLM_FAILED && is_retryable(&err.message()));
                    if attempt < budget.retries && attempt < BACKOFF_MS.len() && retryable {
                        let wait = BACKOFF_MS[attempt];
                        tracing::warn!("大模型调用失败(第 {} 次),{}ms 后重试: {}", attempt + 1, wait, err.message());
                        tokio::time::sleep(Duration::from_millis(wait)).await;
                        attempt += 1;
                        continue;
                    }
                    return Err(err);
                }
            }
        }
    }

    async fn chat_once(
        &self,
        system_prompt: &str,
        user_message: &str,
        history: &[ChatMessage],
        json_mode: bool,
        budget: &CallBudget,
    ) -> AppResult<String> {
        let url = self.chat_completions_url()?;
        let body = self.build_body(system_prompt, user_message, history, json_mode, false, budget.max_tokens);
        let response = self
            .http
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.cfg.api_key.trim()))
            .header("Content-Type", "application/json")
            .json(&body)
            .timeout(budget.timeout)
            .send()
            .await
            .map_err(map_reqwest_error)?;

        let status = response.status();
        if !status.is_success() {
            let detail = response.text().await.unwrap_or_default();
            return Err(AppError::business(
                code::LLM_FAILED,
                format!("大模型返回 HTTP {}: {}", status.as_u16(), abbreviate(&detail, 300)),
            ));
        }

        let payload: Value = response.json().await.map_err(map_reqwest_error)?;
        if let Some(err) = payload.get("error") {
            return Err(AppError::business(
                code::LLM_FAILED,
                format!("大模型返回错误: {}", abbreviate(&err.to_string(), 300)),
            ));
        }
        let choice = payload.get("choices").and_then(|c| c.get(0));
        let finish_reason = choice
            .and_then(|c| c.get("finish_reason"))
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        let content = choice
            .and_then(|c| c.get("message"))
            .and_then(|m| m.get("content"))
            .and_then(|c| c.as_str())
            .unwrap_or_default()
            .to_string();
        // JSON 模式下被截断一定是坏结果,直接给出可操作的提示
        if json_mode && finish_reason == "length" {
            return Err(AppError::business(
                code::LLM_FAILED,
                format!(
                    "大模型输出被截断(已达 {} tokens 上限):请在「设置」里把 max_tokens 调大后重试",
                    budget.max_tokens
                ),
            ));
        }
        if content.trim().is_empty() {
            return Err(AppError::business(code::LLM_EMPTY, "大模型返回内容为空,请重试"));
        }
        Ok(content)
    }

    async fn stream_once<F>(
        &self,
        system_prompt: &str,
        user_message: &str,
        history: &[ChatMessage],
        on_delta: &mut F,
    ) -> AppResult<String>
    where
        F: FnMut(&str) + Send,
    {
        let url = self.chat_completions_url()?;
        let body = self.build_body(system_prompt, user_message, history, false, true, self.cfg.max_tokens);
        let response = self
            .http
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.cfg.api_key.trim()))
            .header("Content-Type", "application/json")
            .header("Accept", "text/event-stream")
            .json(&body)
            .timeout(Duration::from_secs(self.cfg.timeout_seconds))
            .send()
            .await
            .map_err(map_reqwest_error)?;

        let status = response.status();
        if !status.is_success() {
            let detail = response.text().await.unwrap_or_default();
            return Err(AppError::business(
                code::LLM_FAILED,
                format!("大模型返回 HTTP {}: {}", status.as_u16(), abbreviate(&detail, 300)),
            ));
        }

        let mut stream = response.bytes_stream();
        let mut buffer = String::new();
        let mut full = String::new();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(map_reqwest_error)?;
            buffer.push_str(&String::from_utf8_lossy(&chunk));
            while let Some(idx) = buffer.find('\n') {
                let line: String = buffer.drain(..=idx).collect();
                let line = line.trim();
                let Some(data) = line.strip_prefix("data:") else { continue };
                let data = data.trim();
                if data == "[DONE]" {
                    if full.trim().is_empty() {
                        return Err(AppError::business(code::LLM_EMPTY, "大模型返回内容为空,请重试"));
                    }
                    return Ok(full);
                }
                let Ok(value) = serde_json::from_str::<Value>(data) else { continue };
                if let Some(piece) = value
                    .get("choices")
                    .and_then(|c| c.get(0))
                    .and_then(|c| c.get("delta"))
                    .and_then(|d| d.get("content"))
                    .and_then(|c| c.as_str())
                {
                    full.push_str(piece);
                    on_delta(piece);
                }
            }
        }
        if full.trim().is_empty() {
            return Err(AppError::business(code::LLM_EMPTY, "大模型返回内容为空,请重试"));
        }
        Ok(full)
    }

    fn chat_completions_url(&self) -> AppResult<String> {
        let url = self.cfg.chat_completions_url();
        if url.is_empty() {
            return Err(AppError::business(code::CONFIG_ERROR, "尚未配置大模型接口地址"));
        }
        Ok(url)
    }

    fn build_body(
        &self,
        system_prompt: &str,
        user_message: &str,
        history: &[ChatMessage],
        json_mode: bool,
        stream: bool,
        max_tokens: u32,
    ) -> Value {
        let mut messages: Vec<Value> = Vec::with_capacity(history.len() + 2);
        if !system_prompt.trim().is_empty() {
            messages.push(json!({ "role": "system", "content": system_prompt }));
        }
        for msg in history {
            if msg.role.is_empty() {
                continue;
            }
            messages.push(json!({ "role": msg.role, "content": msg.content }));
        }
        messages.push(json!({ "role": "user", "content": user_message }));

        let mut body = json!({
            "model": self.cfg.model.trim(),
            "temperature": self.cfg.temperature,
            "max_tokens": max_tokens,
            "stream": stream,
            "messages": messages,
        });
        if json_mode {
            body["response_format"] = json!({ "type": "json_object" });
        }
        body
    }
}

fn map_reqwest_error(err: reqwest::Error) -> AppError {
    if err.is_timeout() {
        AppError::business(code::LLM_TIMEOUT, "大模型调用超时,请稍后重试或调大超时时间")
    } else {
        AppError::business(code::LLM_FAILED, format!("大模型网络异常: {err}"))
    }
}

/// 5xx / 429 属于可重试错误。
fn is_retryable(message: &str) -> bool {
    for token in ["HTTP 5", "HTTP 429", "网络异常"] {
        if message.contains(token) {
            return true;
        }
    }
    false
}

pub fn abbreviate(text: &str, max: usize) -> String {
    let trimmed = text.trim();
    if trimmed.chars().count() <= max {
        return trimmed.to_string();
    }
    let head: String = trimmed.chars().take(max).collect();
    format!("{head}…")
}

/// 从大模型返回里提取 JSON 对象(容忍 markdown 代码块包裹)。
pub fn extract_json(text: &str) -> AppResult<Value> {
    let cleaned = text.trim();
    let without_fence = strip_code_fence(cleaned);
    if let Ok(value) = serde_json::from_str::<Value>(&without_fence) {
        return Ok(value);
    }
    if let (Some(start), Some(end)) = (without_fence.find('{'), without_fence.rfind('}')) {
        if end > start {
            let slice = &without_fence[start..=end];
            if let Ok(value) = serde_json::from_str::<Value>(slice) {
                return Ok(value);
            }
        }
    }
    Err(AppError::internal(format!(
        "大模型返回的内容不是合法 JSON: {}",
        abbreviate(cleaned, 200)
    )))
}

fn strip_code_fence(text: &str) -> String {
    let trimmed = text.trim();
    if !trimmed.starts_with("```") {
        return trimmed.to_string();
    }
    let without_start = trimmed.trim_start_matches("```").trim_start_matches(|c: char| c.is_alphabetic());
    without_start.trim().trim_end_matches("```").trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_json_from_fenced_block() {
        let raw = "```json\n{\"a\": 1}\n```";
        assert_eq!(extract_json(raw).unwrap()["a"], 1);
    }

    #[test]
    fn extracts_json_with_surrounding_text() {
        let raw = "这是结果: {\"score\": 88} 请查收";
        assert_eq!(extract_json(raw).unwrap()["score"], 88);
    }

    #[test]
    fn long_call_raises_small_budget() {
        let cfg = LlmConfig { max_tokens: 2000, timeout_seconds: 90, ..Default::default() };
        let budget = CallBudget::long_for(&cfg);
        assert_eq!(budget.max_tokens, 8000, "默认 2000 tokens 装不下整份简历,应抬到 8000");
        assert_eq!(budget.timeout, Duration::from_secs(240));
        assert!(!budget.retry_on_timeout, "长任务超时不应反复重试");
    }

    #[test]
    fn long_call_keeps_larger_budget() {
        let cfg = LlmConfig { max_tokens: 16000, timeout_seconds: 600, ..Default::default() };
        let budget = CallBudget::long_for(&cfg);
        assert_eq!(budget.max_tokens, 16000);
        assert_eq!(budget.timeout, Duration::from_secs(600));
    }

    #[test]
    fn url_is_normalized() {
        let mut cfg = LlmConfig { base_url: "https://api.deepseek.com".into(), ..Default::default() };
        assert_eq!(cfg.chat_completions_url(), "https://api.deepseek.com/v1/chat/completions");
        cfg.base_url = "https://api.deepseek.com/v1".into();
        assert_eq!(cfg.chat_completions_url(), "https://api.deepseek.com/v1/chat/completions");
        cfg.base_url = "http://localhost:11434/v1/chat/completions".into();
        assert_eq!(cfg.chat_completions_url(), "http://localhost:11434/v1/chat/completions");
    }
}
