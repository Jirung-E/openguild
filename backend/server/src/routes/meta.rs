use axum::{extract::State, Json};
use sqlx::SqlitePool;

use crate::{
    error::AppResult,
    models::{QuestStatus, QuestType},
};

pub async fn list_quest_types(State(pool): State<SqlitePool>) -> AppResult<Json<Vec<QuestType>>> {
    let types = sqlx::query_as::<_, QuestType>("SELECT * FROM quest_types ORDER BY id")
        .fetch_all(&pool)
        .await?;
    Ok(Json(types))
}

pub async fn list_quest_statuses(
    State(pool): State<SqlitePool>,
) -> AppResult<Json<Vec<QuestStatus>>> {
    let statuses =
        sqlx::query_as::<_, QuestStatus>("SELECT * FROM quest_statuses ORDER BY sort_order")
            .fetch_all(&pool)
            .await?;
    Ok(Json(statuses))
}
