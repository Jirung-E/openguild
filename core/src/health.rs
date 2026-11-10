//! 비정상 quest 파일 감지 (read-only) — 정의되지 않은 status 등.
//!
//! reindex 의 skip 판정(파싱 실패 / unknown status)과 같은 취지지만 DB 를
//! 건드리지 않는 검사. GUI 시동 시 / admin 에서 사용자 알림, CLI 경고에 사용.

use crate::repo::QuestFile;
use crate::repo::fs as repo_fs;
use crate::store::Store;

/// (파일 경로, 사유). 정상 파일은 포함 안 됨.
pub type ProblemFile = (String, String);

/// quest 본문 파일 중 파싱 실패하거나 정의되지 않은 status 를 가진 파일 목록.
///
/// BUG-395: **읽지 못한** 파일(윈도우에서 다른 프로그램이 잡음 등)은 넣지 않는다 — 내용이 잘못된 게 아니라 잠깐의
/// 일이고, 풀리면 저절로 정상이다. 그것까지 보려면 [`scan_quest_files`].
pub async fn list_problem_quest_files(store: &Store) -> Vec<ProblemFile> {
    scan_quest_files(store).await.problems
}

/// [`scan_quest_files`] 의 결과.
#[derive(Debug, Default)]
pub struct QuestFileScan {
    /// 내용이 잘못된 파일 — 고치거나 status 를 정의한 뒤 reindex.
    pub problems: Vec<ProblemFile>,
    /// 지금 읽지 못한 파일 — 사람에게 보일 한 줄(`DEV-001 파일을 읽지 못했습니다 — …`, 경로 없음).
    pub unreadable: Vec<String>,
}

/// quest 본문 파일을 훑어 내용이 잘못된 것과 읽지 못한 것을 나눈다.
pub async fn scan_quest_files(store: &Store) -> QuestFileScan {
    let mut out = QuestFileScan::default();
    let statuses: std::collections::HashSet<String> =
        sqlx::query_scalar("SELECT slug FROM quest_statuses")
            .fetch_all(&store.index_pool)
            .await
            .unwrap_or_default()
            .into_iter()
            .collect();
    let files = match repo_fs::list_quest_body_files(store.paths.quests_dir()) {
        Ok(f) => f,
        Err(_) => return out,
    };
    for path in files {
        match QuestFile::read(&path) {
            Err(e) if crate::error::SaveCause::of(&e).is_some() => {
                let doc = path
                    .file_stem()
                    .map(|s| s.to_string_lossy().into_owned())
                    .unwrap_or_default();
                out.unreadable
                    .push(crate::error::AppError::read_failed(&doc, e).to_string());
            }
            Err(e) => out
                .problems
                .push((path.display().to_string(), format!("파싱 실패: {e:#}"))),
            Ok(qf) => {
                if !statuses.contains(&qf.frontmatter.status) {
                    out.problems.push((
                        path.display().to_string(),
                        format!("정의되지 않은 status: '{}'", qf.frontmatter.status),
                    ));
                }
            }
        }
    }
    out
}
