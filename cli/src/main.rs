//! OpenGuild CLI (`openguild`)
//!
//! 두 모드 지원:
//!   - **로컬 (기본)**: cwd 부터 `.guild` 탐색 → core 직접 호출. 서버 불필요.
//!   - **원격**: `--remote URL` 또는 env `OPENGUILD_REMOTE` 지정 시 HTTP 호출.
//!
//! 환경변수:
//!   OPENGUILD_REMOTE   원격 서버 base URL. 지정 시 원격 모드.
//!
//! 글로벌 옵션:
//!   --remote URL     원격 모드 강제 (env 보다 우선)
//!   --guild PATH     로컬 모드에서 .guild 가 있는 디렉토리 직접 지정 (cwd 자동탐색 대체)
//!   --json           JSON 출력 (agent 용)

use anyhow::{anyhow, Context, Result};
use clap::{Parser, Subcommand};
use openguild_core::models::{
    AddPrerequisiteRequest, ChangeParentRequest, ChangeStatusRequest, CreateQuestRequest,
    ListQuery, QuestDetail, QuestRow as Quest, QuestStatus, QuestType, UpdateQuestRequest,
};
use openguild_core::services::{meta as meta_svc, quests as quest_svc};
use serde::{Deserialize, Serialize};

// ─────────────────────────── CLI 정의 ───────────────────────────

#[derive(Parser)]
#[command(
    name = "openguild",
    version,
    about = "OpenGuild CLI — local + remote guild operations"
)]
struct Cli {
    /// 원격 모드 — 서버 URL 지정 (env: OPENGUILD_REMOTE). 미지정 시 로컬 모드.
    #[arg(long, global = true, value_name = "URL")]
    remote: Option<String>,

    /// 로컬 모드에서 사용할 길드 경로. 미지정 시 cwd 부터 .guild 자동 탐색.
    #[arg(long, global = true, value_name = "PATH")]
    guild: Option<String>,

    /// JSON 출력 (agent 가 stdout 파싱용)
    #[arg(long, global = true)]
    json: bool,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// 현재 디렉토리를 길드로 초기화 (.guild 마커 파일 생성)
    Init {
        /// 길드 이름. 미지정 시 현재 디렉토리 이름 사용.
        #[arg(long)]
        name: Option<String>,
    },
    /// 퀘스트 관련 명령
    Quest {
        #[command(subcommand)]
        sub: QuestCmd,
    },
    /// 퀘스트 타입 목록
    Types,
    /// 퀘스트 상태 목록
    Statuses,
    /// 서버 상태 확인 (health)
    Ping,
    /// 백업 (snapshot) 즉시 생성
    Backup,
    /// 사용 가능한 백업 목록
    Backups,
    /// 백업으로 복원
    Restore {
        /// 특정 timestamp (`YYYYMMDD-HHMMSS`). 미지정 시 최신 사용.
        #[arg(long)]
        to: Option<String>,
    },
}

#[derive(Subcommand)]
enum QuestCmd {
    /// 퀘스트 목록 (인자 없으면 전체 alive, id DESC).
    List {
        /// 타입 prefix 필터 — `DEV` / `BUG` / `REQ`. 다중 입력 가능:
        ///   `--type DEV BUG` (공백), `--type DEV,BUG` (콤마), `--type DEV --type BUG`.
        /// 대소문자 무시.
        #[arg(long = "type", value_name = "PREFIX",
              value_delimiter = ',', num_args = 1..)]
        type_prefix: Vec<String>,
        /// 상태 필터 — name_en (`Open` / `In Progress`) 또는 slug (`open` /
        /// `in_progress` / `in-progress`). 다중 입력: 공백 / 콤마 / 반복.
        /// 대소문자 / 공백 / `_` / `-` 무시.
        #[arg(long, value_delimiter = ',', num_args = 1..)]
        status: Vec<String>,
        /// urgency 필터 — 단일 (`2`), 다중 CSV (`1,2`), 범위 (`1-3`). 1=Critical, 4=Low.
        #[arg(long)]
        urgency: Option<String>,
        /// 생성 시점 ≥ ISO date (`2026-05-15` 또는 `2026-05-15T10:00:00Z`).
        #[arg(long = "created-after", value_name = "ISO")]
        created_after: Option<String>,
        /// 생성 시점 ≤ ISO date.
        #[arg(long = "created-before", value_name = "ISO")]
        created_before: Option<String>,
        /// 갱신 시점 ≥ ISO date.
        #[arg(long = "updated-after", value_name = "ISO")]
        updated_after: Option<String>,
        /// 갱신 시점 ≤ ISO date.
        #[arg(long = "updated-before", value_name = "ISO")]
        updated_before: Option<String>,
        /// **자식** 표시 — 지정 quest slug 가 parent 인 직계 자식들만 보여줌.
        /// (`--no-parent` 와 상호배타.)
        #[arg(long = "child-of", value_name = "SLUG", conflicts_with = "no_parent")]
        child_of: Option<String>,
        /// top-level (parent 없는) quest 만.
        #[arg(long)]
        no_parent: bool,
        /// 선행 quest 가 1개 이상 있는 quest 만.
        #[arg(long = "has-prereq", conflicts_with = "no_prereq")]
        has_prereq: bool,
        /// 선행 quest 가 없는 quest 만.
        #[arg(long = "no-prereq")]
        no_prereq: bool,
        /// 서브 quest 가 1개 이상 있는 quest 만.
        #[arg(long = "has-sub", conflicts_with = "no_sub")]
        has_sub: bool,
        /// 서브 quest 가 없는 leaf quest 만.
        #[arg(long = "no-sub")]
        no_sub: bool,
        /// title / description 부분 일치 검색. 공백 split AND.
        #[arg(long)]
        search: Option<String>,
        /// 정렬 키 — `id` (기본) / `urgency` / `status` / `updated` / `created`.
        /// 다중 입력 가능 (`--sort urgency,id` 또는 `--sort urgency id`). 대소문자 무시.
        #[arg(long, value_delimiter = ',', num_args = 1..)]
        sort: Vec<String>,
        /// 정렬 방향 전체 토글 — 모든 sort 키의 기본 방향 뒤집음.
        #[arg(long)]
        reverse: bool,
        /// 결과 최대 행 수.
        #[arg(long)]
        limit: Option<i64>,
        /// 페이지네이션 offset.
        #[arg(long)]
        offset: Option<i64>,
        /// quest_id (slug) 만 한 줄씩 출력 — `xargs` / pipe 친화.
        /// `--count` 와 상호배타. `--json` 과는 무시되고 정상 JSON 출력.
        #[arg(long, conflicts_with = "count")]
        id_only: bool,
        /// 매칭 개수만 정수로 출력. `--id-only` 와 상호배타.
        #[arg(long)]
        count: bool,
    },
    /// 퀘스트 상세 (슬러그: DEV-001)
    Show { slug: String },
    /// quest 의 변경 이력 (DEV-013) — 최신 → 과거 순.
    History { slug: String },
    /// 새 퀘스트 생성
    New {
        /// 타입 prefix (DEV / BUG / REQ ...)
        #[arg(long = "type", value_name = "PREFIX")]
        type_prefix: String,
        #[arg(long)]
        title: String,
        #[arg(long)]
        description: Option<String>,
        /// 1=Critical 2=High 3=Medium 4=Low (기본 3)
        #[arg(long, default_value_t = 3)]
        urgency: i64,
        /// 부모 퀘스트 슬러그 (서브퀘스트로 생성)
        #[arg(long)]
        parent: Option<String>,
    },
    /// 수정 (제공된 필드만)
    Update {
        slug: String,
        #[arg(long)]
        title: Option<String>,
        #[arg(long)]
        description: Option<String>,
        #[arg(long)]
        urgency: Option<i64>,
        /// 실제 수정 대신 변경 미리보기만 출력
        #[arg(long)]
        dry_run: bool,
    },
    /// 삭제 (soft delete — restore 가능). 안전장치: --yes 없으면 거부
    Delete {
        slug: String,
        /// 같이 삭제할 직계 자식 슬러그 (콤마 구분)
        #[arg(long, value_delimiter = ',')]
        cascade: Vec<String>,
        /// 실제 실행 대신 영향 미리보기만 출력 (변경 X)
        #[arg(long)]
        dry_run: bool,
        /// 삭제를 명시적으로 승인. dry-run 이 아닌 모든 실제 삭제에 필수.
        #[arg(long)]
        yes: bool,
    },
    /// 삭제된(soft deleted) 퀘스트 목록
    Deleted,
    /// 삭제된 퀘스트 복원
    Restore { slug: String },
    /// 상태 변경 (status: name_en 또는 ID)
    Status { slug: String, status: String },
    /// 상태를 In Progress 로 변경
    Start { slug: String },
    /// 상태를 Done 으로 변경
    Done { slug: String },
    /// 상태를 Open 으로 변경
    Reopen { slug: String },
    /// 부모 변경 (slug 또는 --detach)
    Parent {
        slug: String,
        /// 새 부모 슬러그
        parent: Option<String>,
        /// 부모에서 분리
        #[arg(long)]
        detach: bool,
    },
    /// 선행 퀘스트 관리
    Prereq {
        #[command(subcommand)]
        sub: PrereqCmd,
    },
}

#[derive(Subcommand)]
enum PrereqCmd {
    /// 선행 퀘스트 추가
    Add { slug: String, prereq: String },
    /// 선행 퀘스트 제거
    Rm { slug: String, prereq: String },
}

// DTO 는 `openguild_core::models` 에서 직접 사용. 위 use 문 참고.

// ─────────────────────────── HTTP 클라이언트 ───────────────────────────

