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
    /// DEV-042: stable identifier. quest_history 와 .md frontmatter 가 참조.
    /// 파일명 prefix 에서 추출됨 (예: `5-returned.toml` → "returned").
    pub slug: String,
    pub name_en: String,
    pub name_ko: String,
    pub color: String,
    pub sort_order: i64,
}
