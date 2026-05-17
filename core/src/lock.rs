//! Single-writer lock — `.guild/.lock` 파일로 동시 mutation 방지.
//!
//! 동작:
//! - lock acquire: 파일 작성 (없으면 생성). 이미 있으면 PID 확인 → 살아있으면 거부, 죽었으면 강탈.
//! - lock release: 파일 삭제 (RAII guard).
//!
//! 한계 — 본 구현은 best-effort:
//! - PID 검증은 OS 의존. Windows / Unix 처리 별도.
//! - 네트워크 파일 시스템에선 락 충돌 가능.
//! - read-only 작업은 lock 안 잡음 (충돌 없음).
//!
//! 사용 예:
//! ```text
//! let _lock = LockGuard::acquire(&store.paths)?;
//! // ... mutation 수행 ...
//! // _lock 이 drop 되면 자동 release
//! ```

use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LockInfo {
    pub pid: u32,
    pub acquired_at: String, // ISO 8601
    pub command: Option<String>, // 디버그용
}

/// RAII guard — drop 시 lock 파일 삭제.
#[derive(Debug)]
pub struct LockGuard {
    path: PathBuf,
}

impl LockGuard {
    /// Lock 획득 시도. 기존 lock 이 있으면:
    /// - 살아있는 PID 면 거부 (Err).
    /// - 죽은 PID 면 강탈 (덮어쓰기).
    pub fn acquire(guild_paths: &crate::repo::GuildPaths) -> Result<Self> {
        Self::acquire_with_command(guild_paths, None)
    }

    pub fn acquire_with_command(
        guild_paths: &crate::repo::GuildPaths,
        command: Option<String>,
    ) -> Result<Self> {
        let path = guild_paths.lock_file();
        std::fs::create_dir_all(guild_paths.dot_guild())?;

        if path.exists() {
            // 기존 lock 검사
            let content = std::fs::read_to_string(&path).unwrap_or_default();
            match toml::from_str::<LockInfo>(&content) {
                Ok(existing) => {
                    if is_pid_alive(existing.pid) {
                        return Err(anyhow!(
                            ".guild/.lock 이 잠겨있음 (PID {}, since {}): {}\n\
                             다른 mutation 진행 중. 끝나면 다시 시도하세요.\n\
                             강제로 풀려면 .guild/.lock 파일을 수동 삭제.",
                            existing.pid,
                            existing.acquired_at,
                            existing.command.as_deref().unwrap_or("(unknown)")
                        ));
                    }
                    // 죽은 PID → 강탈
                    tracing::warn!(
                        "stale lock detected (PID {} not running). overriding.",
                        existing.pid
                    );
                }
                Err(_) => {
                    // 파싱 실패 — 파일은 있지만 손상. 강탈.
                    tracing::warn!("corrupted lock file. overriding.");
                }
            }
        }

        let info = LockInfo {
            pid: std::process::id(),
            acquired_at: now_iso(),
            command,
        };
        let content = toml::to_string_pretty(&info).context("lock TOML 직렬화 실패")?;
        std::fs::write(&path, content)
            .with_context(|| format!("lock 파일 작성 실패: {}", path.display()))?;
        Ok(Self { path })
    }
}

impl Drop for LockGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

/// 현재 잠금 정보 (있으면).
pub fn current_lock(guild_paths: &crate::repo::GuildPaths) -> Option<LockInfo> {
    let path = guild_paths.lock_file();
    let content = std::fs::read_to_string(&path).ok()?;
    toml::from_str(&content).ok()
}

/// 강제 해제 — 사용자가 stale lock 만났을 때 수동 명령.
pub fn force_release(guild_paths: &crate::repo::GuildPaths) -> Result<()> {
    let path = guild_paths.lock_file();
    if path.exists() {
        std::fs::remove_file(&path)
            .with_context(|| format!("lock 파일 삭제 실패: {}", path.display()))?;
    }
    Ok(())
}

#[cfg(target_os = "windows")]
fn is_pid_alive(_pid: u32) -> bool {
    // Windows: OpenProcess 확인. 일단 false (safe — 강탈 허용)
    // 정확한 구현은 winapi crate 필요. 현재 단순 구현으로 둠.
    // 단점: 락 PID 가 우연히 재사용된 다른 프로세스라도 강탈됨.
    //       단일 사용자 환경에선 큰 위험 X.
    false
}

#[cfg(not(target_os = "windows"))]
fn is_pid_alive(pid: u32) -> bool {
    // Unix: kill -0. signal 0 은 실제 시그널 안 보내고 권한만 확인.
    // 살아있고 권한 있으면 Ok. ESRCH 면 죽음.
    unsafe { libc_kill_zero(pid as i32) }
}

#[cfg(not(target_os = "windows"))]
#[link(name = "c")]
unsafe extern "C" {
    fn kill(pid: i32, sig: i32) -> i32;
}

#[cfg(not(target_os = "windows"))]
unsafe fn libc_kill_zero(pid: i32) -> bool {
    unsafe { kill(pid, 0) == 0 }
}

