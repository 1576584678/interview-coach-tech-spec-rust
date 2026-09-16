//! 面试教练(单机版)核心库:无用户体系、无数据库、无 Redis,
//! 数据全部保存在本地 JSON 文件中,大模型接口可自由配置。

pub mod analysis;
pub mod api;
pub mod browser;
pub mod config;
pub mod error;
pub mod file_parser;
pub mod interview_service;
pub mod llm;
pub mod models;
pub mod prompt;
pub mod question_bank;
pub mod resume_service;
pub mod review_service;
pub mod routes;
pub mod salary;
pub mod state;
pub mod store;
pub mod updater;
pub mod util;

use std::path::Path;

use axum::Router;

use crate::config::{AppConfig, ConfigFile};
use crate::error::AppResult;
use crate::state::{AppState, SharedState};
use crate::store::Store;

/// 本地数据文件名(单文件存储,备份直接拷贝即可)。
pub const DATA_FILE: &str = "interview-coach.json";

/// 根据配置构建共享状态:确保数据目录存在、加载本地数据,并修复上次异常退出留下的状态。
pub fn build_state(config_file: ConfigFile, config: AppConfig) -> AppResult<SharedState> {
    let data_dir = config.data_dir_path();
    std::fs::create_dir_all(&data_dir)?;
    let store = Store::load(data_dir.join(DATA_FILE))?;
    reset_stale_reviews(&store)?;
    // 文件基线:保存设置时以它为准,避免把命令行/环境变量的临时覆盖写回 config.toml
    let file_baseline = config_file.load().unwrap_or_else(|_| config.clone());
    Ok(AppState::new(config_file, config, file_baseline, store))
}

/// 复盘任务只存在于内存中:进程重启后仍停在 `processing` 的会话说明那次生成已经中断。
/// 这里把它复位,否则前端会永远停在「复盘生成中…」并且无法重新触发。
fn reset_stale_reviews(store: &Store) -> AppResult<()> {
    let stale: Vec<u64> = store.read(|db| {
        db.sessions
            .iter()
            .filter(|session| {
                session.review.is_none()
                    && session.review_status.as_deref() == Some(crate::models::review_status::PROCESSING)
            })
            .map(|session| session.id)
            .collect()
    });
    if stale.is_empty() {
        return Ok(());
    }
    store.write(move |db| {
        for session in db.sessions.iter_mut() {
            if stale.contains(&session.id) {
                session.review_status = None;
                session.review_error = None;
            }
        }
        Ok(())
    })
}

pub fn build_router(state: SharedState, web_dir: impl AsRef<Path>) -> Router {
    routes::router(state, web_dir.as_ref().to_path_buf())
}
