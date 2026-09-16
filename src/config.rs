//! 单机版配置:优先读 `config.toml`,环境变量(.env 或系统环境)可覆盖。

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{code, AppError, AppResult};

pub const DEFAULT_CONFIG_FILE: &str = "config.toml";
pub const DEFAULT_DATA_DIR: &str = "data";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AppConfig {
    pub server: ServerConfig,
    pub llm: LlmConfig,
    pub interview: InterviewConfig,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            server: ServerConfig::default(),
            llm: LlmConfig::default(),
            interview: InterviewConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
    /// 启动后自动打开浏览器(单机版默认开启)。
    pub auto_open_browser: bool,
    /// 本地数据目录(配置、会话、简历等 JSON 文件)。
    pub data_dir: String,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            host: "127.0.0.1".to_string(),
            port: 8080,
            auto_open_browser: true,
            data_dir: DEFAULT_DATA_DIR.to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct LlmConfig {
    /// OpenAI 兼容接口地址,例如 https://api.deepseek.com
    pub base_url: String,
    pub api_key: String,
    pub model: String,
    pub temperature: f32,
    pub max_tokens: u32,
    pub timeout_seconds: u64,
}

impl Default for LlmConfig {
    fn default() -> Self {
        Self {
            base_url: "https://api.deepseek.com".to_string(),
            api_key: String::new(),
            model: "deepseek-chat".to_string(),
            temperature: 0.7,
            max_tokens: 8000,
            timeout_seconds: 240,
        }
    }
}

impl LlmConfig {
    /// 拼出 chat/completions 的完整地址,兼容填写 `/v1`、根地址或完整地址三种写法。
    pub fn chat_completions_url(&self) -> String {
        let base = self.base_url.trim().trim_end_matches('/');
        if base.is_empty() {
            return String::new();
        }
        if base.ends_with("/chat/completions") {
            base.to_string()
        } else if base.ends_with("/v1") || base.ends_with("/v4") {
            // 智谱等厂商用 /v4/chat/completions,OpenAI 兼容用 /v1
            format!("{base}/chat/completions")
        } else {
            format!("{base}/v1/chat/completions")
        }
    }

    pub fn configured(&self) -> bool {
        !self.base_url.trim().is_empty() && !self.api_key.trim().is_empty() && !self.model.trim().is_empty()
    }

    /// 用于接口返回的脱敏 API Key。
    pub fn masked_api_key(&self) -> String {
        let key = self.api_key.trim();
        if key.is_empty() {
            return String::new();
        }
        let chars: Vec<char> = key.chars().collect();
        if chars.len() <= 8 {
            return "*".repeat(chars.len());
        }
        let head: String = chars.iter().take(4).collect();
        let tail: String = chars.iter().skip(chars.len() - 4).collect();
        format!("{head}****{tail}")
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct InterviewConfig {
    /// 面试总题数(参考实现为 11 题)。
    pub total_questions: usize,
    pub default_position_category: String,
    pub default_difficulty: String,
    pub default_style: String,
    pub default_mode: String,
}

impl Default for InterviewConfig {
    fn default() -> Self {
        Self {
            total_questions: 11,
            default_position_category: "backend".to_string(),
            default_difficulty: "normal".to_string(),
            default_style: "friendly".to_string(),
            default_mode: "normal".to_string(),
        }
    }
}

impl AppConfig {
    pub fn data_dir_path(&self) -> PathBuf {
        PathBuf::from(self.server.data_dir.trim())
    }

    /// 环境变量覆盖(未配置的环境变量不生效)。
    pub fn apply_env_overrides(&mut self) {
        if let Ok(host) = std::env::var("SERVER_HOST") {
            if !host.trim().is_empty() {
                self.server.host = host;
            }
        }
        if let Some(port) = env_parse::<u16>("SERVER_PORT") {
            self.server.port = port;
        }
        if let Some(auto) = env_parse::<bool>("AUTO_OPEN_BROWSER") {
            self.server.auto_open_browser = auto;
        }
        if let Ok(dir) = std::env::var("DATA_DIR") {
            if !dir.trim().is_empty() {
                self.server.data_dir = dir;
            }
        }
        if let Ok(url) = std::env::var("LLM_BASE_URL") {
            if !url.trim().is_empty() {
                self.llm.base_url = url;
            }
        }
        if let Ok(key) = std::env::var("LLM_API_KEY") {
            if !key.trim().is_empty() {
                self.llm.api_key = key;
            }
        }
        if let Ok(model) = std::env::var("LLM_MODEL") {
            if !model.trim().is_empty() {
                self.llm.model = model;
            }
        }
        if let Some(timeout) = env_parse::<u64>("LLM_TIMEOUT_SECONDS") {
            self.llm.timeout_seconds = timeout;
        }
        if let Some(temperature) = env_parse::<f32>("LLM_TEMPERATURE") {
            self.llm.temperature = temperature;
        }
        if let Some(max_tokens) = env_parse::<u32>("LLM_MAX_TOKENS") {
            self.llm.max_tokens = max_tokens;
        }
        if let Some(total) = env_parse::<usize>("INTERVIEW_TOTAL_QUESTIONS") {
            self.interview.total_questions = total;
        }
    }

    pub fn validate(&self) -> AppResult<()> {
        if self.interview.total_questions == 0 || self.interview.total_questions > 30 {
            return Err(AppError::business(code::CONFIG_ERROR, "面试题数需在 1-30 之间"));
        }
        if !(0.0..=2.0).contains(&self.llm.temperature) {
            return Err(AppError::business(code::CONFIG_ERROR, "temperature 需在 0-2 之间"));
        }
        if self.llm.max_tokens == 0 || self.llm.max_tokens > 32_000 {
            return Err(AppError::business(code::CONFIG_ERROR, "max_tokens 需在 1-32000 之间"));
        }
        if self.llm.timeout_seconds < 5 || self.llm.timeout_seconds > 600 {
            return Err(AppError::business(code::CONFIG_ERROR, "超时时间需在 5-600 秒之间"));
        }
        Ok(())
    }
}

fn env_parse<T: std::str::FromStr>(name: &str) -> Option<T> {
    std::env::var(name).ok().and_then(|raw| raw.trim().parse::<T>().ok())
}

/// 配置文件的读写。
#[derive(Debug, Clone)]
pub struct ConfigFile {
    path: PathBuf,
}

impl ConfigFile {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    pub fn from_env() -> Self {
        let path = std::env::var("CONFIG_FILE")
            .ok()
            .filter(|v| !v.trim().is_empty())
            .unwrap_or_else(|| DEFAULT_CONFIG_FILE.to_string());
        Self::new(path)
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// 读取配置;文件不存在时返回默认配置(不落盘,由调用方决定是否保存)。
    pub fn load(&self) -> AppResult<AppConfig> {
        if !self.path.exists() {
            return Ok(AppConfig::default());
        }
        let raw = std::fs::read_to_string(&self.path)?;
        if raw.trim().is_empty() {
            return Ok(AppConfig::default());
        }
        let cfg: AppConfig = toml::from_str(&raw)
            .map_err(|e| AppError::business(code::CONFIG_ERROR, format!("配置文件解析失败: {e}")))?;
        Ok(cfg)
    }

    pub fn save(&self, cfg: &AppConfig) -> AppResult<()> {
        if let Some(parent) = self.path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)?;
            }
        }
        let text = toml::to_string_pretty(cfg)
            .map_err(|e| AppError::internal(format!("配置序列化失败: {e}")))?;
        write_atomic(&self.path, text.as_bytes())
    }
}

/// 原子写文件:先写临时文件再 rename,避免中途崩溃损坏数据。
pub fn write_atomic(path: &Path, bytes: &[u8]) -> AppResult<()> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}