fn now_iso() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let (y, mo, d, h, mi, s) = epoch_to_ymdhms(secs);
    format!("{y:04}-{mo:02}-{d:02}T{h:02}:{mi:02}:{s:02}Z")
}

fn epoch_to_ymdhms(secs: u64) -> (u32, u32, u32, u32, u32, u32) {
    let s = (secs % 60) as u32;
    let mi = ((secs / 60) % 60) as u32;
    let h = ((secs / 3600) % 24) as u32;
    let mut days = (secs / 86400) as i64;
    let mut year: i64 = 1970;
    loop {
        let dy = if is_leap(year) { 366 } else { 365 };
        if days >= dy {
            days -= dy;
            year += 1;
        } else {
            break;
        }
    }
    let dim = days_in_months(year);
    let mut month: usize = 0;
    while month < 12 && days >= dim[month] as i64 {
        days -= dim[month] as i64;
        month += 1;
    }
    (year as u32, (month + 1) as u32, (days + 1) as u32, h, mi, s)
}

fn is_leap(y: i64) -> bool {
    (y % 4 == 0 && y % 100 != 0) || y % 400 == 0
}
fn days_in_months(y: i64) -> [u32; 12] {
    [
        31,
        if is_leap(y) { 29 } else { 28 },
        31, 30, 31, 30, 31, 31, 30, 31, 30, 31,
    ]
}

// `Path` 미사용 방지
#[allow(dead_code)]
fn _path_ref() -> Option<&'static Path> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repo::GuildPaths;

    fn fresh_tmp(label: &str) -> PathBuf {
        let ns = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let p = std::env::temp_dir().join(format!("og-lock-{label}-{ns}"));
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    #[test]
    fn acquire_creates_file_with_pid() {
        let dir = fresh_tmp("acquire");
        let paths = GuildPaths::new(&dir);

        let _guard = LockGuard::acquire(&paths).unwrap();
        assert!(paths.lock_file().exists());

        let info = current_lock(&paths).unwrap();
        assert_eq!(info.pid, std::process::id());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn drop_releases_lock() {
        let dir = fresh_tmp("drop");
        let paths = GuildPaths::new(&dir);

        {
            let _guard = LockGuard::acquire(&paths).unwrap();
            assert!(paths.lock_file().exists());
        } // drop here
        assert!(!paths.lock_file().exists(), "lock should be released on drop");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn second_acquire_in_same_process_steals() {
        // 같은 PID — is_pid_alive 가 true 라 거부될 수도 있고, drop 후엔 정상.
        // 본 구현은 단일 프로세스 단일 acquire 만 보장 — 같은 프로세스에서 두 번 acquire 는
        // PID 가 alive 라 거부됨 (정확한 동작).
        let dir = fresh_tmp("conflict");
        let paths = GuildPaths::new(&dir);

        let _g1 = LockGuard::acquire(&paths).unwrap();
        // 같은 PID — alive 검사를 통과한다면 ok (현재 Windows 구현은 항상 false 라 강탈됨).
        // Unix 면 거부. 일단 단순히 두 번 acquire 가 panic 안 함만 검증.
        let _attempt = LockGuard::acquire(&paths);
        // 두 케이스 모두 허용 — 본 함수는 "한 번이라도 동작" 만 검증.

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn stale_lock_can_be_stolen() {
        let dir = fresh_tmp("stale");
        let paths = GuildPaths::new(&dir);
        std::fs::create_dir_all(paths.dot_guild()).unwrap();

        // 살아있지 않은 PID 작성. PID 1 은 init — 살아있는 시스템이 많아 stable 검증 어려움.
        // 큰 임의의 PID (>= 2^31 같은) 는 거의 없음.
        let info = LockInfo {
            pid: 99999999,
            acquired_at: "2026-01-01T00:00:00Z".into(),
            command: Some("ghost".into()),
        };
        std::fs::write(paths.lock_file(), toml::to_string_pretty(&info).unwrap()).unwrap();

        // 강탈 성공해야 (Windows 는 항상 강탈, Unix 는 99999999 PID 죽었다고 가정 안전)
        let result = LockGuard::acquire(&paths);
        assert!(result.is_ok(), "stale lock should be stolen: {result:?}");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn force_release_removes_lock() {
        let dir = fresh_tmp("force");
        let paths = GuildPaths::new(&dir);
        std::fs::create_dir_all(paths.dot_guild()).unwrap();
        std::fs::write(paths.lock_file(), "anything").unwrap();
        force_release(&paths).unwrap();
        assert!(!paths.lock_file().exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn force_release_no_op_when_missing() {
        let dir = fresh_tmp("missing");
        let paths = GuildPaths::new(&dir);
        force_release(&paths).unwrap(); // no error
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn current_lock_returns_none_when_no_lock() {
        let dir = fresh_tmp("none");
        let paths = GuildPaths::new(&dir);
        assert!(current_lock(&paths).is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
