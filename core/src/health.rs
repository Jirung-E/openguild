//! 비정상 quest 파일 감지 (read-only) — 정의되지 않은 status 등.
//!
//! reindex 의 skip 판정(파싱 실패 / unknown status)과 같은 취지지만 DB 를
//! 건드리지 않는 검사. GUI 시동 시 / admin 에서 사용자 알림, CLI 경고에 사용.

use crate::repo::fs as repo_fs;
use crate::repo::QuestFile;
use crate::store::Store;

/// (파일 경로, 사유). 정상 파일은 포함 안 됨.
pub type ProblemFile = (String, String);

/// quest 본문 파일 중 파싱 실패하거나 정의되지 않은 status 를 가진 파일 목록.
pub async fn list_problem_quest_files(store: &Store) -> Vec<ProblemFile> {
    let mut out = Vec::new();
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
            Err(e) => out.push((path.display().to_string(), format!("파싱 실패: {e:#}"))),
            Ok(qf) => {
                if !statuses.contains(&qf.frontmatter.status) {
                    out.push((
                        path.display().to_string(),
                        format!("정의되지 않은 status: '{}'", qf.frontmatter.status),
                    ));
                }
            }
        }
    }
    out
}
