//! quest_types / quest_statuses / quest_tag_defs 조회.

use sqlx::SqlitePool;

use crate::error::AppResult;
use crate::models::{QuestStatus, QuestTagDef, QuestType};

pub async fn list_quest_types(pool: &SqlitePool) -> AppResult<Vec<QuestType>> {
    let types = sqlx::query_as::<_, QuestType>("SELECT * FROM quest_types ORDER BY id")
        .fetch_all(pool)
        .await?;
    Ok(types)
}

pub async fn list_quest_statuses(pool: &SqlitePool) -> AppResult<Vec<QuestStatus>> {
    let statuses =
        sqlx::query_as::<_, QuestStatus>("SELECT * FROM quest_statuses ORDER BY sort_order")
            .fetch_all(pool)
            .await?;
    Ok(statuses)
}

/// DEV-068: 모든 tag 정의 — color / description.
pub async fn list_quest_tag_defs(pool: &SqlitePool) -> AppResult<Vec<QuestTagDef>> {
    let defs = sqlx::query_as::<_, QuestTagDef>(
        "SELECT slug, color, description FROM quest_tag_defs ORDER BY slug",
    )
    .fetch_all(pool)
    .await?;
    Ok(defs)
}
