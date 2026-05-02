use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, sqlx::FromRow)]
pub struct QuestType {
    pub id: i64,
    pub prefix: String,
    pub color: String,
    pub description: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, sqlx::FromRow)]
pub struct QuestStatus {
    pub id: i64,
    pub name_en: String,
    pub name_ko: String,
    pub color: String,
    pub sort_order: i64,
}
