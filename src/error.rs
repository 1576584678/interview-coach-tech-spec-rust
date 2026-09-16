//! 统一错误类型与 HTTP 状态映射。

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;

use crate::api::ApiBody;

/// 业务错误码(0 = 成功,非 0 = 错误)。
pub mod code {
    pub const BAD_REQUEST: i32 = 400;
    pub const NOT_FOUND: i32 = 404;
    pub const CONFLICT: i32 = 409;
    pub const INTERNAL: i32 = 500;
    /// 大模型调用失败(HTTP 层错误)。
    pub const LLM_FAILED: i32 = 1003;
    /// 大模型调用超时。
    pub const LLM_TIMEOUT: i32 = 1004;
    /// 大模型返回内容为空。
    pub const LLM_EMPTY: i32 = 1005;
    /// 面试已结束。
    pub const INTERVIEW_FINISHED: i32 = 1006;
    /// 配置缺失或非法。
    pub const CONFIG_ERROR: i32 = 1007;
    /// 文件解析失败。
    pub const FILE_PARSE_FAILED: i32 = 1008;
}

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("{message}")]
    Business { code: i32, message: String },
    #[error("服务内部错误: {0}")]
    Internal(String),
    #[error("读取或写入本地数据失败: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON 解析失败: {0}")]
    Json(#[from] serde_json::Error),
    #[error("配置文件解析失败: {0}")]
    Toml(#[from] toml::de::Error),
    #[error("网络请求失败: {0}")]
    Http(#[from] reqwest::Error),
}

pub type AppResult<T> = Result<T, AppError>;

impl AppError {
    pub fn business(code: i32, message: impl Into<String>) -> Self {
        AppError::Business { code, message: message.into() }
    }

    pub fn bad_request(message: impl Into<String>) -> Self {
        AppError::business(code::BAD_REQUEST, message)
    }

    pub fn not_found(message: impl Into<String>) -> Self {
        AppError::business(code::NOT_FOUND, message)
    }

    pub fn internal(message: impl Into<String>) -> Self {
        AppError::Internal(message.into())
    }

    /// 业务错误码;内部错误统一 500。
    pub fn code(&self) -> i32 {
        match self {
            AppError::Business { code, .. } => *code,
            _ => code::INTERNAL,
        }
    }

    pub fn message(&self) -> String {
        match self {
            AppError::Business { message, .. } => message.clone(),
            AppError::Internal(msg) => msg.clone(),
            other => other.to_string(),
        }
    }

    /// HTTP 状态码:仅常见的客户端错误透传,其余按参考实现返回 200 + 业务码,
    /// 便于前端统一处理(与 Java 版 GlobalExceptionHandler 行为一致)。
    pub fn status(&self) -> StatusCode {
        match self.code() {
            400 => StatusCode::BAD_REQUEST,
            403 => StatusCode::FORBIDDEN,
            404 => StatusCode::NOT_FOUND,
            429 => StatusCode::TOO_MANY_REQUESTS,
            code::INTERNAL => StatusCode::INTERNAL_SERVER_ERROR,
            _ => StatusCode::OK,
        }
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let status = self.status();
        let body = Json(ApiBody::<()>::fail(self.code(), self.message()));
        (status, body).into_response()
    }
}
