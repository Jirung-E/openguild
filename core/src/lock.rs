//! BUG-287: 길드 변경 잠금 — **프로세스 사이**와 **프로세스 안**을 함께 직렬화한다.
//!
//! `.guild` 쓰기는 대부분 "파일 전체 읽기 → 고치기 → 통째로 쓰기" 다. 데스크톱 앱과
//! 에이전트의 CLI 가 같은 퀘스트를 동시에 고치면 먼저 쓴 쪽이 조용히 지워졌다
//! (태그 12개 중 1개, 댓글 13개 중 4개 생존 — 전부 종료 코드 0).
//!
//! 두 층:
//!
//! | 층 | 무엇 | 왜 |
//! |---|---|---|
//! | 프로세스 안 | `Store::write_lock` (tokio Mutex) | 서버의 동시 요청이 blocking 스레드를 줄 세워 점유하지 않게 먼저 async 로 기다린다 |
//! | 프로세스 사이 | `.guild/.lock` 에 OS 권고 잠금 (`File::try_lock`) | unix `flock` / Windows `LockFileEx` 를 표준이 감싼다 — 의존성도 플랫폼 분기도 없다 |
//!
//! 잠금을 쥔 채 프로세스가 죽어도 OS 가 파일 기술자와 함께 푼다 — 예전 PID 파일
//! 방식처럼 "살아 있나" 를 추측할 일이 없다.
//!
//! **공개 변경 진입점에서만 잡는다.** 둘 다 재진입이 안 되므로, 진입점이 다른 진입점을
//! 부르면 제자리에서 멈춘다. 공유하는 몸통은 잠금 없는 내부 함수로 뺀다
//! (`ops::lock_coverage` 시험이 분류를 강제한다).

use anyhow::{Context, Result, anyhow};
use std::fs::{File, OpenOptions, TryLockError};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// 다른 프로세스가 이만큼 놓지 않으면 포기한다. 변경은 사람 속도이고 가장 긴 것이
/// 동기 자동 스냅샷(~2초)이라, 이 시한에 걸리면 멈춘 프로세스가 있다는 뜻이다.
const WAIT: Duration = Duration::from_secs(60);

/// 쥐고 있는 동안 이 길드의 다른 변경은 기다린다. drop 하면 푼다.
///
/// 필드 선언 순서가 drop 순서다 — 파일 잠금을 먼저 놓고 프로세스 안 잠금을 놓는다.
#[must_use = "잠금은 쥐고 있는 동안만 유효하다 — `let _g = ...` 로 묶어 둘 것"]
pub struct MutationGuard {
    _file: Option<File>,
    _local: tokio::sync::OwnedMutexGuard<()>,
}

impl std::fmt::Debug for MutationGuard {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MutationGuard")
            .field("cross_process", &self._file.is_some())
            .finish()
    }
}

pub(crate) async fn acquire(
    local: std::sync::Arc<tokio::sync::Mutex<()>>,
    path: PathBuf,
) -> Result<MutationGuard> {
    let local = local.lock_owned().await;
    let file = tokio::task::spawn_blocking(move || lock_file(&path, WAIT))
        .await
        .context("잠금 대기 작업이 중단됨")??;
    Ok(MutationGuard {
        _file: file,
        _local: local,
    })
}

