pub mod associations;
pub mod engine;
pub mod extensions;
pub mod learning;
pub mod memory;
pub mod model;
pub mod platform;
pub mod policy;
pub mod recommendation;
pub mod runtime;
pub mod startup;
pub mod store;
pub mod usage_score;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("数据库操作失败：{0}")]
    Database(#[from] rusqlite::Error),
    #[error("数据格式无效：{0}")]
    Json(#[from] serde_json::Error),
    #[error("{0}")]
    Rule(String),
}
pub type Result<T> = std::result::Result<T, Error>;
pub(crate) fn rule(message: &str) -> Error {
    Error::Rule(message.into())
}

pub mod website;

pub mod browser_bridge;

pub mod adjustment;
pub mod experience;
pub mod quick_words;
