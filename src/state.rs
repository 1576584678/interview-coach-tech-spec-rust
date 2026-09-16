//! 进程内共享状态。

use std::sync::Arc;

use parking_lot::RwLock;

use crate::config::{AppConfig, ConfigFile};
use crate::error::AppResult;
use crate::llm::LlmClient;
use crate::question_bank::QuestionBank;
use crate::salary::SalaryPlanner;
use crate::store::Store;

pub struct AppState {
    config_file: ConfigFile,
    config: RwLock<AppConfig>,
    pub store: Store,
    pub question_bank: QuestionBank,
    pub salary: SalaryPlanner,
}

pub type SharedState = Arc<AppState>;

impl AppState {
    pub fn new(config_file: ConfigFile, config: AppConfig, store: Store) -> Arc<Self> {
        Arc::new(Self {
            config_file,
            config: RwLock::new(config),
            store,
            question_bank: QuestionBank::embedded(),
            salary: SalaryPlanner::embedded(),
        })
    }

    pub fn config_snapshot(&self) -> AppConfig {
        self.config.read().clone()
    }

    pub fn llm(&self) -> AppResult<LlmClient> {
        LlmClient::new(self.config.read().llm.clone())
    }

    pub fn config_path(&self) -> String {
        self.config_file.path().display().to_string()
    }

    /// 保存配置到 config.toml 并热更新内存中的配置(无需重启进程)。
    pub fn update_config(&self, new_config: AppConfig) -> AppResult<()> {
        new_config.validate()?;
        self.config_file.save(&new_config)?;
        *self.config.write() = new_config;
        Ok(())
    }
}