/// `None` 이면 이 파일시스템이 권고 잠금을 지원하지 않는다(일부 네트워크 공유) —
/// 쓰기를 전부 막는 대신 프로세스 안 보호만으로 진행한다.
fn lock_file(path: &Path, wait: Duration) -> Result<Option<File>> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)
            .with_context(|| format!("잠금 디렉토리 생성 실패: {}", dir.display()))?;
    }
    // 내용은 쓰지 않는다 — 잠금은 파일이 아니라 열린 기술자에 걸린다. 지우지도 않는다:
    // 지운 뒤 다른 프로세스가 새로 만들면 서로 다른 inode 를 잠가 둘 다 통과한다.
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(path)
        .with_context(|| format!("잠금 파일 열기 실패: {}", path.display()))?;

    let deadline = Instant::now() + wait;
    let mut pause = Duration::from_millis(2);
    loop {
        match file.try_lock() {
            Ok(()) => return Ok(Some(file)),
            Err(TryLockError::WouldBlock) => {}
            Err(TryLockError::Error(e)) if e.kind() == std::io::ErrorKind::Unsupported => {
                tracing::warn!(
                    "이 파일시스템은 잠금을 지원하지 않음 — 다른 프로세스와의 동시 쓰기를 막지 못한다: {}",
                    path.display()
                );
                return Ok(None);
            }
            Err(TryLockError::Error(e)) => {
                return Err(anyhow!(e)).with_context(|| format!("잠금 실패: {}", path.display()));
            }
        }
        if Instant::now() >= deadline {
            return Err(anyhow!(
                "다른 openguild 프로세스가 {}초 넘게 길드를 쓰는 중이라 기다리다 포기했습니다 ({}). \
                 멈춘 앱·CLI 가 없는지 확인하세요.",
                wait.as_secs(),
                path.display()
            ));
        }
        std::thread::sleep(pause);
        pause = (pause * 2).min(Duration::from_millis(50));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

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
    fn a_second_handle_waits_even_in_the_same_process() {
        // 권고 잠금은 기술자 단위다 — 같은 프로세스에서 따로 연 핸들도 막혀야
        // "다른 프로세스" 를 흉내 낸 이 시험이 의미가 있다.
        let path = fresh_tmp("second").join(".lock");
        let held = lock_file(&path, WAIT).unwrap().expect("잠금 지원");
        let err = lock_file(&path, Duration::from_millis(100)).unwrap_err();
        assert!(err.to_string().contains("기다리다 포기"), "{err:#}");
        drop(held);
        assert!(lock_file(&path, Duration::from_millis(100)).unwrap().is_some());
    }

    /// 잠금을 쥔 채 **죽은 프로세스**가 다음 쓰기를 영원히 막지 않는다. 진짜 다른
    /// 프로세스여야 하므로 이 시험 바이너리를 자기 자신으로 다시 띄워 쥐게 한다.
    #[test]
    fn a_killed_holder_does_not_block_forever() {
        if let Ok(path) = std::env::var("OG_LOCK_HOLDER") {
            let _held = lock_file(Path::new(&path), WAIT).unwrap();
            std::fs::write(format!("{path}.held"), "").unwrap();
            std::thread::sleep(Duration::from_secs(600));
            return;
        }
        let path = fresh_tmp("killed").join(".lock");
        let mut child = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "lock::tests::a_killed_holder_does_not_block_forever"])
            .env("OG_LOCK_HOLDER", &path)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .unwrap();
        let marker = PathBuf::from(format!("{}.held", path.display()));
        let deadline = Instant::now() + Duration::from_secs(30);
        while !marker.exists() {
            assert!(Instant::now() < deadline, "자식이 잠금을 못 쥐었다");
            std::thread::sleep(Duration::from_millis(20));
        }

        assert!(
            lock_file(&path, Duration::from_millis(200)).is_err(),
            "다른 프로세스가 쥔 잠금을 통과했다"
        );
        child.kill().unwrap();
        child.wait().unwrap();
        assert!(
            lock_file(&path, Duration::from_secs(5)).unwrap().is_some(),
            "죽은 프로세스의 잠금이 안 풀렸다"
        );
    }

    #[test]
    fn the_file_survives_release() {
        // 풀 때 지우면 다음 두 프로세스가 서로 다른 inode 를 잠가 둘 다 통과한다.
        let path = fresh_tmp("keep").join(".lock");
        drop(lock_file(&path, WAIT).unwrap());
        assert!(path.exists());
    }

    #[test]
    fn a_legacy_pid_lock_file_does_not_block() {
        // 예전 PID 파일 방식이 남긴 내용이 있어도 잠금과는 무관하다.
        let path = fresh_tmp("legacy").join(".lock");
        std::fs::write(&path, "pid = 1\nacquired_at = \"x\"\n").unwrap();
        assert!(lock_file(&path, Duration::from_millis(100)).unwrap().is_some());
    }

    #[tokio::test]
    async fn guards_serialize_tasks() {
        let dir = fresh_tmp("tasks");
        let local = Arc::new(tokio::sync::Mutex::new(()));
        let inside = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let mut jobs = Vec::new();
        for _ in 0..8 {
            let (local, inside, path) = (local.clone(), inside.clone(), dir.join(".lock"));
            jobs.push(tokio::spawn(async move {
                let _g = acquire(local, path).await.unwrap();
                let n = inside.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                assert_eq!(n, 0, "잠금 안에 둘이 들어왔다");
                tokio::time::sleep(Duration::from_millis(5)).await;
                inside.fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
            }));
        }
        for j in jobs {
            j.await.unwrap();
        }
    }
}
