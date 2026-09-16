//! 统一响应体:`{ code, message, data }`。

use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct ApiBody<T> {
    pub code: i32,
    pub message: String,
    pub data: Option<T>,
}

impl<T> ApiBody<T> {
    pub fn ok(data: T) -> Self {
        Self { code: 0, message: "success".to_string(), data: Some(data) }
    }

    pub fn fail(code: i32, message: impl Into<String>) -> Self {
        Self { code, message: message.into(), data: None }
    }
}

impl ApiBody<()> {
    pub fn empty() -> Self {
        Self { code: 0, message: "success".to_string(), data: None }
    }
}

/// 空 data 的成功响应(用于 delete/complete 之类无返回值的接口)。
pub fn ok_empty() -> axum::Json<ApiBody<()>> {
    axum::Json(ApiBody::empty())
}
