//! Campaign 모델 (DEV-011).
//!
//! - `CampaignRow` — DB SELECT 결과 flat 구조.
//! - `CampaignDetail` — 체크리스트 + 연결된 quest 포함 (상세 화면용).
//! - `CampaignChecklistItem` — 체크리스트 한 항목.
//! - `CampaignSummary` — Home 카드용 (제목 / 기간 / 진행률).
//! - Create / Update / Link 등 요청 바디.

use serde::{Deserialize, Serialize};

/// Campaign 상태. planning 의 "활성 / 완료" 만.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CampaignStatus {
    Active,
    Done,
}

impl CampaignStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Done => "done",
        }
    }

    #[allow(clippy::should_implement_trait)]
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "active" => Some(Self::Active),
            "done" => Some(Self::Done),
            _ => None,
        }
    }
}

/// DB row (campaigns 테이블 단일 SELECT). 상세 항목은 별도 query.
#[derive(Debug, Serialize, Deserialize, sqlx::FromRow, Clone)]
pub struct CampaignRow {
    pub id: i64,
    pub campaign_slug: String, // "C-001"
    pub title: String,
    pub description: Option<String>,
    /// "active" | "done" — `CampaignStatus::from_str` 로 변환.
    pub status: String,
    pub started_at: Option<String>,
    pub ended_at: Option<String>,
    pub display_order: i64,
    /// DEV-087: 배너 이미지 (`.guild/` 상대 경로). None = 없음.
    #[serde(default)]
    pub image_path: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// 체크리스트 항목 (campaign_checklists 테이블 row).
#[derive(Debug, Serialize, Deserialize, sqlx::FromRow, Clone)]
pub struct CampaignChecklistItem {
    pub id: i64,
    pub campaign_id: i64,
    pub text: String,
    pub checked: bool,
    pub order_idx: i64,
}

/// 연결된 Quest 의 간단 정보 (Campaign detail / 카드 표시용).
#[derive(Debug, Serialize, Deserialize, sqlx::FromRow, Clone)]
pub struct CampaignLinkedQuest {
    pub id: i64,
    pub quest_id: String, // slug "DEV-001"
    pub title: String,
    pub type_prefix: String,
    pub type_color: String,
    pub status_slug: String,
    pub status_name_en: String,
    pub status_color: String,
}

/// Campaign 상세 응답 (캠페인 + 체크리스트 + 연결 quest).
#[derive(Debug, Serialize, Deserialize)]
pub struct CampaignDetail {
    #[serde(flatten)]
    pub campaign: CampaignRow,
    pub checklists: Vec<CampaignChecklistItem>,
    pub linked_quests: Vec<CampaignLinkedQuest>,
    /// DEV-093: 링크된 quest 중 alive (= linked_quests.len() — 이미 service 가 alive filter).
    #[serde(default)]
    pub quest_total: i64,
    /// DEV-093: 위 중 status.counts_as_done = true 인 수.
    #[serde(default)]
    pub quest_done: i64,
    /// DEV-093: quest_done / quest_total. 0 일 때 0.0.
    #[serde(default)]
    pub quest_progress: f64,
    /// DEV-156: 본문과 별개 첨부 목록 (Jira 식). sidecar 진리원 — service 는 빈
    /// 채로 두고 Store 가진 호출 계층에서 채운다.
    #[serde(default)]
    pub attachments: Vec<crate::models::QuestAttachment>,
}

/// Home / 카드 표시용 압축 요약.
#[derive(Debug, Serialize, Deserialize)]
pub struct CampaignSummary {
    pub id: i64,
    pub campaign_slug: String,
    pub title: String,
    pub status: String,
    pub started_at: Option<String>,
    pub ended_at: Option<String>,
    pub display_order: i64,
    /// DEV-087: 배너 이미지 (`.guild/` 상대 경로) — Home carousel 배경.
    #[serde(default)]
    pub image_path: Option<String>,
    pub created_at: String,
    /// 체크리스트 완료율 (체크된 항목 / 전체 항목). 항목이 0개면 0.0.
    /// = `checklist_progress` 의 별칭 (frontend 호환).
    pub progress: f64,
    /// 전체 체크리스트 항목 수 (UI 가 "3/10" 처럼 표시 가능).
    pub checklist_total: i64,
    pub checklist_checked: i64,
    /// DEV-093: 링크된 quest 중 alive (soft delete 제외) 개수.
    #[serde(default)]
    pub quest_total: i64,
    /// DEV-093: 위 중 status.counts_as_done = true 인 quest 수.
    #[serde(default)]
    pub quest_done: i64,
    /// DEV-093: quest_done / quest_total. quest_total = 0 이면 0.0.
    #[serde(default)]
    pub quest_progress: f64,
}

// --- 요청 바디 ---

#[derive(Debug, Serialize, Deserialize)]
pub struct CreateCampaignRequest {
    pub title: String,
    pub description: Option<String>,
    pub started_at: Option<String>,
    pub ended_at: Option<String>,
}

/// PATCH /campaigns/{slug} — 모든 필드 optional.
#[derive(Debug, Serialize, Deserialize, Default)]
pub struct UpdateCampaignRequest {
    pub title: Option<String>,
    pub description: Option<String>,
    pub status: Option<String>, // "active" | "done"
    pub started_at: Option<String>,
    pub ended_at: Option<String>,
    pub display_order: Option<i64>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AddChecklistRequest {
    pub text: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct UpdateChecklistRequest {
    pub text: Option<String>,
    pub checked: Option<bool>,
    pub order_idx: Option<i64>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct LinkQuestRequest {
    /// quest slug ("DEV-001") — server / CLI 가 ID 로 resolve.
    pub quest_slug: String,
}