struct HttpClient {
    base: String,
    http: reqwest::blocking::Client,
}

impl HttpClient {
    fn new(base: String) -> Self {
        Self {
            base,
            http: reqwest::blocking::Client::new(),
        }
    }

    fn url(&self, path: &str) -> String {
        format!("{}{}", self.base, path)
    }

    /// 응답 코드 검사 후 JSON 디코드. 4xx/5xx 면 {"error": "..."} 추출.
    fn handle<T: for<'de> Deserialize<'de>>(
        &self,
        res: reqwest::blocking::Response,
    ) -> Result<T> {
        let status = res.status();
        let body = res.text().unwrap_or_default();
        if !status.is_success() {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(&body)
                && let Some(err) = v.get("error").and_then(|e| e.as_str())
            {
                return Err(anyhow!("{}: {}", status, err));
            }
            return Err(anyhow!("{}: {}", status, body));
        }
        if body.is_empty() {
            // 204 No Content 등 — caller 가 () 또는 Option 받기를 기대해야
            return serde_json::from_str("null")
                .context("empty body, response type does not allow null");
        }
        serde_json::from_str(&body).with_context(|| format!("failed to parse JSON: {body}"))
    }

    fn get<T: for<'de> Deserialize<'de>>(&self, path: &str) -> Result<T> {
        let res = self.http.get(self.url(path)).send()?;
        self.handle(res)
    }

    fn post<B: Serialize, T: for<'de> Deserialize<'de>>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<T> {
        let res = self.http.post(self.url(path)).json(body).send()?;
        self.handle(res)
    }

    fn patch<B: Serialize, T: for<'de> Deserialize<'de>>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<T> {
        let res = self.http.patch(self.url(path)).json(body).send()?;
        self.handle(res)
    }

    fn delete_no_body(&self, path: &str) -> Result<()> {
        let res = self.http.delete(self.url(path)).send()?;
        let status = res.status();
        if !status.is_success() {
            let body = res.text().unwrap_or_default();
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(&body)
                && let Some(err) = v.get("error").and_then(|e| e.as_str())
            {
                return Err(anyhow!("{}: {}", status, err));
            }
            return Err(anyhow!("{}: {}", status, body));
        }
        Ok(())
    }

    // ── 도메인 메서드 ─────────────────────────────────────

    fn ping(&self) -> Result<String> {
        let res = self.http.get(self.url("/health")).send()?;
        let status = res.status();
        if !status.is_success() {
            return Err(anyhow!("{}", status));
        }
        Ok(res.text().unwrap_or_else(|_| "ok".to_string()))
    }

    fn list_quests(&self, q: &ListQuery) -> Result<Vec<Quest>> {
        let qs = list_query_to_querystring(q);
        let path = if qs.is_empty() {
            "/api/quests".to_string()
        } else {
            format!("/api/quests?{qs}")
        };
        self.get(&path)
    }

    fn list_deleted_quests(&self) -> Result<Vec<Quest>> {
        self.get("/api/deleted-quests")
    }

    fn quest_by_slug(&self, slug: &str) -> Result<QuestDetail> {
        self.get(&format!("/api/quests/by/{slug}"))
    }

    fn list_quest_history(&self, id: i64) -> Result<Vec<openguild_core::models::QuestHistoryEntry>> {
        self.get(&format!("/api/quests/{id}/history"))
    }

    fn quest_types(&self) -> Result<Vec<QuestType>> {
        self.get("/api/quest-types")
    }

    fn quest_statuses(&self) -> Result<Vec<QuestStatus>> {
        self.get("/api/quest-statuses")
    }

    fn create_quest(&self, body: &CreateQuestRequest) -> Result<Quest> {
        self.post("/api/quests", body)
    }

    fn update_quest(&self, id: i64, body: &UpdateQuestRequest) -> Result<Quest> {
        self.patch(&format!("/api/quests/{id}"), body)
    }

    fn delete_quest(&self, id: i64, cascade_ids: &[i64]) -> Result<()> {
        let qs = if cascade_ids.is_empty() {
            String::new()
        } else {
            let s = cascade_ids
                .iter()
                .map(|i| i.to_string())
                .collect::<Vec<_>>()
                .join(",");
            format!("?cascade={s}")
        };
        self.delete_no_body(&format!("/api/quests/{id}{qs}"))
    }

    fn restore_quest(&self, id: i64) -> Result<Quest> {
        self.patch(&format!("/api/quests/{id}/restore"), &serde_json::json!({}))
    }

    fn change_status(&self, id: i64, status_id: i64) -> Result<Quest> {
        self.patch(
            &format!("/api/quests/{id}/status"),
            &serde_json::json!({ "status_id": status_id }),
        )
    }

    fn change_parent(&self, id: i64, parent_id: Option<i64>) -> Result<Quest> {
        self.patch(
            &format!("/api/quests/{id}/parent"),
            &serde_json::json!({ "parent_quest_id": parent_id }),
        )
    }

    fn add_prerequisite(&self, id: i64, prereq_id: i64) -> Result<()> {
        let res = self
            .http
            .post(self.url(&format!("/api/quests/{id}/prerequisites")))
            .json(&serde_json::json!({ "prerequisite_id": prereq_id }))
            .send()?;
        let status = res.status();
        if !status.is_success() {
            let body = res.text().unwrap_or_default();
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(&body)
                && let Some(err) = v.get("error").and_then(|e| e.as_str())
            {
                return Err(anyhow!("{}: {}", status, err));
            }
            return Err(anyhow!("{}: {}", status, body));
        }
        Ok(())
    }

    fn remove_prerequisite(&self, id: i64, prereq_id: i64) -> Result<()> {
        self.delete_no_body(&format!("/api/quests/{id}/prerequisites/{prereq_id}"))
    }

    fn create_snapshot(&self) -> Result<openguild_core::snapshot::SnapshotInfo> {
        self.post("/api/admin/snapshot", &serde_json::json!({}))
    }

    fn list_snapshots(&self) -> Result<Vec<openguild_core::snapshot::SnapshotInfo>> {
        self.get("/api/admin/snapshots")
    }

    fn restore_snapshot(
        &self,
        to: Option<String>,
    ) -> Result<openguild_core::snapshot::SnapshotInfo> {
        let body = match to {
            Some(ts) => serde_json::json!({ "to": ts }),
            None => serde_json::json!({}),
        };
        let resp: serde_json::Value = self.post("/api/admin/restore", &body)?;
        // 응답에는 timestamp 만 있음 — list 에서 size 채우기 위해 한 번 더 조회.
        let ts = resp
            .get("restored_to")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!("server 응답에 restored_to 누락"))?
            .to_string();
        let list = self.list_snapshots()?;
        list.into_iter()
            .find(|s| s.timestamp == ts)
            .ok_or_else(|| anyhow!("복원된 snapshot 정보 누락"))
    }
}

// ─────────────────────────── Backend (Http / Local) ───────────────────────────

/// 백엔드 추상화. 같은 메서드 시그니처로 HTTP / Local 양쪽 지원.
///
/// - Http: 기존 reqwest blocking 클라이언트 위임
/// - Local: tokio runtime 으로 `core::services::*` 직접 호출
enum Backend {
    Http(HttpClient),
    Local(LocalBackend),
}

struct LocalBackend {
    store: openguild_core::Store,
    rt: tokio::runtime::Runtime,
    /// 호스트 길드 경로 (info 출력용)
    guild_path: std::path::PathBuf,
}

impl Backend {
    /// `--remote` / `OPENGUILD_REMOTE` 지정 시 Http, 그 외 로컬 모드.
    /// 로컬 모드: `--guild PATH` 또는 cwd 부터 `.guild` 자동 탐색.
    fn new(remote: Option<String>, guild_arg: Option<String>) -> Result<Self> {
        let remote = remote.or_else(|| std::env::var("OPENGUILD_REMOTE").ok());
        if let Some(url) = remote {
            return Ok(Backend::Http(HttpClient::new(url)));
        }

        // 로컬 모드 — 길드 경로 결정
        let guild_path = if let Some(p) = guild_arg {
            let pb = std::path::PathBuf::from(p);
            if openguild_core::guild_file::find_from(&pb).is_none_or(|f| f != pb) {
                return Err(anyhow!(
                    "no .guild file at {} (use `openguild init` first)",
                    pb.display()
                ));
            }
            pb
        } else {
            openguild_core::guild_file::find_from_cwd().ok_or_else(|| {
                anyhow!(
                    "no .guild found in cwd or its ancestors.\n\
                     로컬 모드: `openguild init` 으로 길드를 만드세요.\n\
                     원격 모드: `--remote URL` 또는 env OPENGUILD_REMOTE 지정."
                )
            })?
        };

        // Store 가 .guild/index.db + journal.db 자동 마이그레이션 + 시드.
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .context("failed to start tokio runtime")?;
        let store = rt.block_on(openguild_core::Store::open(&guild_path))?;

        // Recent guild 자동 등록 — 실패해도 ops 자체엔 영향 없음 (warn 만).
        if let Err(e) = openguild_core::recents::add(&guild_path) {
            eprintln!("[openguild] warn: recents 갱신 실패 — {e:#}");
        }

        Ok(Backend::Local(LocalBackend {
            store,
            rt,
            guild_path,
        }))
    }

    /// AppError → anyhow::Error 변환.
    fn map_err<T>(r: openguild_core::AppResult<T>) -> Result<T> {
        r.map_err(|e| anyhow!("{}", e))
    }

    // ── 도메인 메서드 ──────────────────────────────────────

    fn ping(&self) -> Result<String> {
        match self {
            Backend::Http(c) => c.ping(),
            Backend::Local(l) => Ok(format!("local mode ({})", l.guild_path.display())),
        }
    }

