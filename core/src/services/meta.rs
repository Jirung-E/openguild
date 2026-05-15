//! quest_types / quest_statuses 조회.

use sqlx::SqlitePool;

use crate::error::AppResult;
use crate::models::{QuestStatus, QuestType};

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
