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
//!
//! # 두 겹 — 길드 RW + 문서 (DEV-431, 설계는 REQ-036 / DEV-430)
//!
//! 길드 잠금은 **공유 / 독점** 둘이다. 보통 변경은 길드를 **공유**로 잡고 자기가 고치는 **문서만** 독점으로
//! 잡는다 — 서로 다른 문서끼리는 동시에 된다. 길드 전체가 필요한 일(백업, 타입·상태 이름 바꾸기)은 길드를
//! **독점**으로 잡는다.
//!
//! **잠그는 순서를 부르는 쪽이 못 고른다.** 여러 문서를 서로 다른 순서로 잡으면 엇갈려 영영 기다린다. 그래서
//! 문서 목록은 [`Store::lock_docs`](crate::Store::lock_docs) 한 번에 받고, 순서(이름 순)는 여기서 정한다. 이미
//! 잠금을 쥔 작업이 또 잠그면 오류다 — "하나 잡고 나중에 하나 더" 가 엇갈림의 유일한 길이다.
//!
//! 지금(DEV-431)은 틀만 있다 — `Store::mutation_guard` 는 여전히 길드 **독점**이라 동작은 예전과 같다. 변경
//! 함수를 문서 잠금으로 옮기는 것은 DEV-432.

use anyhow::{Context, Result, anyhow, bail};
use std::collections::{HashMap, HashSet};
use std::fs::{File, OpenOptions, TryLockError};
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// 다른 프로세스가 이만큼 놓지 않으면 포기한다. 변경은 사람 속도이고 가장 긴 것이
/// 동기 자동 스냅샷(~2초)이라, 이 시한에 걸리면 멈춘 프로세스가 있다는 뜻이다.
const WAIT: Duration = Duration::from_secs(60);

/// 문서 잠금 파일이 사는 곳(`.guild/.locks/`). 안에 `*` 한 줄짜리 `.gitignore` 를 두어 폴더째 git 에서
/// 빠진다 — 이미 있는 길드의 `.guild/.gitignore` 를 고치지 않아도 된다.
const LOCKS_DIR: &str = ".locks";

/// 길드 잠금을 어떻게 잡나.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GuildMode {
    /// 보통 변경 — 서로 막지 않는다. 문서 잠금과 함께 쓴다.
    Shared,
    /// 길드 전체가 필요한 일 — 모두를 막는다.
    Exclusive,
}

/// 잠글 문서 하나. "문서" 는 사람이 보는 한 덩이다 — 퀘스트 DEV-1 은 본문 · 댓글 · 메모 · 이력 파일을 한
/// 문서로 본다.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DocKey {
    kind: &'static str,
    id: String,
}

impl DocKey {
    pub fn new(kind: &'static str, id: impl Into<String>) -> Self {
        Self { kind, id: id.into() }
    }
    pub fn quest(slug: &str) -> Self {
        Self::new("quest", slug)
    }
    pub fn campaign(slug: &str) -> Self {
        Self::new("campaign", slug)
    }
    pub fn book(id: &str) -> Self {
        Self::new("book", id)
    }
    pub fn rule(slug: &str) -> Self {
        Self::new("rule", slug)
    }
    /// 길드에 하나뿐인 것 — 보드 위치, 도서관 폴더 목록, 타입별 번호 같은 것.
    pub fn single(name: &'static str) -> Self {
        Self::new(name, "")
    }

    /// 잠금 파일 이름. 파일 이름에 못 쓰는 글자는 `%XX` 로 — 어느 OS 에서도 같은 이름이 되게.
    fn file_name(&self) -> String {
        let mut out = String::from(self.kind);
        if !self.id.is_empty() {
            out.push('~');
            for b in self.id.bytes() {
                if b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-') {
                    out.push(b as char);
                } else {
                    out.push_str(&format!("%{b:02X}"));
                }
            }
        }
        out
    }
}

impl std::fmt::Display for DocKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.id.is_empty() {
            f.write_str(self.kind)
        } else {
            write!(f, "{}:{}", self.kind, self.id)
        }
    }
}

/// 프로세스 안 잠금들. `Store` 가 하나 들고, 복제본끼리 나눠 쓴다. 포인터 하나로 감싼다 — `Store` 가
/// 커지지 않게(CLI 의 `Backend` 가 `Store` 를 그대로 품는다).
#[derive(Clone, Default)]
pub struct Locks(Arc<LocksInner>);