    fn list_quests(&self, q: &ListQuery) -> Result<Vec<Quest>> {
        match self {
            Backend::Http(c) => c.list_quests(q),
            Backend::Local(l) => {
                Self::map_err(l.rt.block_on(quest_svc::list(&l.store.index_pool, q)))
            }
        }
    }

    fn list_deleted_quests(&self) -> Result<Vec<Quest>> {
        match self {
            Backend::Http(c) => c.list_deleted_quests(),
            Backend::Local(l) => {
                Self::map_err(l.rt.block_on(quest_svc::list_deleted(&l.store.index_pool)))
            }
        }
    }

    fn quest_by_slug(&self, slug: &str) -> Result<QuestDetail> {
        match self {
            Backend::Http(c) => c.quest_by_slug(slug),
            Backend::Local(l) => Self::map_err(
                l.rt.block_on(quest_svc::get_by_slug(&l.store.index_pool, slug)),
            ),
        }
    }

    fn list_quest_history(&self, id: i64) -> Result<Vec<openguild_core::models::QuestHistoryEntry>> {
        match self {
            Backend::Http(c) => c.list_quest_history(id),
            Backend::Local(l) => Self::map_err(
                l.rt.block_on(quest_svc::list_history(&l.store.index_pool, id)),
            ),
        }
    }

    fn quest_types(&self) -> Result<Vec<QuestType>> {
        match self {
            Backend::Http(c) => c.quest_types(),
            Backend::Local(l) => Self::map_err(
                l.rt.block_on(meta_svc::list_quest_types(&l.store.index_pool)),
            ),
        }
    }

    fn quest_statuses(&self) -> Result<Vec<QuestStatus>> {
        match self {
            Backend::Http(c) => c.quest_statuses(),
            Backend::Local(l) => Self::map_err(
                l.rt.block_on(meta_svc::list_quest_statuses(&l.store.index_pool)),
            ),
        }
    }

    // ── mutations: ops::* (파일 + journal + auto block) ──

    fn create_quest(&self, body: CreateQuestRequest) -> Result<Quest> {
        match self {
            Backend::Http(c) => c.create_quest(&body),
            Backend::Local(l) => Self::map_err(
                l.rt.block_on(openguild_core::ops::create_quest(&l.store, body)),
            ),
        }
    }

    fn update_quest(&self, id: i64, body: UpdateQuestRequest) -> Result<Quest> {
        match self {
            Backend::Http(c) => c.update_quest(id, &body),
            Backend::Local(l) => Self::map_err(
                l.rt.block_on(openguild_core::ops::update_quest(&l.store, id, body)),
            ),
        }
    }

    fn delete_quest(&self, id: i64, cascade_ids: &[i64]) -> Result<()> {
        match self {
            Backend::Http(c) => c.delete_quest(id, cascade_ids),
            Backend::Local(l) => Self::map_err(
                l.rt.block_on(openguild_core::ops::delete_quest(&l.store, id, cascade_ids)),
            ),
        }
    }

    fn restore_quest(&self, id: i64) -> Result<Quest> {
        match self {
            Backend::Http(c) => c.restore_quest(id),
            Backend::Local(l) => Self::map_err(
                l.rt.block_on(openguild_core::ops::restore_quest(&l.store, id)),
            ),
        }
    }

    fn change_status(&self, id: i64, status_id: i64) -> Result<Quest> {
        match self {
            Backend::Http(c) => c.change_status(id, status_id),
            Backend::Local(l) => Self::map_err(l.rt.block_on(
                openguild_core::ops::change_status(
                    &l.store,
                    id,
                    ChangeStatusRequest { status_id },
                ),
            )),
        }
    }

    fn change_parent(&self, id: i64, parent_id: Option<i64>) -> Result<Quest> {
        match self {
            Backend::Http(c) => c.change_parent(id, parent_id),
            Backend::Local(l) => Self::map_err(l.rt.block_on(
                openguild_core::ops::change_parent(
                    &l.store,
                    id,
                    ChangeParentRequest {
                        parent_quest_id: parent_id,
                    },
                ),
            )),
        }
    }

    fn add_prerequisite(&self, id: i64, prereq_id: i64) -> Result<()> {
        match self {
            Backend::Http(c) => c.add_prerequisite(id, prereq_id),
            Backend::Local(l) => Self::map_err(l.rt.block_on(
                openguild_core::ops::add_prerequisite(
                    &l.store,
                    id,
                    AddPrerequisiteRequest {
                        prerequisite_id: prereq_id,
                    },
                ),
            )),
        }
    }

    fn remove_prerequisite(&self, id: i64, prereq_id: i64) -> Result<()> {
        match self {
            Backend::Http(c) => c.remove_prerequisite(id, prereq_id),
            Backend::Local(l) => Self::map_err(l.rt.block_on(
                openguild_core::ops::remove_prerequisite(&l.store, id, prereq_id),
            )),
        }
    }

    // ── 백업 / 복원 ──────────────────────────────────────

    fn create_backup(&self) -> Result<openguild_core::snapshot::SnapshotInfo> {
        match self {
            Backend::Http(c) => c.create_snapshot(),
            Backend::Local(l) => {
                l.rt.block_on(openguild_core::snapshot::create_snapshot(&l.store))
            }
        }
    }

    fn list_backups(&self) -> Result<Vec<openguild_core::snapshot::SnapshotInfo>> {
        match self {
            Backend::Http(c) => c.list_snapshots(),
            Backend::Local(l) => openguild_core::snapshot::list_snapshots(&l.store.paths),
        }
    }

    fn restore_backup(&self, to: Option<String>) -> Result<openguild_core::snapshot::SnapshotInfo> {
        match self {
            Backend::Http(c) => c.restore_snapshot(to),
            Backend::Local(l) => {
                let snapshots = openguild_core::snapshot::list_snapshots(&l.store.paths)?;
                let target = if let Some(ts) = to {
                    snapshots
                        .iter()
                        .find(|s| s.timestamp == ts)
                        .cloned()
                        .ok_or_else(|| anyhow!("snapshot 없음: {ts}"))?
                } else {
                    snapshots
                        .last()
                        .cloned()
                        .ok_or_else(|| anyhow!("사용 가능한 snapshot 이 없습니다"))?
                };
                l.rt.block_on(openguild_core::snapshot::restore_snapshot(
                    &l.store, &target,
                ))?;
                Ok(target)
            }
        }
    }

    // ── 슬러그 → ID 헬퍼 ─────────────────────────────────

    fn id_of(&self, slug: &str) -> Result<i64> {
        Ok(self.quest_by_slug(slug)?.quest.id)
    }

