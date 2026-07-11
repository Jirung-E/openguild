//! openguild core — 모든 비즈니스 로직과 sqlx 접근의 단일 진실 소스.
//!
//! 사용자:
//! - `server` (HTTP wrapper)
//! - `cli` (로컬 모드)
//! - `desktop` (Tauri invoke handler)
//!
//! 이 crate 는 HTTP / stdin/stdout / GUI 인터페이스 어느 것도 모른다 —
//! 순수히 도메인 로직 + 저장소.

pub mod counter;
pub mod db;
pub mod drift;
pub mod error;
pub mod file_mtime;
pub mod health;
pub mod incremental;
pub mod guild_file;
pub mod lock;
pub mod maintenance;
pub mod migrate;
pub mod models;
pub mod ops;
pub mod recents;
pub mod reindex;
pub mod replay;
pub mod repo;
pub mod services;
pub mod snapshot;
pub mod store;
pub mod time;
pub mod user_dirs;

pub use error::{AppError, AppResult};
pub use store::Store;
