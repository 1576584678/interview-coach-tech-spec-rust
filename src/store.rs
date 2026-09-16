//! 本地存储:所有数据放在一个 JSON 文件里(data/interview-coach.json),
//! 单机版不需要数据库,备份/迁移直接拷贝该文件即可。

use std::path::{Path, PathBuf};

use parking_lot::RwLock;

use crate::config::write_atomic;
use crate::error::{AppError, AppResult};
use crate::models::Database;

pub struct Store {
    path: PathBuf,
    db: RwLock<Database>,
}

impl Store {
    pub fn load(path: impl Into<PathBuf>) -> AppResult<Self> {
        let path = path.into();
        let db = if path.exists() {
            let raw = std::fs::read_to_string(&path)?;
            if raw.trim().is_empty() {
                Database::default()
            } else {
                serde_json::from_str(&raw).map_err(|e| {
                    AppError::internal(format!("本地数据文件损坏({}): {e}", path.display()))
                })?
            }
        } else {
            Database::default()
        };
        Ok(Self { path, db: RwLock::new(db) })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// 只读访问。
    pub fn read<T>(&self, f: impl FnOnce(&Database) -> T) -> T {
        let guard = self.db.read();
        f(&guard)
    }

    /// 修改并在成功返回后落盘;闭包返回 Err 时不写入文件。
    pub fn write<T>(&self, f: impl FnOnce(&mut Database) -> AppResult<T>) -> AppResult<T> {
        let mut guard = self.db.write();
        // 先克隆一份快照,失败时回滚内存状态,避免内存与磁盘不一致。
        let mut working = guard.clone();
        let result = f(&mut working)?;
        self.persist(&working)?;
        *guard = working;
        Ok(result)
    }

    fn persist(&self, db: &Database) -> AppResult<()> {
        let text = serde_json::to_string_pretty(db)?;
        write_atomic(&self.path, text.as_bytes())
    }
}
