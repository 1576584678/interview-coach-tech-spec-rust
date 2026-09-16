//! 路由装配:API 前缀 `/api`,其余路径交给前端静态资源(SPA)。

pub mod health;
pub mod interview;
pub mod plan;
pub mod resume;
pub mod salary;
pub mod settings;

use std::path::PathBuf;

use axum::routing::{delete, get, post};
use axum::Router;
use tower_http::cors::CorsLayer;
use tower_http::services::{ServeDir, ServeFile};
use tower_http::trace::TraceLayer;

use crate::state::SharedState;

/// `/api` 下的未知路径返回 JSON 404,而不是回落到前端页面。
async fn api_not_found() -> crate::error::AppError {
    crate::error::AppError::not_found("接口不存在")
}

pub fn router(state: SharedState, web_dir: PathBuf) -> Router {
    let api = Router::new()
        .route("/health", get(health::health))
        .route("/meta", get(settings::meta))
        .route("/config", get(settings::get_config).put(settings::update_config))
        .route("/config/test", post(settings::test_llm))
        .route("/interview/start", post(interview::start))
        .route("/interview/history", get(interview::history))
        .route("/interview/stats", get(interview::stats))
        .route("/interview/improvement-plan", get(plan::get_plan))
        .route("/interview/improvement-plan/regenerate", post(plan::regenerate))
        .route("/interview/:id/answer", post(interview::answer))
        .route("/interview/:id/answer/stream", post(interview::answer_stream))
        .route("/interview/:id/skip", post(interview::skip))
        .route("/interview/:id/complete", post(interview::complete))
        .route("/interview/:id/abandon", post(interview::abandon))
        .route("/interview/:id/detail", get(interview::detail))
        .route("/interview/:id/result", get(interview::result))
        .route("/interview/:id/review/retry", post(interview::retry_review))
        .route("/interview/:id", delete(interview::delete_session))
        .route("/resume/upload", post(resume::upload))
        .route("/resume/text", post(resume::create_from_text))
        .route("/resume/list", get(resume::list))
        .route("/resume/:id", get(resume::detail).delete(resume::delete))
        .route("/resume/:id/diagnosis", post(resume::diagnose))
        .route("/resume/:id/optimize", post(resume::optimize))
        .route("/resume/:id/star", post(resume::star))
        .route("/salary/estimate", get(salary::estimate))
        .with_state(state)
        .fallback(api_not_found);

    let index = web_dir.join("index.html");
    // 前端是 hash 路由,未知路径一律回落到 index.html(直接访问 /settings 也能打开)
    let static_service = ServeDir::new(&web_dir).fallback(ServeFile::new(index));

    Router::new()
        .nest("/api", api)
        .fallback_service(static_service)
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
}
