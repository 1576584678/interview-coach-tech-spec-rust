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
pub mod util;

use std::path::Path;

use axum::Router;

use crate::config::{AppConfig, ConfigFile};
use crate::error::AppResult;
use crate::state::{AppState, SharedState};
use crate::store::Store;

/// 本地数据文件名(单文件存储,备份直接拷贝即可)。
pub const DATA_FILE: &str = "interview-coach.json";

/// 根据配置构建共享状态:确保数据目录存在并加载本地数据。
pub fn build_state(config_file: ConfigFile, config: AppConfig) -> AppResult<SharedState> {
    let data_dir = config.data_dir_path();
    std::fs::create_dir_all(&data_dir)?;
    let store = Store::load(data_dir.join(DATA_FILE))?;
    Ok(AppState::new(config_file, config, store))
}

pub fn build_router(state: SharedState, web_dir: impl AsRef<Path>) -> Router {
    routes::router(state, web_dir.as_ref().to_path_buf())
}
