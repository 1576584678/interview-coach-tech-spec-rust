//! 健康检查。

use axum::extract::State;
use axum::Json;

use crate::api::ApiBody;
use crate::state::SharedState;

pub async fn health(State(state): State<SharedState>) -> Json<ApiBody<serde_json::Value>> {
    let config = state.config_snapshot();
    Json(ApiBody::ok(serde_json::json!({
        "status": "ok",
        "version": env!("CARGO_PKG_VERSION"),
        "llmConfigured": config.llm.configured(),
        "model": config.llm.model,
        "dataFile": state.store.path().display().to_string(),
        "configFile": state.config_path(),
    })))
}
