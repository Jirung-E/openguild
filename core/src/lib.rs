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
pub mod events;
pub mod file_mtime;
pub mod health;
pub mod incremental;
pub mod guild_file;
pub mod locale;
pub mod lock;
#[cfg(test)]
mod test_env;
pub mod maintenance;
pub mod merge;
pub mod migrate;
pub mod models;
pub mod ops;
pub mod plugins;
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

/// BUG-358: 시험이 만드는 임시 폴더 이름 뒤에 붙이는 번호 — 프로세스 안에서 하나씩 올라간다.
///
/// 예전엔 `og-<이름표>-<시각 ns>` 로만 구분했다. macOS 의 시각은 마이크로초 단위라, 같은 이름표를 쓰는 두 시험이
/// 병렬로 같은 마이크로초에 시작하면 같은 폴더를 받아 같은 `index.db` 를 함께 열고 잠겼다(CI 에서 가끔
/// `database is locked`). 서버 시험은 같은 이유로 이미 카운터를 붙여 두었다.
#[cfg(test)]
pub(crate) fn test_seq() -> u64 {
    static SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}
