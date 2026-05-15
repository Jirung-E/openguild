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
#[derive(Debug, Serialize, Deserialize)]
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

#[derive(Debug, Serialize, Deserialize)]
pub struct CreateQuestRequest {
    pub quest_type_id: i64,
    pub title: String,
    pub description: Option<String>,
    pub status_id: i64,
    pub urgency: Option<i64>, // default: 3 (Medium)
    pub parent_quest_id: Option<i64>,
}

#[derive(Debug, Serialize, Deserialize)]
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quest_row_serde_roundtrip() {
        let q = QuestRow {
            id: 1,
            quest_id: "DEV-001".into(),
            quest_type_id: 1,
            type_prefix: "DEV".into(),
            type_color: "#4A90D9".into(),
            number: 1,
            title: "test".into(),
            description: Some("body".into()),
            status_id: 1,
            status_name_en: "Open".into(),
            status_name_ko: "게시됨".into(),
            status_color: "#8B95A1".into(),
            urgency: 3,
            parent_quest_id: None,
            created_at: "".into(),
            updated_at: "".into(),
        };
        let json = serde_json::to_string(&q).unwrap();
        let back: QuestRow = serde_json::from_str(&json).unwrap();
        assert_eq!(back.quest_id, "DEV-001");
        assert_eq!(back.description.as_deref(), Some("body"));
    }

    #[test]
    fn create_quest_request_deserialize() {
        let body = r##"{
            "quest_type_id": 1,
            "title": "t",
            "status_id": 1
        }"##;
        let req: CreateQuestRequest = serde_json::from_str(body).unwrap();
        assert_eq!(req.quest_type_id, 1);
        assert_eq!(req.title, "t");
        assert!(req.description.is_none());
        assert!(req.urgency.is_none());
    }

    #[test]
    fn change_parent_request_accepts_null() {
        let body = r##"{ "parent_quest_id": null }"##;
        let req: ChangeParentRequest = serde_json::from_str(body).unwrap();
        assert!(req.parent_quest_id.is_none());

        let body2 = r##"{ "parent_quest_id": 42 }"##;
        let req2: ChangeParentRequest = serde_json::from_str(body2).unwrap();
        assert_eq!(req2.parent_quest_id, Some(42));
    }

    #[test]
    fn quest_dependency_serde() {
        let d = QuestDependency {
            quest_id: 5,
            prerequisite_id: 3,
        };
        let json = serde_json::to_string(&d).unwrap();
        assert!(json.contains("\"quest_id\":5"));
        assert!(json.contains("\"prerequisite_id\":3"));
    }
}