    /// 상태 인자(이름 또는 ID) → status_id
    fn resolve_status_id(&self, input: &str) -> Result<i64> {
        if let Ok(n) = input.parse::<i64>() {
            return Ok(n);
        }
        let statuses = self.quest_statuses()?;
        match_status_id(input, &statuses).ok_or_else(|| {
            anyhow!(
                "unknown status '{input}'. available: {}",
                statuses
                    .iter()
                    .map(|s| s.name_en.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        })
    }

    fn resolve_type_id(&self, prefix: &str) -> Result<i64> {
        let types = self.quest_types()?;
        match_type_id(prefix, &types).ok_or_else(|| {
            anyhow!(
                "unknown type '{prefix}'. available: {}",
                types
                    .iter()
                    .map(|t| t.prefix.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        })
    }
}

// ─────────────────────────── 순수 매칭 헬퍼 ───────────────────────────

/// 상태 이름(name_en, 대소문자/공백 무시) 또는 ID → status_id.
fn match_status_id(input: &str, statuses: &[QuestStatus]) -> Option<i64> {
    if let Ok(n) = input.parse::<i64>() {
        return Some(n);
    }
    let want = input.to_lowercase();
    statuses
        .iter()
        .find(|s| {
            let en = s.name_en.to_lowercase();
            en == want || en.replace(' ', "_") == want || en.replace(' ', "-") == want
        })
        .map(|s| s.id)
}

/// `Vec<String>` (clap 다중 값) → `Some("a,b,c")` 또는 빈 Vec 이면 None.
fn vec_to_csv(v: Vec<String>) -> Option<String> {
    if v.is_empty() {
        None
    } else {
        Some(v.join(","))
    }
}

/// `ListQuery` → `?type=DEV,BUG&status=open&urgency=2&...` urlencoded querystring.
/// 모든 필드 미지정이면 빈 문자열 반환.
fn list_query_to_querystring(q: &ListQuery) -> String {
    let mut parts: Vec<String> = Vec::new();
    if let Some(t) = &q.r#type {
        parts.push(format!("type={}", urlencode(t)));
    }
    if let Some(s) = &q.status {
        parts.push(format!("status={}", urlencode(s)));
    }
    if let Some(u) = &q.urgency {
        parts.push(format!("urgency={}", urlencode(u)));
    }
    if let Some(v) = &q.created_after {
        parts.push(format!("created_after={}", urlencode(v)));
    }
    if let Some(v) = &q.created_before {
        parts.push(format!("created_before={}", urlencode(v)));
    }
    if let Some(v) = &q.updated_after {
        parts.push(format!("updated_after={}", urlencode(v)));
    }
    if let Some(v) = &q.updated_before {
        parts.push(format!("updated_before={}", urlencode(v)));
    }
    if let Some(c) = &q.child_of {
        parts.push(format!("child_of={}", urlencode(c)));
    }
    if q.no_parent {
        parts.push("no_parent=true".into());
    }
    if q.has_prereq {
        parts.push("has_prereq=true".into());
    }
    if q.no_prereq {
        parts.push("no_prereq=true".into());
    }
    if q.has_sub {
        parts.push("has_sub=true".into());
    }
    if q.no_sub {
        parts.push("no_sub=true".into());
    }
    if let Some(s) = &q.search {
        parts.push(format!("search={}", urlencode(s)));
    }
    if let Some(s) = &q.sort {
        parts.push(format!("sort={}", urlencode(s)));
    }
    if q.reverse {
        parts.push("reverse=true".into());
    }
    if let Some(l) = q.limit {
        parts.push(format!("limit={l}"));
    }
    if let Some(o) = q.offset {
        parts.push(format!("offset={o}"));
    }
    parts.join("&")
}

/// 최소 URL encoding — `quest list` 옵션은 ASCII / 일부 한글 정도. 영문/숫자/`-_.~`
/// 제외하고 모두 percent-encode. reqwest 의 url builder 쓰는 게 정석이지만 deps
/// 추가 피하려 직접 구현.
fn urlencode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for byte in s.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char);
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

/// 타입 prefix (대소문자 무시) → type_id.
fn match_type_id(prefix: &str, types: &[QuestType]) -> Option<i64> {
    let want = prefix.to_uppercase();
    types
        .iter()
        .find(|t| t.prefix.to_uppercase() == want)
        .map(|t| t.id)
}

// ─────────────────────────── 출력 ───────────────────────────

use std::io::IsTerminal;

/// 색깔 적용 여부 — TTY + NO_COLOR 미설정일 때만.
/// NO_COLOR 컨벤션: https://no-color.org/
fn use_color() -> bool {
    if std::env::var_os("NO_COLOR").is_some() {
        return false;
    }
    std::io::stdout().is_terminal()
}

fn hex_rgb(hex: &str) -> Option<(u8, u8, u8)> {
    let s = hex.trim_start_matches('#');
    if s.len() != 6 {
        return None;
    }
    let r = u8::from_str_radix(&s[0..2], 16).ok()?;
    let g = u8::from_str_radix(&s[2..4], 16).ok()?;
    let b = u8::from_str_radix(&s[4..6], 16).ok()?;
    Some((r, g, b))
}

/// ANSI 24-bit truecolor — `#RRGGBB` → "\x1b[38;2;r;g;bm{text}\x1b[0m".
/// 비-TTY / NO_COLOR 일 땐 plain 반환.
fn colorize(text: &str, hex: &str) -> String {
    if !use_color() {
        return text.to_string();
    }
    let Some((r, g, b)) = hex_rgb(hex) else {
        return text.to_string();
    };
    format!("\x1b[38;2;{r};{g};{b}m{text}\x1b[0m")
}

/// urgency 등급별 색 — GUI 의 URGENCY_COLOR 와 동일.
fn urgency_color(urgency: i64) -> &'static str {
    match urgency {
        1 => "#E94F4F", // Critical
        2 => "#F5A623", // High
        3 => "#F5D623", // Medium
        _ => "#8B95A1", // Low (4 또는 그 외)
    }
}

/// 기본 포맷 — 제목 한 줄만. list / status / parent 등에서 사용.
/// description 은 안 보임 (자세히는 `quest show <slug>`).
fn print_quest(q: &Quest, json: bool) {
    if json {
        println!("{}", serde_json::to_string_pretty(q).unwrap());
        return;
    }
    print_quest_line(q);
}

/// new / update 직후 — description 변경 결과를 사용자가 즉시 확인할 수 있도록
/// 전체 multi-line 표시.
fn print_quest_full(q: &Quest, json: bool) {
    if json {
        println!("{}", serde_json::to_string_pretty(q).unwrap());
        return;
    }
    print_quest_line(q);
    if let Some(d) = &q.description
        && !d.is_empty()
    {
        for line in d.lines() {
            println!("           {line}");
        }
    }
}

/// 한 줄 출력 공통 — 색깔 적용 시 truecolor 사용.
/// `DEV-005 [Testing] title (urgency 3)` 형태, 각 부분 색 분리.
fn print_quest_line(q: &Quest) {
    let id = colorize(&format!("{:<10}", q.quest_id), &q.type_color);
    let status_label = format!("[{}]", q.status_name_en);
    let status = colorize(&status_label, &q.status_color);
    let urg_color = urgency_color(q.urgency);
    let urgency = colorize(&format!("(urgency {})", q.urgency), urg_color);
    println!("{id} {status} {} {urgency}", q.title);
}

fn print_quest_list(quests: &[Quest], json: bool) {
    if json {
        println!("{}", serde_json::to_string_pretty(quests).unwrap());
        return;
    }
    if quests.is_empty() {
        println!("(no quests)");
        return;
    }
    for q in quests {
        print_quest(q, false);
    }
}

fn print_quest_detail(d: &QuestDetail, json: bool) {
    if json {
        println!("{}", serde_json::to_string_pretty(d).unwrap());
        return;
    }
    let q = &d.quest;
    println!("{}  {}", q.quest_id, q.title);
    println!("  status   : {} ({})", q.status_name_en, q.status_name_ko);
    println!("  urgency  : {}", q.urgency);
    if let Some(p) = q.parent_quest_id {
        println!("  parent   : id={p}");
    }
    if let Some(desc) = &q.description
        && !desc.is_empty()
    {
        println!("  description:");
        for line in desc.lines() {
            println!("    {line}");
        }
    }
    if !d.sub_quests.is_empty() {
        println!("  sub-quests ({}):", d.sub_quests.len());
        for s in &d.sub_quests {
            println!("    - {} {}", s.quest_id, s.title);
        }
    }
    if !d.prerequisites.is_empty() {
        println!("  prerequisites ({}):", d.prerequisites.len());
        for p in &d.prerequisites {
            println!("    - {} {}", p.quest_id, p.title);
        }
    }
}

// ─────────────────────────── 명령 처리 ───────────────────────────

fn run() -> Result<()> {
    let cli = Cli::parse();

    // Init 은 길드 자체를 만드는 명령 — 백엔드 연결 불필요. 먼저 처리.
    if let Command::Init { name } = &cli.command {
        return init_guild(name.clone(), cli.json);
    }

    let c = Backend::new(cli.remote.clone(), cli.guild.clone())?;

    match cli.command {
        Command::Init { .. } => unreachable!("handled above"),
        Command::Ping => {
            let s = c.ping()?;
            if cli.json {
                println!("{}", serde_json::json!({ "ok": true, "body": s }));
            } else {
                println!("ok ({s})");
            }
        }
        Command::Types => {
            let types = c.quest_types()?;
            if cli.json {
                println!("{}", serde_json::to_string_pretty(&types)?);
            } else {
                for t in &types {
                    println!("{:<6} {}", t.prefix, t.description.as_deref().unwrap_or(""));
                }
            }
        }
        Command::Statuses => {
            let statuses = c.quest_statuses()?;
            if cli.json {
                println!("{}", serde_json::to_string_pretty(&statuses)?);
            } else {
                for s in &statuses {
                    println!("{:<14} {}", s.name_en, s.name_ko);
                }
            }
        }
        Command::Backup => {
            let info = c.create_backup()?;
            if cli.json {
                println!(
                    "{}",
                    serde_json::json!({
                        "ok": true,
                        "timestamp": info.timestamp,
                        "size_bytes": info.size_bytes,
                        "path": info.path.to_string_lossy(),
                    })
                );
            } else {
                println!("✓ snapshot 생성: {} ({} bytes)", info.timestamp, info.size_bytes);
                println!("  path: {}", info.path.display());
            }
        }
        Command::Backups => {
            let list = c.list_backups()?;
            if cli.json {
                let arr: Vec<_> = list
                    .iter()
                    .map(|s| {
                        serde_json::json!({
                            "timestamp": s.timestamp,
                            "size_bytes": s.size_bytes,
                            "path": s.path.to_string_lossy(),
                        })
                    })
                    .collect();
                println!("{}", serde_json::to_string_pretty(&arr)?);
            } else if list.is_empty() {
                println!("(사용 가능한 백업 없음)");
                println!();
                println!("`openguild backup` 으로 생성하세요.");
            } else {
                println!("백업 목록 (오래된 순):");
                for s in &list {
                    println!("  {} — {} bytes", s.timestamp, s.size_bytes);
                }
            }
        }
        Command::Restore { to } => {
            let info = c.restore_backup(to)?;
            if cli.json {
                println!(
                    "{}",
                    serde_json::json!({
                        "ok": true,
                        "restored_to": info.timestamp,
                    })
                );
            } else {
                println!("✓ 복원 완료: {}", info.timestamp);
                println!();
                println!("주의: 파일 시스템 (`.guild/quests/*.md`) 자동 갱신 안 됨.");
                println!("      필요시 `openguild-server reindex`.");
            }
        }
        Command::Quest { sub } => match sub {
            QuestCmd::List {
                type_prefix,
                status,
                urgency,
                created_after,
                created_before,
                updated_after,
                updated_before,
                child_of,
                no_parent,
                has_prereq,
                no_prereq,
                has_sub,
                no_sub,
                search,
                sort,
                reverse,
                limit,
                offset,
                id_only,
                count,
            } => {
                let q = ListQuery {
                    r#type: vec_to_csv(type_prefix),
                    status: vec_to_csv(status),
                    urgency,
                    created_after,
                    created_before,
                    updated_after,
                    updated_before,
                    child_of,
                    no_parent,
                    has_prereq,
                    no_prereq,
                    has_sub,
                    no_sub,
                    search,
                    sort: vec_to_csv(sort),
                    reverse,
                    limit,
                    offset,
                };
                let quests = c.list_quests(&q)?;
                if count {
                    println!("{}", quests.len());
                } else if id_only {
                    for q in &quests {
                        println!("{}", q.quest_id);
                    }
                } else {
                    print_quest_list(&quests, cli.json);
                }
            }
            QuestCmd::Show { slug } => {
                let d = c.quest_by_slug(&slug)?;
                print_quest_detail(&d, cli.json);
            }
            QuestCmd::History { slug } => {
                let d = c.quest_by_slug(&slug)?;
                let history = c.list_quest_history(d.quest.id)?;
                if cli.json {
                    println!("{}", serde_json::to_string_pretty(&history).unwrap());
                } else if history.is_empty() {
                    println!("(이력 없음)");
                } else {
                    for h in &history {
                        let old = h.old_value.as_deref().unwrap_or("∅");
                        let new = h.new_value.as_deref().unwrap_or("∅");
                        println!("{}  {:<14} {} → {}", h.ts, h.op, old, new);
                    }
                }
            }
            QuestCmd::New {
                type_prefix,
                title,
                description,
                urgency,
                parent,
            } => {
                let type_id = c.resolve_type_id(&type_prefix)?;
                let statuses = c.quest_statuses()?;
                let open_status = statuses
                    .iter()
                    .min_by_key(|s| s.sort_order)
                    .ok_or_else(|| anyhow!("no quest statuses defined"))?;
                let parent_id = match parent {
                    Some(p) => Some(c.id_of(&p)?),
                    None => None,
                };
                let body = CreateQuestRequest {
                    quest_type_id: type_id,
                    title,
                    description,
                    status_id: open_status.id,
                    urgency: Some(urgency),
                    parent_quest_id: parent_id,
                };
                let q = c.create_quest(body)?;
                // multi-line description 도 그대로 보여줘 사용자가 "잘렸다" 오해 방지.
                print_quest_full(&q, cli.json);
            }
            QuestCmd::Update {
                slug,
                title,
                description,
                urgency,
                dry_run,
            } => {
                let detail = c.quest_by_slug(&slug)?;
                let id = detail.quest.id;

                if dry_run {
                    if cli.json {
                        let mut diff = serde_json::Map::new();
                        if let Some(t) = &title {
                            diff.insert(
                                "title".into(),
                                serde_json::json!({ "from": detail.quest.title, "to": t }),
                            );
                        }
                        if let Some(d) = &description {
                            diff.insert(
                                "description".into(),
                                serde_json::json!({
                                    "from": detail.quest.description,
                                    "to": d,
                                }),
                            );
                        }
                        if let Some(u) = urgency {
                            diff.insert(
                                "urgency".into(),
                                serde_json::json!({ "from": detail.quest.urgency, "to": u }),
                            );
                        }
                        println!(
                            "{}",
                            serde_json::json!({
                                "dry_run": true,
                                "slug": slug,
                                "changes": diff,
                            })
                        );
                    } else {
                        println!("[dry-run] update {}", slug);
                        if let Some(t) = &title {
                            println!("  title:       {:?} → {:?}", detail.quest.title, t);
                        }
                        if let Some(d) = &description {
                            let from = detail.quest.description.as_deref().unwrap_or("");
                            println!("  description: {:?} → {:?}", from, d);
                        }
                        if let Some(u) = urgency {
                            println!("  urgency:     {} → {}", detail.quest.urgency, u);
                        }
                        if title.is_none() && description.is_none() && urgency.is_none() {
                            println!("  (no fields to change)");
                        }
                    }
                    return Ok(());
                }

                let body = UpdateQuestRequest {
                    title,
                    description,
                    urgency,
                };
                let q = c.update_quest(id, body)?;
                // description 변경 가능성 있음 → multi-line 전체 표시.
                print_quest_full(&q, cli.json);
            }
            QuestCmd::Delete {
                slug,
                cascade,
                dry_run,
                yes,
            } => {
                // 어떤 영향이 있는지 detail 로 미리 본다 (dry-run / 사용자 확인용)
                let detail = c.quest_by_slug(&slug)?;
                let cascade_set: std::collections::HashSet<&str> =
                    cascade.iter().map(|s| s.as_str()).collect();
                let cascade_subs: Vec<&Quest> = detail
                    .sub_quests
                    .iter()
                    .filter(|s| cascade_set.contains(s.quest_id.as_str()))
                    .collect();
                let detached_subs: Vec<&Quest> = detail
                    .sub_quests
                    .iter()
                    .filter(|s| !cascade_set.contains(s.quest_id.as_str()))
                    .collect();
                let actual_subs: std::collections::HashSet<&str> =
                    detail.sub_quests.iter().map(|s| s.quest_id.as_str()).collect();
                let invalid_cascade: Vec<&String> = cascade
                    .iter()
                    .filter(|c| !actual_subs.contains(c.as_str()))
                    .collect();

                // dry-run: 무조건 출력만, 변경 X
                if dry_run {
                    if cli.json {
                        println!(
                            "{}",
                            serde_json::json!({
                                "dry_run": true,
                                "would_delete": detail.quest.quest_id,
                                "cascade_delete": cascade_subs.iter().map(|s| &s.quest_id).collect::<Vec<_>>(),
                                "detach_children": detached_subs.iter().map(|s| &s.quest_id).collect::<Vec<_>>(),
                                "unaffected_prerequisites": detail.prerequisites.iter().map(|s| &s.quest_id).collect::<Vec<_>>(),
                                "invalid_cascade": invalid_cascade,
                            })
                        );
                    } else {
                        println!(
                            "[dry-run] would delete {} ({})",
                            detail.quest.quest_id, detail.quest.title
                        );
                        if !cascade_subs.is_empty() {
                            println!(
                                "[dry-run] cascade delete: {}",
                                cascade_subs
                                    .iter()
                                    .map(|s| s.quest_id.as_str())
                                    .collect::<Vec<_>>()
                                    .join(", ")
                            );
                        }
                        if !detached_subs.is_empty() {
                            println!(
                                "[dry-run] detach (parent → null): {}",
                                detached_subs
                                    .iter()
                                    .map(|s| s.quest_id.as_str())
                                    .collect::<Vec<_>>()
                                    .join(", ")
                            );
                        }
                        if !detail.prerequisites.is_empty() {
                            println!(
                                "[dry-run] unaffected prerequisites: {}",
                                detail
                                    .prerequisites
                                    .iter()
                                    .map(|s| s.quest_id.as_str())
                                    .collect::<Vec<_>>()
                                    .join(", ")
                            );
                        }
                        for c in &invalid_cascade {
                            eprintln!(
                                "warning: cascade target '{c}' is not a direct child of {}",
                                detail.quest.quest_id
                            );
                        }
                    }
                    return Ok(());
                }

                // 실제 삭제는 --yes 필수
                if !yes {
                    return Err(anyhow!(
                        "delete 는 위험한 작업입니다. 영향 확인은 --dry-run, 실제 실행은 --yes 를 명시하세요."
                    ));
                }

                let id = detail.quest.id;
                let mut cascade_ids = Vec::new();
                for s in &cascade {
                    cascade_ids.push(c.id_of(s)?);
                }
                c.delete_quest(id, &cascade_ids)?;
                if cli.json {
                    println!(
                        "{}",
                        serde_json::json!({
                            "ok": true,
                            "deleted": slug,
                            "cascade_deleted": cascade_subs.iter().map(|s| &s.quest_id).collect::<Vec<_>>(),
                            "detached": detached_subs.iter().map(|s| &s.quest_id).collect::<Vec<_>>(),
                        })
                    );
                } else {
                    println!("deleted {slug}");
                    if !cascade_subs.is_empty() {
                        println!(
                            "  cascade-deleted: {}",
                            cascade_subs
                                .iter()
                                .map(|s| s.quest_id.as_str())
                                .collect::<Vec<_>>()
                                .join(", ")
                        );
                    }
                    if !detached_subs.is_empty() {
                        println!(
                            "  detached: {}",
                            detached_subs
                                .iter()
                                .map(|s| s.quest_id.as_str())
                                .collect::<Vec<_>>()
                                .join(", ")
                        );
                    }
                }
            }
            QuestCmd::Deleted => {
                let quests = c.list_deleted_quests()?;
                print_quest_list(&quests, cli.json);
            }
            QuestCmd::Restore { slug } => {
                // alive 목록엔 없으니 deleted 목록에서 slug → id 매칭
                let deleted = c.list_deleted_quests()?;
                let q = deleted
                    .iter()
                    .find(|q| q.quest_id == slug)
                    .ok_or_else(|| {
                        anyhow!("'{slug}' is not in the deleted list (또는 이미 alive)")
                    })?;
                let restored = c.restore_quest(q.id)?;
                print_quest(&restored, cli.json);
            }
            QuestCmd::Status { slug, status } => {
                let id = c.id_of(&slug)?;
                let status_id = c.resolve_status_id(&status)?;
                let q = c.change_status(id, status_id)?;
                print_quest(&q, cli.json);
            }
            QuestCmd::Start { slug } => {
                let id = c.id_of(&slug)?;
                let status_id = c.resolve_status_id("In Progress")?;
                let q = c.change_status(id, status_id)?;
                print_quest(&q, cli.json);
            }
            QuestCmd::Done { slug } => {
                let id = c.id_of(&slug)?;
                let status_id = c.resolve_status_id("Done")?;
                let q = c.change_status(id, status_id)?;
                print_quest(&q, cli.json);
            }
            QuestCmd::Reopen { slug } => {
                let id = c.id_of(&slug)?;
                let status_id = c.resolve_status_id("Open")?;
                let q = c.change_status(id, status_id)?;
                print_quest(&q, cli.json);
            }
            QuestCmd::Parent {
                slug,
                parent,
                detach,
            } => {
                if detach && parent.is_some() {
                    return Err(anyhow!("--detach 와 parent 인자를 동시에 사용할 수 없음"));
                }
                let id = c.id_of(&slug)?;
                let parent_id = if detach {
                    None
                } else {
                    match parent {
                        Some(p) => Some(c.id_of(&p)?),
                        None => {
                            return Err(anyhow!(
                                "부모 슬러그를 지정하거나 --detach 를 사용하세요"
                            ))
                        }
                    }
                };
                let q = c.change_parent(id, parent_id)?;
                print_quest(&q, cli.json);
            }
            QuestCmd::Prereq { sub } => match sub {
                PrereqCmd::Add { slug, prereq } => {
                    let id = c.id_of(&slug)?;
                    let pid = c.id_of(&prereq)?;
                    c.add_prerequisite(id, pid)?;
                    if cli.json {
                        println!(
                            "{}",
                            serde_json::json!({ "ok": true, "added": prereq, "to": slug })
                        );
                    } else {
                        println!("{slug} prereq + {prereq}");
                    }
                }
                PrereqCmd::Rm { slug, prereq } => {
                    let id = c.id_of(&slug)?;
                    let pid = c.id_of(&prereq)?;
                    c.remove_prerequisite(id, pid)?;
                    if cli.json {
                        println!(
                            "{}",
                            serde_json::json!({ "ok": true, "removed": prereq, "from": slug })
                        );
                    } else {
                        println!("{slug} prereq - {prereq}");
                    }
                }
            },
        },
    }
    Ok(())
}

// ─────────────────────────── init ───────────────────────────

/// 현재 디렉토리를 길드로 초기화. `<name>.guild` 마커 파일 생성.
fn init_guild(name_arg: Option<String>, json: bool) -> Result<()> {
    let cwd = std::env::current_dir().context("현재 디렉토리를 확인할 수 없음")?;
    let (guild_path, name) = init_guild_at(&cwd, name_arg)?;

    if json {
        println!(
            "{}",
            serde_json::json!({
                "ok": true,
                "guild_path": guild_path.to_string_lossy(),
                "name": name,
            })
        );
    } else {
        // 서버 / gui 안내는 출력 X — cli 의 책임 범위 밖.
        println!("✓ guild created: {}", guild_path.display());
        println!("  name: {name}");
    }
    Ok(())
}

/// 순수 로직 — 디렉토리 경로를 받아 `.guild` 파일 + `.guild/` 디렉토리 구조 작성.
/// (마커 파일 경로, 길드 이름) 반환.
///
/// idempotent:
/// - 마커 (`{name}.guild`) 가 이미 있으면 건드리지 않고 기존 이름 사용.
/// - `.guild/` 와 시드 (types/statuses) 가 있으면 건드리지 않음.
/// - 둘 다 있으면 essentially no-op (성공 반환).
/// - 부분 상태 (마커만 있고 `.guild/` 없음) 자동 업그레이드.
///
/// 단위 테스트에서 tempdir 로 직접 호출 가능 (cwd 의존성 없음).
fn init_guild_at(
    cwd: &std::path::Path,
    name_arg: Option<String>,
) -> Result<(std::path::PathBuf, String)> {
    // 기존 마커 검색.
    let existing_marker = std::fs::read_dir(cwd)
        .context("디렉토리 읽기 실패")?
        .filter_map(|e| e.ok())
        .find(|e| {
            e.path().is_file()
                && e.file_name()
                    .to_string_lossy()
                    .to_lowercase()
                    .ends_with(".guild")
        });

    let (guild_path, name) = if let Some(entry) = existing_marker {
        // 이미 마커 있음 — 그대로 사용 (--name 지정해도 무시, 기존 이름 보존).
        let path = entry.path();
        let parsed = openguild_core::guild_file::load(cwd.to_str().ok_or_else(|| {
            anyhow!("디렉토리 경로 인코딩 오류: {}", cwd.display())
        })?)
        .with_context(|| format!("기존 마커 파싱 실패: {}", path.display()))?;
        if let Some(arg) = &name_arg
            && arg != &parsed.name
        {
            eprintln!(
                "ℹ︎ 기존 길드 이름 보존: \"{}\" (--name \"{}\" 무시)",
                parsed.name, arg
            );
        }
        (path, parsed.name)
    } else {
        // 새 마커 생성.
        let name = match name_arg {
            Some(n) => n,
            None => cwd
                .file_name()
                .and_then(|s| s.to_str())
                .ok_or_else(|| anyhow!("현재 디렉토리 이름을 추출할 수 없음. --name 으로 지정하세요."))?
                .to_string(),
        };
        let guild_path = cwd.join(format!("{name}.guild"));
        let today = today_date();
        let content = format!(
            "name = \"{}\"\nversion = \"1.0\"\ncreated_at = \"{}\"\n",
            name.replace('\\', "\\\\").replace('"', "\\\""),
            today
        );
        std::fs::write(&guild_path, content)
            .with_context(|| format!("길드 파일 작성 실패: {}", guild_path.display()))?;
        (guild_path, name)
    };

    // .guild/ 디렉토리 + 기본 시드 (types/statuses) + .gitignore.
    // idempotent — 이미 있는 파일은 건드리지 않음.
    openguild_core::repo::seed_guild_dir(cwd)
        .with_context(|| format!(".guild/ 시드 실패: {}", cwd.display()))?;

    Ok((guild_path, name))
}

/// 오늘 날짜 (YYYY-MM-DD, UTC). chrono 의존 없이 epoch → 분해.
fn today_date() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let mut days = (secs / 86400) as i64;
    let mut year: i64 = 1970;
    loop {
        let dy = if is_leap_init(year) { 366 } else { 365 };
        if days >= dy {
            days -= dy;
            year += 1;
        } else {
            break;
        }
    }
    let dim = days_in_months_init(year);
    let mut month = 0usize;
    while month < 12 && days >= dim[month] as i64 {
        days -= dim[month] as i64;
        month += 1;
    }
    format!("{:04}-{:02}-{:02}", year, month + 1, days + 1)
}

