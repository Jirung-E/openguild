//! BUG-286: 보드 위치 쓰기 — **파일이 진리원, DB 는 투영.**
//!
//! 예전엔 `services::quests::update_position(s)` 가 `index.db` 에만 SQL 로 썼다.
//! `index.db` 는 [[BOOK-001]] 이 정한 폐기가능 캐시라, `rm index.db && reindex`
//! (브랜치 전환마다 실행하는 명령) 한 번에 사용자의 보드 배치가 전부 사라졌다.
//!
//! 이제 [`crate::repo::positions`] 의 `positions.json` 에 먼저 쓰고, DB 는 그걸
//! 비추기만 한다. reindex 는 파일에서 다시 채운다.
//!
//! # 쓰는 순서 — 파일 먼저
//!
//! 파일이 진리원이므로 파일부터 쓴다. 파일은 됐는데 DB 가 실패하면, 화면은 잠시
//! 옛 위치를 보이지만 다음 적재·reindex 에서 파일대로 돌아온다. 반대(DB 먼저)면
//! 화면은 새 위치인데 **다음 reindex 에서 사라진다** — 고치려던 바로 그 증상이다.

use crate::error::{AppError, AppResult};
use crate::models::{PositionItem, QuestPosition, UpdatePositionRequest};
use crate::repo::positions::{self as file, Pos};
use crate::services::quests as db;
use crate::store::Store;

/// 퀘스트 id → 현재 slug. 없는(지워진) 퀘스트면 `None`.
async fn slug_of(store: &Store, id: i64) -> AppResult<Option<String>> {
    Ok(sqlx::query_scalar(
        "SELECT qt.prefix || '-' || printf('%03d', q.number)
           FROM quests q JOIN quest_types qt ON q.quest_type_id = qt.id
          WHERE q.id = ?",
    )
    .bind(id)
    .fetch_optional(&store.index_pool)
    .await?)
}

/// **파일이 아직 없으면 옛 DB 의 위치로 먼저 채운다.** 호출자는 `write_lock` 을
/// 잡고 부른다.
///
/// 이게 없으면 이행 구멍이 생긴다. 업그레이드한 사용자가 reindex 없이 앱만 열고 노드
/// 하나를 옮기면, 없는 파일이 빈 상태로 읽혀 **그 노드 하나만 든 파일**이 만들어진다.
/// 다음 reindex 는 "파일이 있다" 고 보고 그걸 쓰므로 **나머지 배치가 전부 날아간다.**
/// 그래서 이행은 reindex 가 아니라 **첫 쓰기 전에** 한다(reindex 쪽 이행은 쓰기가 한
/// 번도 없던 경우를 덮는다).
async fn ensure_file(store: &Store) -> AppResult<()> {
    if file::read(&store.paths)
        .map_err(AppError::Internal)?
        .is_some()
    {
        return Ok(());
    }
    let legacy: Vec<(String, f64, f64)> = sqlx::query_as(
        "SELECT t.prefix || '-' || printf('%03d', q.number) AS slug, p.x, p.y
           FROM quest_positions p
           JOIN quests q ON q.id = p.quest_id
           JOIN quest_types t ON t.id = q.quest_type_id",
    )
    .fetch_all(&store.index_pool)
    .await?;
    let items: Vec<_> = legacy
        .into_iter()
        .map(|(s, x, y)| (s, Pos { x, y }))
        .collect();
    // 비어 있어도 쓴다 — "이행을 마쳤다" 를 파일의 존재로 남긴다.
    file::write(&store.paths, &items.into_iter().collect()).map_err(AppError::Internal)
}

/// 위치 하나.
pub async fn update_position(
    store: &Store,
    id: i64,
    body: UpdatePositionRequest,
) -> AppResult<QuestPosition> {
    let slug = slug_of(store, id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("quest not found: {id}")))?;
    // 파일은 read-modify-write 라 같은 프로세스 안의 동시 저장이 서로를 지우지 않게.
    let _w = store.write_lock.lock().await;
    ensure_file(store).await?;
    file::upsert(
        &store.paths,
        &[(
            slug,
            Pos {
                x: body.x,
                y: body.y,
            },
        )],
    )
    .map_err(AppError::Internal)?;
    db::update_position(&store.index_pool, id, body).await
}

