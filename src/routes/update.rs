//! 应用更新接口:检查新版本、一键下载并就地替换本地程序。

use axum::extract::State;
use axum::Json;

use crate::api::ApiBody;
use crate::error::{code, AppError, AppResult};
use crate::state::SharedState;
use crate::updater::{self, ApplyResult, UpdateInfo};

/// 查询 GitHub 上最新版本,和本机版本比较。
pub async fn check() -> AppResult<Json<ApiBody<UpdateInfo>>> {
    Ok(Json(ApiBody::ok(updater::check().await?)))
}

/// 下载并就地更新(同一时刻只允许一个更新任务)。
///
/// 版本判断放在服务端做,前端拿不到「任意下载地址」的能力。
pub async fn apply(State(state): State<SharedState>) -> AppResult<Json<ApiBody<ApplyResult>>> {
    let _guard = state.update_lock.lock().await;
    let info = updater::check().await?;
    if !info.has_update {
        return Err(AppError::business(
            code::BAD_REQUEST,
            format!("已是最新版本 {}", info.current_version),
        ));
    }
    if !info.supported {
        return Err(AppError::business(code::BAD_REQUEST, info.message.clone()));
    }
    Ok(Json(ApiBody::ok(updater::download_and_apply(&info).await?)))
}
