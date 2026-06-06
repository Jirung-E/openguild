use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, sqlx::FromRow)]
pub struct QuestType {
    pub id: i64,
    pub prefix: String,
    pub color: String,
    pub description: Option<String>,
}

/// DEV-068: 사용자 정의 tag — color / description. 파일 진리원.
/// `quest_tags` 의 사용 tag 가 def 없어도 정상 (UI 기본 색 fallback).
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct QuestTagDef {
    pub slug: String,
    pub color: String,
    pub description: String,
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
    /// DEV-093: 이 status 가 "완료" 로 카운트되는지 — 캠페인 진행률 계산용.
    /// migration 0012 의 backfill 로 done / cancelled 가 자동 1.
    #[serde(default)]
    pub counts_as_done: bool,
}
