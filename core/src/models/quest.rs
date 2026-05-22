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
    /// DEV-046: stable identifier (예: "open", "testing"). status_id 와 달리
    /// status 추가/순서 변경에도 안전. 사용자 / 외부 클라이언트가 참조하기
    /// 좋은 키.
    pub status_slug: String,
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
    /// DEV-047: parent row 전체 (slug + 색 + 제목 표시용). None 이면 root.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<QuestRow>,
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

/// `quest list` 필터 / 정렬 / 제한.
///
/// 모든 필드 Option / bool default — 미지정 시 기존 동작 (전체 alive, id DESC).
/// 필드 추가 시 server / cli / gui 셋 다 동시 갱신.
///
/// 다중 값은 콤마 구분 string — `?type=DEV,BUG`. service 에서 split.
#[derive(Debug, Deserialize, Default, Clone)]
#[serde(default)]
pub struct ListQuery {
    /// type prefix 필터 — `"DEV"` 또는 다중 `"DEV,BUG"`. 대소문자 무시.
    pub r#type: Option<String>,
    /// status 필터 — `"open"` 또는 다중 `"open,testing"`.
    /// name_en / slug 양쪽 매칭 (대소문자 / 공백 / `_` / `-` 무시).
    pub status: Option<String>,
    /// urgency 필터 — 단일 `"2"`, 다중 CSV `"1,2"`, 범위 `"1-3"`.
    /// 모두 1..=4 범위 안.
    pub urgency: Option<String>,
    /// `created_at >= ?` (ISO 8601, `YYYY-MM-DD` 또는 `YYYY-MM-DDTHH:MM:SSZ`).
    pub created_after: Option<String>,
    /// `created_at <= ?` (inclusive).
    pub created_before: Option<String>,
    /// `updated_at >= ?`.
    pub updated_after: Option<String>,
    /// `updated_at <= ?`.
    pub updated_before: Option<String>,
    /// 선행 quest 가 1개 이상 있는 quest 만. `no_prereq` 와 상호배타.
    pub has_prereq: bool,
    /// 선행 quest 가 없는 quest 만. `has_prereq` 와 상호배타.
    pub no_prereq: bool,
    /// 서브 quest 가 1개 이상 있는 quest 만. `no_sub` 와 상호배타.
    pub has_sub: bool,
    /// 서브 quest 가 없는 leaf quest 만. `has_sub` 와 상호배타.
    pub no_sub: bool,
    /// 검색 키워드 — title / description 부분 일치 (대소문자 무시).
    /// 공백 split 후 AND (모든 토큰 포함).
    pub search: Option<String>,
    /// `search` 토큰을 title 만 검사 — description 제외 (DEV-037).
    /// default false (title + description 둘 다).
    pub title_only: bool,
    /// **자식 quest 들** 을 보여줌 — 지정 slug 가 parent 인 직계 자식.
    /// `--no-parent` 와 상호배타.
    pub child_of: Option<String>,
    /// top-level 만 (`parent_quest_id IS NULL`). `--child-of` 와 상호배타.
    pub no_parent: bool,
    /// 정렬 키 — 콤마 구분 다중 키 (`"urgency,id"`). 각 키마다 기본 방향
    /// (urgency / status = ASC, updated / created / id = DESC).
    /// 화이트리스트: id / urgency / status / updated / created. 대소문자 무시.
    pub sort: Option<String>,
    /// 정렬 방향 전체 토글 — 모든 sort 키의 기본 방향 뒤집음.
    pub reverse: bool,
    /// 결과 최대 행 수.
    pub limit: Option<i64>,
    /// 페이지네이션 — `limit` 와 같이 사용.
    pub offset: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct DeleteQuestQuery {
    /// "1,2,3" 형식의 cascade 삭제 대상 직계 자식 ID 목록
    pub cascade: Option<String>,
}

/// DEV-013: Quest 변경 이력 한 행.
#[derive(Debug, Serialize, Deserialize, sqlx::FromRow, Clone)]
pub struct QuestHistoryEntry {
    pub id: i64,
    pub quest_id: i64,
    pub ts: String,
    pub op: String,
    pub old_value: Option<String>,
    pub new_value: Option<String>,
    pub actor: Option<String>,
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
            status_slug: "open".into(),
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