#[derive(Default)]
pub struct LocksInner {
    guild: Arc<tokio::sync::RwLock<()>>,
    docs: Mutex<HashMap<DocKey, Arc<tokio::sync::Mutex<()>>>>,
    /// 지금 잠금을 쥔 작업들 — 같은 작업이 또 잠그려 하면 오류로 돌려준다.
    holders: Arc<Mutex<HashSet<tokio::task::Id>>>,
}

impl std::fmt::Debug for Locks {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Locks")
    }
}

impl Locks {
    fn doc(&self, key: &DocKey) -> Arc<tokio::sync::Mutex<()>> {
        let mut m = self.0.docs.lock().unwrap_or_else(|e| e.into_inner());
        m.entry(key.clone()).or_default().clone()
    }
}

/// 이 작업이 잠금을 쥐고 있다는 표시. 사라지면 지운다.
struct Holder {
    set: Arc<Mutex<HashSet<tokio::task::Id>>>,
    id: tokio::task::Id,
}
impl Drop for Holder {
    fn drop(&mut self) {
        if let Ok(mut s) = self.set.lock() {
            s.remove(&self.id);
        }
    }
}

enum GuildLocal {
    Read(#[allow(dead_code)] tokio::sync::OwnedRwLockReadGuard<()>),
    Write(#[allow(dead_code)] tokio::sync::OwnedRwLockWriteGuard<()>),
}

/// 쥐고 있는 동안 이 길드(독점) 또는 이 문서들(공유)의 다른 변경은 기다린다. drop 하면 푼다.
///
/// 푸는 순서 — 문서들, 길드 파일 잠금, 프로세스 안 길드 잠금, 그다음에 [`after`](Self::after_unlock).
#[must_use = "잠금은 쥐고 있는 동안만 유효하다 — `let _g = ...` 로 묶어 둘 것"]
pub struct MutationGuard {
    docs: Vec<(Option<File>, tokio::sync::OwnedMutexGuard<()>)>,
    file: Option<File>,
    local: Option<GuildLocal>,
    holder: Option<Holder>,
    /// BUG-339: 잠금이 **풀린 뒤에** 할 일. 끝날 때까지 기다리라고 적은 플러그인 줄이 여기서 돈다 —
    /// 잠금을 쥔 채 돌리면 그 훅이 같은 길드를 고치려 할 때 자기 자신을 기다리다 시한에 잘렸다.
    after: Option<Box<dyn FnOnce() + Send>>,
    mode: GuildMode,
}

impl MutationGuard {
    /// 잠금이 풀린 뒤에 할 일을 건다. 하나만 — 두 번 걸면 앞의 것을 대신한다.
    pub fn after_unlock(&mut self, f: impl FnOnce() + Send + 'static) {
        self.after = Some(Box::new(f));
    }

    pub fn mode(&self) -> GuildMode {
        self.mode
    }
}

impl Drop for MutationGuard {
    fn drop(&mut self) {
        // 잠금부터 놓는다 — 그래야 뒤에 도는 일이 이 길드를 다시 잠글 수 있다.
        while let Some(d) = self.docs.pop() {
            drop(d);
        }
        self.file.take();
        self.local.take();
        self.holder.take();
        if let Some(f) = self.after.take() {
            f();
        }
    }
}

impl std::fmt::Debug for MutationGuard {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MutationGuard")
            .field("mode", &self.mode)
            .field("docs", &self.docs.len())
            .field("cross_process", &self.file.is_some())
            .finish()
    }
}

/// 잠근다. `docs` 는 정렬·중복 제거한 뒤 그 순서로 잡는다 — 부르는 쪽이 순서를 정할 수 없다.
pub(crate) async fn acquire(
    locks: &Locks,
    dot_guild: &Path,
    mode: GuildMode,
    docs: &[DocKey],
) -> Result<MutationGuard> {
    // 쥔 채 또 잠그면 오류. 작업(tokio task) 안에서만 알아볼 수 있다 — `block_on` 으로 바로 도는 경우
    // (CLI)는 `ops::lock_coverage` 시험이 정적으로 막는다(잠그는 진입점끼리 서로 부르지 않는다).
    let holder = match tokio::task::try_id() {
        Some(id) => {
            let mut set = locks.0.holders.lock().unwrap_or_else(|e| e.into_inner());
            if !set.insert(id) {
                bail!(
                    "이미 길드 잠금을 쥔 작업이 또 잠그려 했습니다 — 잠글 문서를 한 번에 넘기거나, 다 풀고 \
                     처음부터 다시 잡아야 합니다(엇갈려 멈추는 것을 막으려는 규칙)"
                );
            }
            Some(Holder { set: locks.0.holders.clone(), id })
        }
        None => None,
    };

    let mut docs = docs.to_vec();
    docs.sort();
    docs.dedup();

    let local = match mode {
        GuildMode::Shared => GuildLocal::Read(locks.0.guild.clone().read_owned().await),
        GuildMode::Exclusive => GuildLocal::Write(locks.0.guild.clone().write_owned().await),
    };
    let gpath = dot_guild.join(".lock");
    let shared = mode == GuildMode::Shared;
    let file = tokio::task::spawn_blocking(move || lock_file(&gpath, WAIT, shared))
        .await
        .context("잠금 대기 작업이 중단됨")??;

    let mut held = Vec::with_capacity(docs.len());
    if !docs.is_empty() {
        let dir = dot_guild.join(LOCKS_DIR);
        ensure_locks_dir(&dir)?;
        for d in &docs {
            let local = locks.doc(d).lock_owned().await;
            let p = dir.join(d.file_name());
            let f = tokio::task::spawn_blocking(move || lock_file(&p, WAIT, false))
                .await
                .context("잠금 대기 작업이 중단됨")??;
            held.push((f, local));
        }
    }

    Ok(MutationGuard {
        docs: held,
        file,
        local: Some(local),
        holder,
        after: None,
        mode,
    })
}

/// `.guild/.locks/` 와 그 안의 `.gitignore`(`*`). 폴더째 git 에서 빠진다.
fn ensure_locks_dir(dir: &Path) -> Result<()> {
    std::fs::create_dir_all(dir)
        .with_context(|| format!("잠금 디렉토리 생성 실패: {}", dir.display()))?;
    let ignore = dir.join(".gitignore");
    if !ignore.exists() {
        std::fs::write(&ignore, "# openguild document locks - local, never tracked\n*\n")
            .with_context(|| format!("{} 를 못 썼습니다", ignore.display()))?;
    }
    Ok(())
}

/// `None` 이면 이 파일시스템이 권고 잠금을 지원하지 않는다(일부 네트워크 공유) —
/// 쓰기를 전부 막는 대신 프로세스 안 보호만으로 진행한다.
///
/// `shared` 면 공유 잠금 — 공유끼리는 함께 잡히고 독점과는 서로 막는다.
fn lock_file(path: &Path, wait: Duration, shared: bool) -> Result<Option<File>> {
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
        let r = if shared { file.try_lock_shared() } else { file.try_lock() };
        match r {
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
    use std::path::PathBuf;

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
        let held = lock_file(&path, WAIT, false).unwrap().expect("잠금 지원");
        let err = lock_file(&path, Duration::from_millis(100), false).unwrap_err();
        assert!(err.to_string().contains("기다리다 포기"), "{err:#}");
        drop(held);
        assert!(lock_file(&path, Duration::from_millis(100), false).unwrap().is_some());
    }

    /// 잠금을 쥔 채 **죽은 프로세스**가 다음 쓰기를 영원히 막지 않는다. 진짜 다른
    /// 프로세스여야 하므로 이 시험 바이너리를 자기 자신으로 다시 띄워 쥐게 한다.
    #[test]
    fn a_killed_holder_does_not_block_forever() {
        if let Ok(path) = std::env::var("OG_LOCK_HOLDER") {
            let _held = lock_file(Path::new(&path), WAIT, false).unwrap();
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
            lock_file(&path, Duration::from_millis(200), false).is_err(),
            "다른 프로세스가 쥔 잠금을 통과했다"
        );
        child.kill().unwrap();
        child.wait().unwrap();
        assert!(
            lock_file(&path, Duration::from_secs(5), false).unwrap().is_some(),
            "죽은 프로세스의 잠금이 안 풀렸다"
        );
    }

    #[test]
    fn the_file_survives_release() {
        // 풀 때 지우면 다음 두 프로세스가 서로 다른 inode 를 잠가 둘 다 통과한다.
        let path = fresh_tmp("keep").join(".lock");
        drop(lock_file(&path, WAIT, false).unwrap());
        assert!(path.exists());
    }

    #[test]
    fn a_legacy_pid_lock_file_does_not_block() {
        // 예전 PID 파일 방식이 남긴 내용이 있어도 잠금과는 무관하다.
        let path = fresh_tmp("legacy").join(".lock");
        std::fs::write(&path, "pid = 1\nacquired_at = \"x\"\n").unwrap();
        assert!(lock_file(&path, Duration::from_millis(100), false).unwrap().is_some());
    }

    #[tokio::test]
    async fn guards_serialize_tasks() {
        let dir = fresh_tmp("tasks");
        let locks = Locks::default();
        let inside = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let mut jobs = Vec::new();
        for _ in 0..8 {
            let (locks, inside, dir) = (locks.clone(), inside.clone(), dir.clone());
            jobs.push(tokio::spawn(async move {
                let _g = acquire(&locks, &dir, GuildMode::Exclusive, &[]).await.unwrap();
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

    // ── DEV-431: 길드 RW + 문서 잠금 ─────────────────────────────

    /// 파일 잠금 기준(= 다른 프로세스가 보는 것) — 공유끼리는 함께, 독점과는 서로 막는다.
    #[test]
    fn shared_locks_coexist_and_exclude_an_exclusive_one() {
        let path = fresh_tmp("rw").join(".lock");
        let short = Duration::from_millis(100);
        let a = lock_file(&path, WAIT, true).unwrap().expect("잠금 지원");
        let b = lock_file(&path, short, true).unwrap();
        assert!(b.is_some(), "공유끼리 막혔다");
        assert!(lock_file(&path, short, false).is_err(), "공유가 쥔 동안 독점이 들어왔다");
        drop((a, b));
        let x = lock_file(&path, WAIT, false).unwrap();
        assert!(lock_file(&path, short, true).is_err(), "독점이 쥔 동안 공유가 들어왔다");
        drop(x);
    }

    async fn within<T>(ms: u64, f: impl std::future::Future<Output = T>) -> Option<T> {
        tokio::time::timeout(Duration::from_millis(ms), f).await.ok()
    }

    /// 다른 문서끼리는 동시에, 같은 문서는 줄 선다.
    #[tokio::test]
    async fn different_documents_do_not_wait_but_the_same_one_does() {
        let dir = fresh_tmp("docs");
        let locks = Locks::default();
        let a = acquire(&locks, &dir, GuildMode::Shared, &[DocKey::quest("DEV-1")]).await.unwrap();
        // 다른 문서 — 곧바로 된다(다른 작업에서).
        let (l2, d2) = (locks.clone(), dir.clone());
        let other = tokio::spawn(async move {
            within(1000, acquire(&l2, &d2, GuildMode::Shared, &[DocKey::quest("DEV-2")])).await.map(|r| r.is_ok())
        });
        assert_eq!(other.await.unwrap(), Some(true), "다른 문서인데 기다렸다");
        // 같은 문서 — 쥔 동안은 못 들어온다.
        let (l3, d3) = (locks.clone(), dir.clone());
        let same = tokio::spawn(async move {
            within(200, acquire(&l3, &d3, GuildMode::Shared, &[DocKey::quest("DEV-1")])).await.is_some()
        });
        assert!(!same.await.unwrap(), "같은 문서인데 동시에 들어왔다");
        drop(a);
        let (l4, d4) = (locks.clone(), dir.clone());
        let after = tokio::spawn(async move {
            within(1000, acquire(&l4, &d4, GuildMode::Shared, &[DocKey::quest("DEV-1")])).await.map(|r| r.is_ok())
        });
        assert_eq!(after.await.unwrap(), Some(true), "풀었는데 못 들어왔다");
    }

    /// 독점은 공유가 다 나갈 때까지 기다리고, 독점이 쥔 동안은 공유도 못 들어온다.
    #[tokio::test]
    async fn an_exclusive_lock_waits_for_and_blocks_shared_ones() {
        let dir = fresh_tmp("excl");
        let locks = Locks::default();
        let s = acquire(&locks, &dir, GuildMode::Shared, &[DocKey::quest("DEV-1")]).await.unwrap();
        let (l, d) = (locks.clone(), dir.clone());
        let blocked = tokio::spawn(async move { within(200, acquire(&l, &d, GuildMode::Exclusive, &[])).await.is_some() });
        assert!(!blocked.await.unwrap(), "공유가 쥔 동안 독점이 들어왔다");
        drop(s);
        let x = acquire(&locks, &dir, GuildMode::Exclusive, &[]).await.unwrap();
        let (l, d) = (locks.clone(), dir.clone());
        let blocked = tokio::spawn(async move {
            within(200, acquire(&l, &d, GuildMode::Shared, &[DocKey::quest("DEV-9")])).await.is_some()
        });
        assert!(!blocked.await.unwrap(), "독점이 쥔 동안 공유가 들어왔다");
        drop(x);
    }

    /// 목록을 어떤 순서로 넘겨도 같은 순서로 잡는다 — 엇갈린 두 작업이 서로 기다리다 멈추지 않는다.
    #[tokio::test]
    async fn opposite_orders_never_deadlock() {
        let dir = fresh_tmp("order");
        let locks = Locks::default();
        let (a, b) = (DocKey::quest("DEV-1"), DocKey::quest("DEV-2"));
        let mut jobs = Vec::new();
        for i in 0..40 {
            let (locks, dir) = (locks.clone(), dir.clone());
            let docs = if i % 2 == 0 { vec![a.clone(), b.clone()] } else { vec![b.clone(), a.clone()] };
            jobs.push(tokio::spawn(async move {
                let _g = acquire(&locks, &dir, GuildMode::Shared, &docs).await.unwrap();
                tokio::task::yield_now().await;
            }));
        }
        let all = async {
            for j in jobs {
                j.await.unwrap();
            }
        };
        assert!(within(10_000, all).await.is_some(), "엇갈린 순서에서 멈췄다");
    }

    /// 쥔 채 또 잠그면 오류 — "하나 잡고 나중에 하나 더" 가 엇갈림의 유일한 길이다.
    #[tokio::test]
    async fn locking_again_while_holding_is_an_error() {
        let dir = fresh_tmp("nest");
        let locks = Locks::default();
        let (l, d) = (locks.clone(), dir.clone());
        let r = tokio::spawn(async move {
            let _g = acquire(&l, &d, GuildMode::Shared, &[DocKey::quest("DEV-1")]).await.unwrap();
            within(1000, acquire(&l, &d, GuildMode::Shared, &[DocKey::quest("DEV-2")]))
                .await
                .map(|r| r.map(|_| ()).map_err(|e| e.to_string()))
        })
        .await
        .unwrap();
        let err = r.expect("오류 대신 멈췄다").expect_err("쥔 채 또 잠갔는데 통과했다");
        assert!(err.contains("또 잠그려"), "{err}");
        // 풀고 나면 같은 작업이 다시 잡을 수 있다.
        let (l, d) = (locks.clone(), dir.clone());
        let again = tokio::spawn(async move {
            drop(acquire(&l, &d, GuildMode::Shared, &[DocKey::quest("DEV-1")]).await.unwrap());
            acquire(&l, &d, GuildMode::Shared, &[DocKey::quest("DEV-2")]).await.is_ok()
        });
        assert!(again.await.unwrap());
    }

    /// 문서 잠금 폴더는 스스로 git 에서 빠진다 — 이미 있는 길드의 `.gitignore` 를 안 고쳐도 된다.
    #[tokio::test]
    async fn the_locks_folder_ignores_itself() {
        let dir = fresh_tmp("ignore");
        let locks = Locks::default();
        drop(acquire(&locks, &dir, GuildMode::Shared, &[DocKey::new("rule", "커밋 규칙/v2")]).await.unwrap());
        let ig = std::fs::read_to_string(dir.join(LOCKS_DIR).join(".gitignore")).unwrap();
        assert!(ig.lines().any(|l| l.trim() == "*"), "{ig}");
        // 파일 이름에 못 쓰는 글자는 바뀌고, 잠금 파일은 풀어도 남는다(지우면 inode 가 갈린다).
        let names: Vec<String> = std::fs::read_dir(dir.join(LOCKS_DIR))
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert!(names.iter().any(|n| n.starts_with("rule~") && !n.contains('/')), "{names:?}");
    }
}