fn is_leap_init(y: i64) -> bool {
    (y % 4 == 0 && y % 100 != 0) || y % 400 == 0
}
fn days_in_months_init(y: i64) -> [u32; 12] {
    [
        31,
        if is_leap_init(y) { 29 } else { 28 },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ]
}

fn main() {
    if let Err(e) = run() {
        eprintln!("error: {e:#}");
        std::process::exit(1);
    }
}

// ─────────────────────────── 테스트 ───────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn st(id: i64, en: &str) -> QuestStatus {
        QuestStatus {
            id,
            name_en: en.into(),
            name_ko: "".into(),
            color: "".into(),
            sort_order: 0,
        }
    }
    fn ty(id: i64, prefix: &str) -> QuestType {
        QuestType {
            id,
            prefix: prefix.into(),
            color: "".into(),
            description: None,
        }
    }

    // === 매칭 헬퍼 ===

    #[test]
    fn status_match_by_id() {
        let list = vec![st(1, "Open"), st(2, "In Progress")];
        assert_eq!(match_status_id("1", &list), Some(1));
        assert_eq!(match_status_id("2", &list), Some(2));
    }

    #[test]
    fn status_match_by_name_case_insensitive() {
        let list = vec![st(1, "Open"), st(2, "In Progress")];
        assert_eq!(match_status_id("open", &list), Some(1));
        assert_eq!(match_status_id("OPEN", &list), Some(1));
        assert_eq!(match_status_id("In Progress", &list), Some(2));
    }

    #[test]
    fn status_match_with_separators() {
        let list = vec![st(2, "In Progress")];
        assert_eq!(match_status_id("in_progress", &list), Some(2));
        assert_eq!(match_status_id("in-progress", &list), Some(2));
    }

    #[test]
    fn status_match_unknown() {
        let list = vec![st(1, "Open")];
        assert_eq!(match_status_id("Closed", &list), None);
    }

    #[test]
    fn type_match_case_insensitive() {
        let list = vec![ty(1, "DEV"), ty(2, "BUG")];
        assert_eq!(match_type_id("dev", &list), Some(1));
        assert_eq!(match_type_id("Bug", &list), Some(2));
        assert_eq!(match_type_id("BUG", &list), Some(2));
    }

    #[test]
    fn type_match_unknown() {
        let list = vec![ty(1, "DEV")];
        assert_eq!(match_type_id("REQ", &list), None);
    }

    // === CLI 파서 ===

    #[test]
    fn cli_parse_quest_list() {
        let cli = Cli::try_parse_from(["openguild", "quest", "list"]).unwrap();
        match cli.command {
            Command::Quest {
                sub: QuestCmd::List {
                    type_prefix,
                    status,
                    urgency,
                    created_after,
                    created_before,
                    updated_after,
                    updated_before,
                    child_of,
                    no_parent,
                    has_prereq,
                    no_prereq,
                    has_sub,
                    no_sub,
                    search,
                    sort,
                    reverse,
                    limit,
                    offset,
                    id_only,
                    count,
                },
            } => {
                assert!(type_prefix.is_empty());
                assert!(status.is_empty());
                assert!(urgency.is_none());
                assert!(created_after.is_none());
                assert!(created_before.is_none());
                assert!(updated_after.is_none());
                assert!(updated_before.is_none());
                assert!(child_of.is_none());
                assert!(!no_parent);
                assert!(!has_prereq);
                assert!(!no_prereq);
                assert!(!has_sub);
                assert!(!no_sub);
                assert!(search.is_none());
                assert!(sort.is_empty());
                assert!(!reverse);
                assert!(limit.is_none());
                assert!(offset.is_none());
                assert!(!id_only);
                assert!(!count);
            }
            _ => panic!("expected QuestCmd::List"),
        }
        assert!(!cli.json);
    }

    // === DEV-027: quest list 필터 / 정렬 / limit ===

    #[test]
    fn cli_parse_list_type_single() {
        let cli = Cli::try_parse_from([
            "openguild", "quest", "list", "--type", "DEV",
        ]).unwrap();
        match cli.command {
            Command::Quest { sub: QuestCmd::List { type_prefix, .. } } => {
                assert_eq!(type_prefix, vec!["DEV"]);
            }
            _ => panic!(),
        }
    }

    #[test]
    fn cli_parse_list_type_multi_repeated() {
        // --type DEV --type BUG (repeated)
        let cli = Cli::try_parse_from([
            "openguild", "quest", "list", "--type", "DEV", "--type", "BUG",
        ]).unwrap();
        match cli.command {
            Command::Quest { sub: QuestCmd::List { type_prefix, .. } } => {
                assert_eq!(type_prefix, vec!["DEV", "BUG"]);
            }
            _ => panic!(),
        }
    }

    #[test]
    fn cli_parse_list_type_multi_csv() {
        // --type DEV,BUG (comma-delimited)
        let cli = Cli::try_parse_from([
            "openguild", "quest", "list", "--type", "DEV,BUG",
        ]).unwrap();
        match cli.command {
            Command::Quest { sub: QuestCmd::List { type_prefix, .. } } => {
                assert_eq!(type_prefix, vec!["DEV", "BUG"]);
            }
            _ => panic!(),
        }
    }

    #[test]
    fn cli_parse_list_type_multi_whitespace() {
        // --type DEV BUG (공백 구분, num_args(1..)).
        let cli = Cli::try_parse_from([
            "openguild", "quest", "list", "--type", "DEV", "BUG",
        ]).unwrap();
        match cli.command {
            Command::Quest { sub: QuestCmd::List { type_prefix, .. } } => {
                assert_eq!(type_prefix, vec!["DEV", "BUG"]);
            }
            _ => panic!(),
        }
    }

    #[test]
    fn cli_parse_list_sort_multi_csv() {
        let cli = Cli::try_parse_from([
            "openguild", "quest", "list", "--sort", "urgency,id",
        ]).unwrap();
        match cli.command {
            Command::Quest { sub: QuestCmd::List { sort, .. } } => {
                assert_eq!(sort, vec!["urgency", "id"]);
            }
            _ => panic!(),
        }
    }

    #[test]
    fn cli_parse_list_sort_multi_whitespace() {
        let cli = Cli::try_parse_from([
            "openguild", "quest", "list", "--sort", "urgency", "id",
        ]).unwrap();
        match cli.command {
            Command::Quest { sub: QuestCmd::List { sort, .. } } => {
                assert_eq!(sort, vec!["urgency", "id"]);
            }
            _ => panic!(),
        }
    }

    #[test]
    fn cli_parse_list_all_new_options() {
        let cli = Cli::try_parse_from([
            "openguild", "quest", "list",
            "--type", "BUG",
            "--status", "in_progress",
            "--urgency", "2",
            "--child-of", "DEV-001",
            "--sort", "urgency",
            "--reverse",
            "--limit", "5",
            "--offset", "10",
        ]).unwrap();
        match cli.command {
            Command::Quest {
                sub: QuestCmd::List {
                    type_prefix, status, urgency,
                    created_after, created_before, updated_after, updated_before,
                    child_of, no_parent,
                    has_prereq, no_prereq, has_sub, no_sub,
                    search,
                    sort, reverse, limit, offset, id_only, count,
                },
            } => {
                assert_eq!(type_prefix, vec!["BUG"]);
                assert_eq!(status, vec!["in_progress"]);
                assert_eq!(urgency.as_deref(), Some("2"));
                assert!(created_after.is_none());
                assert!(created_before.is_none());
                assert!(updated_after.is_none());
                assert!(updated_before.is_none());
                assert_eq!(child_of.as_deref(), Some("DEV-001"));
                assert!(!no_parent);
                assert!(!has_prereq);
                assert!(!no_prereq);
                assert!(!has_sub);
                assert!(!no_sub);
                assert!(search.is_none());
                assert_eq!(sort, vec!["urgency"]);
                assert!(reverse);
                assert_eq!(limit, Some(5));
                assert_eq!(offset, Some(10));
                assert!(!id_only);
                assert!(!count);
            }
            _ => panic!(),
        }
    }

    #[test]
    fn cli_parse_list_child_of_and_no_parent_conflict() {
        // clap 가 conflicts_with 로 자동 에러.
        let res = Cli::try_parse_from([
            "openguild", "quest", "list", "--child-of", "DEV-001", "--no-parent",
        ]);
        assert!(res.is_err());
    }

    #[test]
    fn cli_parse_list_id_only_and_count_conflict() {
        let res = Cli::try_parse_from([
            "openguild", "quest", "list", "--id-only", "--count",
        ]);
        assert!(res.is_err());
    }

    // === querystring 빌더 ===

    #[test]
    fn querystring_empty_when_no_fields() {
        let q = ListQuery::default();
        assert_eq!(list_query_to_querystring(&q), "");
    }

    #[test]
    fn querystring_single_type() {
        let q = ListQuery {
            r#type: Some("DEV".into()),
            ..Default::default()
        };
        assert_eq!(list_query_to_querystring(&q), "type=DEV");
    }

    #[test]
    fn querystring_multi_type_csv() {
        let q = ListQuery {
            r#type: Some("DEV,BUG".into()),
            ..Default::default()
        };
        assert_eq!(list_query_to_querystring(&q), "type=DEV%2CBUG");
    }

    #[test]
    fn querystring_multi_sort_csv() {
        let q = ListQuery {
            sort: Some("urgency,id".into()),
            ..Default::default()
        };
        assert_eq!(list_query_to_querystring(&q), "sort=urgency%2Cid");
    }

    #[test]
    fn querystring_all_new_fields() {
        let q = ListQuery {
            r#type: Some("BUG".into()),
            status: Some("in_progress".into()),
            urgency: Some("2".into()),
            created_after: None,
            created_before: None,
            updated_after: None,
            updated_before: None,
            child_of: Some("DEV-001".into()),
            no_parent: false,
            has_prereq: false,
            no_prereq: false,
            has_sub: false,
            no_sub: false,
            search: None,
            sort: Some("urgency".into()),
            reverse: true,
            limit: Some(5),
            offset: Some(10),
        };
        let qs = list_query_to_querystring(&q);
        assert_eq!(
            qs,
            "type=BUG&status=in_progress&urgency=2&child_of=DEV-001\
             &sort=urgency&reverse=true&limit=5&offset=10"
        );
    }

    #[test]
    fn querystring_no_parent_flag() {
        let q = ListQuery { no_parent: true, ..Default::default() };
        assert_eq!(list_query_to_querystring(&q), "no_parent=true");
    }

    #[test]
    fn querystring_special_chars_encoded() {
        let q = ListQuery {
            status: Some("In Progress".into()),
            ..Default::default()
        };
        assert_eq!(list_query_to_querystring(&q), "status=In%20Progress");
    }

    #[test]
    fn vec_to_csv_empty() {
        assert_eq!(vec_to_csv(Vec::new()), None);
    }

    #[test]
    fn vec_to_csv_joins() {
        assert_eq!(vec_to_csv(vec!["DEV".into(), "BUG".into()]), Some("DEV,BUG".into()));
    }

    #[test]
    fn urlencode_alphanum_unchanged() {
        assert_eq!(urlencode("DEV-001"), "DEV-001");
        assert_eq!(urlencode("abc.123_xyz~"), "abc.123_xyz~");
    }

    #[test]
    fn urlencode_special_chars() {
        assert_eq!(urlencode(" "), "%20");
        assert_eq!(urlencode("&="), "%26%3D");
        assert_eq!(urlencode("In Progress"), "In%20Progress");
    }

    #[test]
    fn cli_parse_global_json_and_remote() {
        let cli = Cli::try_parse_from([
            "openguild",
            "--json",
            "--remote",
            "http://example.com",
            "quest",
            "list",
        ])
        .unwrap();
        assert!(cli.json);
        assert_eq!(cli.remote.as_deref(), Some("http://example.com"));
    }

    #[test]
    fn cli_parse_guild_flag() {
        let cli =
            Cli::try_parse_from(["openguild", "--guild", "./monitor", "quest", "list"]).unwrap();
        assert_eq!(cli.guild.as_deref(), Some("./monitor"));
        assert!(cli.remote.is_none());
    }

    #[test]
    fn cli_parse_quest_new_minimal() {
        let cli =
            Cli::try_parse_from(["openguild", "quest", "new", "--type", "DEV", "--title", "test"])
                .unwrap();
        match cli.command {
            Command::Quest {
                sub:
                    QuestCmd::New {
                        type_prefix,
                        title,
                        urgency,
                        parent,
                        description,
                    },
            } => {
                assert_eq!(type_prefix, "DEV");
                assert_eq!(title, "test");
                assert_eq!(urgency, 3); // default
                assert!(parent.is_none());
                assert!(description.is_none());
            }
            _ => panic!("expected quest new"),
        }
    }

    #[test]
    fn cli_parse_quest_new_full() {
        let cli = Cli::try_parse_from([
            "openguild", "quest", "new",
            "--type", "BUG",
            "--title", "fix",
            "--description", "details",
            "--urgency", "1",
            "--parent", "DEV-007",
        ])
        .unwrap();
        match cli.command {
            Command::Quest {
                sub:
                    QuestCmd::New {
                        type_prefix,
                        title,
                        urgency,
                        parent,
                        description,
                    },
            } => {
                assert_eq!(type_prefix, "BUG");
                assert_eq!(title, "fix");
                assert_eq!(urgency, 1);
                assert_eq!(parent.as_deref(), Some("DEV-007"));
                assert_eq!(description.as_deref(), Some("details"));
            }
            _ => panic!("expected quest new"),
        }
    }

    #[test]
    fn cli_parse_quest_delete_with_cascade() {
        let cli = Cli::try_parse_from([
            "openguild",
            "quest",
            "delete",
            "DEV-001",
            "--cascade",
            "DEV-002,DEV-003",
            "--yes",
        ])
        .unwrap();
        match cli.command {
            Command::Quest {
                sub:
                    QuestCmd::Delete {
                        slug,
                        cascade,
                        dry_run,
                        yes,
                    },
            } => {
                assert_eq!(slug, "DEV-001");
                assert_eq!(cascade, vec!["DEV-002".to_string(), "DEV-003".to_string()]);
                assert!(yes);
                assert!(!dry_run);
            }
            _ => panic!("expected quest delete"),
        }
    }

    #[test]
    fn cli_parse_quest_delete_dry_run() {
        let cli = Cli::try_parse_from(["openguild", "quest", "delete", "DEV-001", "--dry-run"]).unwrap();
        match cli.command {
            Command::Quest {
                sub: QuestCmd::Delete { dry_run, yes, .. },
            } => {
                assert!(dry_run);
                assert!(!yes);
            }
            _ => panic!("expected quest delete"),
        }
    }

    #[test]
    fn cli_parse_quest_delete_default_no_yes_no_dryrun() {
        // 안전장치 검증: 기본값에서 yes / dry_run 모두 false 여야 한다
        let cli = Cli::try_parse_from(["openguild", "quest", "delete", "DEV-001"]).unwrap();
        match cli.command {
            Command::Quest {
                sub: QuestCmd::Delete { dry_run, yes, .. },
            } => {
                assert!(!dry_run);
                assert!(!yes);
                // run() 단계에서 둘 다 false 면 에러를 반환하도록 핸들러가 막음
            }
            _ => panic!("expected quest delete"),
        }
    }

    #[test]
    fn cli_parse_parent_with_detach_flag() {
        // 파서 단계에선 detach + parent 동시 허용. 의미 검증은 run() 에서.
        let cli =
            Cli::try_parse_from(["openguild", "quest", "parent", "DEV-001", "--detach"]).unwrap();
        match cli.command {
            Command::Quest {
                sub:
                    QuestCmd::Parent {
                        slug,
                        parent,
                        detach,
                    },
            } => {
                assert_eq!(slug, "DEV-001");
                assert!(detach);
                assert!(parent.is_none());
            }
            _ => panic!("expected quest parent"),
        }
    }

    #[test]
    fn cli_parse_prereq_subcommand() {
        let cli = Cli::try_parse_from([
            "openguild", "quest", "prereq", "add", "DEV-001", "DEV-002",
        ])
        .unwrap();
        match cli.command {
            Command::Quest {
                sub: QuestCmd::Prereq { sub: PrereqCmd::Add { slug, prereq } },
            } => {
                assert_eq!(slug, "DEV-001");
                assert_eq!(prereq, "DEV-002");
            }
            _ => panic!("expected prereq add"),
        }
    }

    // === DTO deserialize ===

    #[test]
    fn quest_dto_deserialize() {
        let json = r##"{
            "id": 1,
            "quest_id": "DEV-001",
            "quest_type_id": 1,
            "type_prefix": "DEV",
            "type_color": "#4A90D9",
            "number": 1,
            "title": "test",
            "description": null,
            "status_id": 1,
            "status_name_en": "Open",
            "status_name_ko": "게시됨",
            "status_color": "#8B95A1",
            "urgency": 3,
            "parent_quest_id": null,
            "created_at": "2024-01-01",
            "updated_at": "2024-01-01"
        }"##;
        let q: Quest = serde_json::from_str(json).unwrap();
        assert_eq!(q.quest_id, "DEV-001");
        assert_eq!(q.urgency, 3);
        assert!(q.parent_quest_id.is_none());
    }

    #[test]
    fn quest_detail_deserialize_with_relations() {
        let json = r##"{
            "id": 1, "quest_id": "DEV-001", "quest_type_id": 1, "type_prefix": "DEV",
            "type_color": "#4A90D9", "number": 1, "title": "p", "description": null,
            "status_id": 1, "status_name_en": "Open", "status_name_ko": "게시됨",
            "status_color": "#8B95A1", "urgency": 3, "parent_quest_id": null,
            "created_at": "", "updated_at": "",
            "sub_quests": [{
                "id": 2, "quest_id": "DEV-002", "quest_type_id": 1, "type_prefix": "DEV",
                "type_color": "#4A90D9", "number": 2, "title": "child", "description": null,
                "status_id": 1, "status_name_en": "Open", "status_name_ko": "게시됨",
                "status_color": "#8B95A1", "urgency": 3, "parent_quest_id": 1,
                "created_at": "", "updated_at": ""
            }],
            "prerequisites": []
        }"##;
        let d: QuestDetail = serde_json::from_str(json).unwrap();
        assert_eq!(d.quest.id, 1);
        assert_eq!(d.sub_quests.len(), 1);
        assert_eq!(d.sub_quests[0].parent_quest_id, Some(1));
        assert!(d.prerequisites.is_empty());
    }

    #[test]
    fn quest_status_dto_deserialize() {
        let json = r##"[
            {"id":1,"name_en":"Open","name_ko":"게시됨","color":"#8B95A1","sort_order":0},
            {"id":2,"name_en":"In Progress","name_ko":"진행 중","color":"#4A90D9","sort_order":1}
        ]"##;
        let v: Vec<QuestStatus> = serde_json::from_str(json).unwrap();
        assert_eq!(v.len(), 2);
        assert_eq!(v[1].name_en, "In Progress");
    }

    // ───────── init_guild_at — tempdir 기반 ─────────

    fn fresh_tmp(label: &str) -> std::path::PathBuf {
        let ns = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let p = std::env::temp_dir().join(format!("og-cli-{label}-{ns}"));
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    #[test]
    fn init_creates_guild_file_with_default_name() {
        let dir = fresh_tmp("default");
        // 디렉토리 이름이 default name 으로 쓰이도록 하위 디렉토리 사용
        let sub = dir.join("monitor");
        std::fs::create_dir_all(&sub).unwrap();

        let (path, name) = init_guild_at(&sub, None).unwrap();
        assert_eq!(name, "monitor");
        assert!(path.exists());
        assert_eq!(path.file_name().unwrap(), "monitor.guild");

        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("name = \"monitor\""));
        assert!(content.contains("version = \"1.0\""));
        assert!(content.contains("created_at = "));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn init_uses_name_arg_when_provided() {
        let dir = fresh_tmp("named");
        let (path, name) = init_guild_at(&dir, Some("커스텀이름".into())).unwrap();
        assert_eq!(name, "커스텀이름");
        assert_eq!(path.file_name().unwrap(), "커스텀이름.guild");
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("name = \"커스텀이름\""));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn init_preserves_existing_guild_name() {
        // 기존 마커 보존 + --name 인자 무시 (안내 stderr).
        let dir = fresh_tmp("preserve");
        std::fs::write(
            dir.join("existing.guild"),
            "name = \"X\"\nversion = \"1.0\"\ncreated_at = \"2026-01-01\"\n",
        )
        .unwrap();

        let (path, name) = init_guild_at(&dir, Some("new".into())).unwrap();
        assert_eq!(name, "X", "기존 이름 보존");
        assert_eq!(path.file_name().unwrap(), "existing.guild");
        // "new" 라는 새 파일 안 생김
        assert!(!dir.join("new.guild").exists());
        // .guild/ 시드가 추가됨 (idempotent upgrade)
        assert!(dir.join(".guild/types/DEV.toml").exists());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn init_idempotent_on_full_state() {
        let dir = fresh_tmp("idem");
        let first = init_guild_at(&dir, Some("alpha".into())).unwrap();
        let second = init_guild_at(&dir, None).unwrap();
        assert_eq!(first, second, "두 번째 호출도 같은 결과");
        // 한 마커만 존재
        let guild_files: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.path().is_file()
                    && e.file_name().to_string_lossy().ends_with(".guild")
            })
            .collect();
        assert_eq!(guild_files.len(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn init_creates_dot_guild_structure() {
        let dir = fresh_tmp("dotguild");
        init_guild_at(&dir, Some("test".into())).unwrap();

        assert!(dir.join("test.guild").is_file());
        assert!(dir.join(".guild").is_dir());
        assert!(dir.join(".guild/quests").is_dir());
        assert!(dir.join(".guild/types").is_dir());
        assert!(dir.join(".guild/statuses").is_dir());
        assert!(dir.join(".guild/backups").is_dir());
        assert!(dir.join(".guild/.gitignore").is_file());
        assert!(dir.join(".guild/types/DEV.toml").is_file());
        assert!(dir.join(".guild/statuses/1-open.toml").is_file());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn init_writes_parsable_toml() {
        let dir = fresh_tmp("toml");
        let (path, _) = init_guild_at(&dir, Some("길드 이름".into())).unwrap();
        let content = std::fs::read_to_string(&path).unwrap();
        // toml 파싱 가능 + 정확한 값
        let parsed: toml::Value = toml::from_str(&content).unwrap();
        assert_eq!(parsed["name"].as_str().unwrap(), "길드 이름");
        assert_eq!(parsed["version"].as_str().unwrap(), "1.0");
        // created_at 은 YYYY-MM-DD 형식
        let date = parsed["created_at"].as_str().unwrap();
        assert_eq!(date.len(), 10);
        assert_eq!(&date[4..5], "-");
        assert_eq!(&date[7..8], "-");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
