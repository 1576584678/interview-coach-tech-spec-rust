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
    /// 本进程实际生效的配置(含命令行与环境变量覆盖)。
    config: RwLock<AppConfig>,
    /// 配置文件里的原始内容(不含进程级覆盖),保存设置时以它为基准。
    file_baseline: RwLock<AppConfig>,
    pub store: Store,
    pub question_bank: QuestionBank,
    pub salary: SalaryPlanner,
}

pub type SharedState = Arc<AppState>;

impl AppState {
    pub fn new(
        config_file: ConfigFile,
        config: AppConfig,
        file_baseline: AppConfig,
        store: Store,
    ) -> Arc<Self> {
        Arc::new(Self {
            config_file,
            config: RwLock::new(config),
            file_baseline: RwLock::new(file_baseline),
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
    ///
    /// `host` / `port` / `data_dir` 只会由启动参数或环境变量改变(设置页不提供这几个字段),
    /// 因此写文件时一律取文件里的原值,避免一次「保存配置」就把临时的 `--port`、
    /// `--data-dir` 永久写进 config.toml;内存里仍保留本进程实际生效的值。
    pub fn update_config(&self, mut new_config: AppConfig) -> AppResult<()> {
        {
            let baseline = self.file_baseline.read();
            new_config.server.host = baseline.server.host.clone();
            new_config.server.port = baseline.server.port;
            new_config.server.data_dir = baseline.server.data_dir.clone();
        }
        new_config.validate()?;
        self.config_file.save(&new_config)?;

        let current = self.config.read();
        let mut effective = new_config.clone();
        effective.server.host = current.server.host.clone();
        effective.server.port = current.server.port;
        effective.server.data_dir = current.server.data_dir.clone();
        drop(current);

        *self.file_baseline.write() = new_config;
        *self.config.write() = effective;
        Ok(())
    }
}
