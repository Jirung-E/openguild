use serde::{Deserialize, Serialize};

/// DB에서 퀘스트를 조회할 때 사용하는 플랫 구조체 (type, status JOIN 포함)
#[derive(Debug, Serialize, Deserialize, sqlx::FromRow, Clone)]
pub struct QuestRow {
    pub id: i64,
    pub quest_id: String, // "DEV-001"
    pub quest_type_id: i64,
    pub type_prefix: String,
    pub type_color: String,
    pub number: i64,
    pub title: String,
    pub description: Option<String>,
    pub status_id: i64,
    pub status_name_en: String,
    pub status_name_ko: String,
    pub status_color: String,
    pub urgency: i64,
    pub parent_quest_id: Option<i64>,
    pub created_at: String,
    pub updated_at: String,
}

/// 퀘스트 상세 응답 (서브퀘스트, 선행퀘스트, 위치 포함)
#[derive(Debug, Serialize)]
pub struct QuestDetail {
    #[serde(flatten)]
    pub quest: QuestRow,
    pub sub_quests: Vec<QuestRow>,
    pub prerequisites: Vec<QuestRow>,
    pub position: Option<QuestPosition>,
}

#[derive(Debug, Serialize, Deserialize, sqlx::FromRow)]
pub struct QuestPosition {
    pub quest_id: i64,
    pub x: f64,
    pub y: f64,
}

// --- 요청 바디 ---

#[derive(Debug, Deserialize)]
pub struct CreateQuestRequest {
    pub quest_type_id: i64,
    pub title: String,
    pub description: Option<String>,
    pub status_id: i64,
    pub urgency: Option<i64>, // default: 3 (Medium)
    pub parent_quest_id: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateQuestRequest {
    pub title: Option<String>,
    pub description: Option<String>,
    pub urgency: Option<i64>,
}

/// 부모 변경 전용 요청. `parent_quest_id: null`로 분리(detach) 가능.
#[derive(Debug, Deserialize)]
pub struct ChangeParentRequest {
    pub parent_quest_id: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct CandidatesQuery {
    /// "parent" | "sub" | "prereq"
    pub relation: String,
}

#[derive(Debug, Deserialize)]
pub struct DeleteQuestQuery {
    /// "1,2,3" 형식의 cascade 삭제 대상 직계 자식 ID 목록
    pub cascade: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ChangeStatusRequest {
    pub status_id: i64,
}

#[derive(Debug, Deserialize)]
pub struct AddPrerequisiteRequest {
    pub prerequisite_id: i64,
}

#[derive(Debug, Deserialize)]
pub struct UpdatePositionRequest {
    pub x: f64,
    pub y: f64,
}

#[derive(Debug, Serialize, Deserialize, sqlx::FromRow)]
pub struct QuestDependency {
    pub quest_id: i64,
    pub prerequisite_id: i64,
}