/// 여러 위치를 한 번에 — [[BUG-284]] 가 보드 적재 때 자동 배치 노드를 고정하는 경로.
///
/// **파일은 한 번만 쓴다.** 수백 개를 건마다 다시 쓰면 그만큼 디스크를 친다.
/// 없는 퀘스트는 건너뛴다(보드가 들고 있던 목록과 그사이 지워진 것이 어긋날 수 있다).
pub async fn update_positions(store: &Store, items: &[PositionItem]) -> AppResult<usize> {
    if items.is_empty() {
        return Ok(0);
    }
    let mut to_file = Vec::with_capacity(items.len());
    for it in items {
        if let Some(slug) = slug_of(store, it.quest_id).await? {
            to_file.push((slug, Pos { x: it.x, y: it.y }));
        }
    }
    let _w = store.write_lock.lock().await;
    ensure_file(store).await?;
    file::upsert(&store.paths, &to_file).map_err(AppError::Internal)?;
    db::update_positions(&store.index_pool, items).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::CreateQuestRequest;

    fn fresh_tmp(label: &str) -> std::path::PathBuf {
        use std::sync::atomic::{AtomicU64, Ordering};
        static SEQ: AtomicU64 = AtomicU64::new(0);
        let ns = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let n = SEQ.fetch_add(1, Ordering::Relaxed);
        let p = std::env::temp_dir().join(format!("og-pos-{label}-{ns}-{n}"));
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    async fn open(dir: &std::path::Path) -> Store {
        let s = Store::open(dir).await.unwrap();
        crate::reindex::reindex(&s).await.unwrap();
        s
    }

    async fn make(store: &Store, title: &str) -> crate::models::QuestRow {
        crate::ops::quests::create_quest(
            store,
            CreateQuestRequest {
                quest_type_id: 1,
                title: title.into(),
                description: None,
                status_slug: "open".into(),
                urgency: Some(3),
                parent_quest_id: None,
            },
        )
        .await
        .unwrap()
    }

    async fn pos_of(store: &Store, slug: &str) -> Option<(f64, f64)> {
        crate::services::quests::list_positions(&store.index_pool)
            .await
            .unwrap()
            .into_iter()
            .find(|p| p.quest_slug.as_deref() == Some(slug))
            .map(|p| (p.x, p.y))
    }

    /// **이 결함의 본체.** 위치를 저장한 뒤 `rm index.db && reindex` 해도 남아야 한다.
    ///
    /// 예전엔 위치가 index.db 에만 있어 이 명령 한 번에 전부 사라졌다. BOOK-001 이
    /// "회피책이 아니라 불변식의 일부" 라 하고, 릴리스 규칙이 **브랜치 전환마다**
    /// 실행하라는 명령이다 — 즉 브랜치를 바꿀 때마다 보드 배치가 날아갔다.
    #[tokio::test]
    async fn a_position_survives_deleting_the_index() {
        let dir = fresh_tmp("survive");
        crate::repo::seed_guild_dir(&dir).unwrap();
        // slug 를 박아 두지 않는다 — 시드의 type_id 1 이 무엇인지에 따라 달라진다.
        // 처음엔 "DEV-001" 로 박았다가, 실제로는 BUG-001 이라 없는 것을 찾았다.
        let slug;
        {
            let store = open(&dir).await;
            let q = make(&store, "옮긴 노드").await;
            slug = q.quest_id.clone();
            update_position(&store, q.id, UpdatePositionRequest { x: 777.0, y: 888.0 })
                .await
                .unwrap();
        }

        // 폐기가능 캐시를 지운다 — 브랜치 전환 뒤 하는 그대로.
        std::fs::remove_file(dir.join(".guild/index.db")).unwrap();
        let store = open(&dir).await;

        assert_eq!(
            pos_of(&store, &slug).await,
            Some((777.0, 888.0)),
            "rm index.db 뒤에 위치가 사라졌다 — 캐시에만 있었다"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 퀘스트 타입이 바뀌면 slug 가 바뀐다. 파일의 키가 안 따라가면 다음 reindex 가
    /// 옛 slug 로 찾다가 못 찾아 위치를 버린다.
    #[tokio::test]
    async fn a_position_follows_a_quest_type_change() {
        let dir = fresh_tmp("retype");
        crate::repo::seed_guild_dir(&dir).unwrap();
        let store = open(&dir).await;
        let q = make(&store, "타입 바꿀 노드").await;
        update_position(&store, q.id, UpdatePositionRequest { x: 1.0, y: 2.0 })
            .await
            .unwrap();

        // **지금 타입과 다른 타입**을 고른다. 처음엔 "DEV 가 아닌 것" 을 골랐는데,
        // 퀘스트가 이미 BUG 라서 BUG 를 골라 변경 없음(NoOp)으로 통과했다 —
        // 옮기는 코드를 한 줄도 안 태우고.
        let bug = crate::services::meta::list_quest_types(&store.index_pool)
            .await
            .unwrap()
            .into_iter()
            .find(|t| t.prefix != q.type_prefix)
            .expect("시드에 다른 타입이 있어야 한다");
        let moved = crate::ops::quests::change_quest_type(
            &store,
            q.id,
            crate::models::ChangeTypeRequest {
                new_type_prefix: bug.prefix.clone(),
            },
        )
        .await
        .unwrap();
        assert_ne!(
            moved.quest_id, q.quest_id,
            "전제: slug 가 실제로 바뀌어야 한다"
        );
        drop(store);

        std::fs::remove_file(dir.join(".guild/index.db")).unwrap();
        let store = open(&dir).await;
        assert_eq!(
            pos_of(&store, &moved.quest_id).await,
            Some((1.0, 2.0)),
            "타입을 바꾼 뒤 위치가 사라졌다 — 파일 키가 안 옮겨졌다"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 타입의 **이름**을 바꾸면(`BUG` → `DEFECT`) 그 타입 퀘스트의 slug 가 **전부**
    /// 바뀐다. 키를 안 옮기면 한 타입의 위치가 통째로 사라진다.
    #[tokio::test]
    async fn positions_follow_a_type_prefix_rename() {
        let dir = fresh_tmp("rename-type");
        crate::repo::seed_guild_dir(&dir).unwrap();
        let store = open(&dir).await;
        let a = make(&store, "하나").await;
        let b = make(&store, "둘").await;
        update_positions(
            &store,
            &[
                PositionItem {
                    quest_id: a.id,
                    x: 5.0,
                    y: 6.0,
                },
                PositionItem {
                    quest_id: b.id,
                    x: 7.0,
                    y: 8.0,
                },
            ],
        )
        .await
        .unwrap();

        let old_prefix = a.type_prefix.clone();
        crate::ops::meta::rename_type(&store, old_prefix.clone(), "ZZZ".into())
            .await
            .unwrap();
        drop(store);

        std::fs::remove_file(dir.join(".guild/index.db")).unwrap();
        let store = open(&dir).await;
        let new_a = a.quest_id.replacen(&old_prefix, "ZZZ", 1);
        let new_b = b.quest_id.replacen(&old_prefix, "ZZZ", 1);
        assert_eq!(pos_of(&store, &new_a).await, Some((5.0, 6.0)), "{new_a}");
        assert_eq!(pos_of(&store, &new_b).await, Some((7.0, 8.0)), "{new_b}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// **이행 구멍.** 업그레이드한 사용자가 reindex 없이 노드 하나를 옮기면, 없는 파일이
    /// 빈 상태로 읽혀 그 노드 하나만 든 파일이 생긴다 → 다음 reindex 가 그 파일을 믿어
    /// 나머지 배치를 전부 버린다. 첫 쓰기가 옛 DB 위치로 파일을 먼저 채워야 한다.
    #[tokio::test]
    async fn the_first_write_carries_over_legacy_positions() {
        let dir = fresh_tmp("legacy");
        crate::repo::seed_guild_dir(&dir).unwrap();
        let store = open(&dir).await;
        let a = make(&store, "옛 위치 A").await;
        let b = make(&store, "새로 옮길 B").await;

        // 이 변경 **이전** 동작을 흉내 — DB 에만 쓰고 파일은 없다.
        crate::services::quests::update_position(
            &store.index_pool,
            a.id,
            UpdatePositionRequest { x: 11.0, y: 22.0 },
        )
        .await
        .unwrap();
        assert!(
            !dir.join(".guild/positions.json").exists(),
            "전제: 아직 파일이 없어야 한다"
        );

        // 업그레이드 뒤 첫 조작 — 다른 노드 하나만 옮긴다.
        update_position(&store, b.id, UpdatePositionRequest { x: 33.0, y: 44.0 })
            .await
            .unwrap();
        drop(store);

        std::fs::remove_file(dir.join(".guild/index.db")).unwrap();
        let store = open(&dir).await;
        assert_eq!(
            pos_of(&store, &a.quest_id).await,
            Some((11.0, 22.0)),
            "옛 위치가 날아갔다 — 첫 쓰기가 B 하나만 든 파일을 만들었다"
        );
        assert_eq!(pos_of(&store, &b.quest_id).await, Some((33.0, 44.0)));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 깨진 파일은 **덮어쓰지 않는다** — 빈 상태로 갈음하면 다음 쓰기가 배치를 통째로
    /// 날린다(DEV-385 가 동의 파일에서 겪은 그대로).
    #[tokio::test]
    async fn a_corrupt_file_is_not_overwritten() {
        let dir = fresh_tmp("corrupt");
        crate::repo::seed_guild_dir(&dir).unwrap();
        let store = open(&dir).await;
        let q = make(&store, "노드").await;
        let f = dir.join(".guild/positions.json");
        std::fs::write(&f, "{ 깨진 json").unwrap();

        let r = update_position(&store, q.id, UpdatePositionRequest { x: 1.0, y: 1.0 }).await;
        assert!(r.is_err(), "깨진 파일 위에 그냥 썼다");
        assert_eq!(
            std::fs::read_to_string(&f).unwrap(),
            "{ 깨진 json",
            "깨진 파일이 덮어써졌다 — 사용자가 고칠 기회를 잃었다"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
