//! BUG-250: 테스트에서 **프로세스 전역 env** 를 건드릴 때 쓰는 단일 잠금.
//!
//! `std::env::set_var` / `remove_var` 는 프로세스 전체에 즉시 반영되고,
//! 멀티스레드에서 다른 스레드가 `getenv` 하는 중이면 **정의되지 않은 동작**이다
//! (그래서 최신 Rust 에서 `unsafe` 다). `cargo test` 는 기본이 병렬 실행이라
//! 테스트 스레드들이 그 조건에 정확히 해당한다.
//!
//! 예전엔 파일마다 **각자의 잠금**을 뒀다 — `recents`, `user_dirs`, `locale` 이
//! 서로 다른 Mutex 를, `snapshot` 은 아예 잠금 없이("테스트 단일 스레드" 라는
//! 사실과 다른 주석과 함께) env 를 건드렸다. 파일 안에서는 직렬화되지만
//! **파일 사이에서는 자유롭게 겹쳐서**, 예컨대 `locale` 이 `OPENGUILD_HOME` 을
//! 세우는 동안 `recents` 테스트가 env 를 읽는 창이 생긴다.
//!
//! 증상은 간헐 실패였다 — `resolve_guild_ref_resolves_by_name_from_recents` 가
//! 수십 회에 한 번 깨졌고, 머신이 바쁠수록(다른 앱 실행 등) 잦아졌다.
//! 잠금을 하나로 합쳐 env 를 만지는 모든 테스트를 한 줄로 세운다.

/// env 를 건드리는 **모든** 테스트가 공유하는 잠금.
///
/// 한 테스트가 panic 해도 다른 테스트가 깨지지 않도록 `PoisonError` 는
/// 흡수한다(지키는 데이터가 `()` 라 오염될 상태가 없다).
pub(crate) fn env_lock() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: std::sync::OnceLock<std::sync::Mutex<()>> = std::sync::OnceLock::new();
    LOCK.get_or_init(|| std::sync::Mutex::new(()))
        .lock()
        .unwrap_or_else(|e| e.into_inner())
}
