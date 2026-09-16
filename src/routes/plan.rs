//! 个性化提升计划接口。

use axum::extract::State;
use axum::Json;

use crate::api::ApiBody;
use crate::error::AppResult;
use crate::models::ImprovementPlan;
use crate::state::SharedState;

pub async fn get_plan(
    State(state): State<SharedState>,
) -> AppResult<Json<ApiBody<ImprovementPlan>>> {
    let plan = crate::analysis::improvement_plan(&state, false).await?;
    Ok(Json(ApiBody::ok(plan)))
}

pub async fn regenerate(
    State(state): State<SharedState>,
) -> AppResult<Json<ApiBody<ImprovementPlan>>> {
    let plan = crate::analysis::improvement_plan(&state, true).await?;
    Ok(Json(ApiBody::ok(plan)))
}
