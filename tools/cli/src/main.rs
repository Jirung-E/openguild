//! OpenGuild CLI (`og`)
//!
//! 백엔드 HTTP API 의 agent / 사람용 콘솔 클라이언트.
//! frontend (Svelte) 와 같은 endpoint 를 호출. 서버는 별도로 띄워둬야 한다.
//!
//! 환경변수:
//!   OPENGUILD_URL   서버 base URL (기본: http://localhost:3000)
//!
//! 글로벌 옵션:
//!   --url            서버 URL (env 보다 우선)
//!   --json           JSON 출력 (agent 용)

use anyhow::{anyhow, Context, Result};
use clap::{Parser, Subcommand};
use serde::{Deserialize, Serialize};

const DEFAULT_URL: &str = "http://localhost:3000";

// ─────────────────────────── CLI 정의 ───────────────────────────

#[derive(Parser)]
#[command(
    name = "og",
    version,
    about = "OpenGuild CLI — HTTP client for the OpenGuild server"
)]
struct Cli {
    /// 서버 URL (env: OPENGUILD_URL, 기본: http://localhost:3000)
    #[arg(long, global = true)]
    url: Option<String>,

    /// JSON 출력 (agent 가 stdout 파싱용)
    #[arg(long, global = true)]
    json: bool,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
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
}

#[derive(Subcommand)]
enum QuestCmd {
    /// 전체 퀘스트 목록
    List,
    /// 퀘스트 상세 (슬러그: DEV-001)
    Show { slug: String },
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

// ─────────────────────────── DTO ───────────────────────────

#[derive(Serialize, Deserialize, Debug, Clone)]
struct Quest {
    id: i64,
    quest_id: String,
    quest_type_id: i64,
    type_prefix: String,
    type_color: String,
    number: i64,
    title: String,
    description: Option<String>,
    status_id: i64,
    status_name_en: String,
    status_name_ko: String,
    status_color: String,
    urgency: i64,
    parent_quest_id: Option<i64>,
    created_at: String,
    updated_at: String,
}

#[derive(Serialize, Deserialize, Debug)]
struct QuestDetail {
    #[serde(flatten)]
    quest: Quest,
    sub_quests: Vec<Quest>,
    prerequisites: Vec<Quest>,
}

#[derive(Serialize, Deserialize, Debug)]
struct QuestType {
    id: i64,
    prefix: String,
    color: String,
    description: Option<String>,
}

#[derive(Serialize, Deserialize, Debug)]
struct QuestStatus {
    id: i64,
    name_en: String,
    name_ko: String,
    color: String,
    sort_order: i64,
}

// ─────────────────────────── HTTP 클라이언트 ───────────────────────────

struct Client {
    base: String,
    http: reqwest::blocking::Client,
}

impl Client {
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
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(&body) {
                if let Some(err) = v.get("error").and_then(|e| e.as_str()) {
                    return Err(anyhow!("{}: {}", status, err));
                }
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
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(&body) {
                if let Some(err) = v.get("error").and_then(|e| e.as_str()) {
                    return Err(anyhow!("{}: {}", status, err));
                }
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

    fn list_quests(&self) -> Result<Vec<Quest>> {
        self.get("/api/quests")
    }

    fn list_deleted_quests(&self) -> Result<Vec<Quest>> {
        self.get("/api/deleted-quests")
    }

    fn quest_by_slug(&self, slug: &str) -> Result<QuestDetail> {
        self.get(&format!("/api/quests/by/{slug}"))
    }

    fn quest_types(&self) -> Result<Vec<QuestType>> {
        self.get("/api/quest-types")
    }

    fn quest_statuses(&self) -> Result<Vec<QuestStatus>> {
        self.get("/api/quest-statuses")
    }

    fn create_quest(&self, body: &serde_json::Value) -> Result<Quest> {
        self.post("/api/quests", body)
    }

    fn update_quest(&self, id: i64, body: &serde_json::Value) -> Result<Quest> {
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
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(&body) {
                if let Some(err) = v.get("error").and_then(|e| e.as_str()) {
                    return Err(anyhow!("{}: {}", status, err));
                }
            }
            return Err(anyhow!("{}: {}", status, body));
        }
        Ok(())
    }

    fn remove_prerequisite(&self, id: i64, prereq_id: i64) -> Result<()> {
        self.delete_no_body(&format!("/api/quests/{id}/prerequisites/{prereq_id}"))
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

/// 타입 prefix (대소문자 무시) → type_id.
fn match_type_id(prefix: &str, types: &[QuestType]) -> Option<i64> {
    let want = prefix.to_uppercase();
    types
        .iter()
        .find(|t| t.prefix.to_uppercase() == want)
        .map(|t| t.id)
}

// ─────────────────────────── 출력 ───────────────────────────

/// JSON 옵션이면 JSON, 아니면 사람용 포맷터로.
fn print_quest(q: &Quest, json: bool) {
    if json {
        println!("{}", serde_json::to_string_pretty(q).unwrap());
        return;
    }
    println!(
        "{:<10} [{}] {} (urgency {})",
        q.quest_id, q.status_name_en, q.title, q.urgency
    );
    if let Some(d) = &q.description {
        if !d.is_empty() {
            println!("           {}", d.lines().next().unwrap_or(""));
        }
    }
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
    if let Some(desc) = &q.description {
        if !desc.is_empty() {
            println!("  description:");
            for line in desc.lines() {
                println!("    {line}");
            }
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
    let url = cli
        .url
        .or_else(|| std::env::var("OPENGUILD_URL").ok())
        .unwrap_or_else(|| DEFAULT_URL.to_string());
    let c = Client::new(url);

    match cli.command {
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
        Command::Quest { sub } => match sub {
            QuestCmd::List => {
                let quests = c.list_quests()?;
                print_quest_list(&quests, cli.json);
            }
            QuestCmd::Show { slug } => {
                let d = c.quest_by_slug(&slug)?;
                print_quest_detail(&d, cli.json);
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
                let body = serde_json::json!({
                    "quest_type_id": type_id,
                    "title": title,
                    "description": description,
                    "status_id": open_status.id,
                    "urgency": urgency,
                    "parent_quest_id": parent_id,
                });
                let q = c.create_quest(&body)?;
                print_quest(&q, cli.json);
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

                let mut body = serde_json::Map::new();
                if let Some(t) = title {
                    body.insert("title".into(), serde_json::Value::String(t));
                }
                if let Some(d) = description {
                    body.insert("description".into(), serde_json::Value::String(d));
                }
                if let Some(u) = urgency {
                    body.insert("urgency".into(), serde_json::Value::from(u));
                }
                let q = c.update_quest(id, &serde_json::Value::Object(body))?;
                print_quest(&q, cli.json);
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
        let cli = Cli::try_parse_from(["og", "quest", "list"]).unwrap();
        assert!(matches!(
            cli.command,
            Command::Quest {
                sub: QuestCmd::List
            }
        ));
        assert!(!cli.json);
        assert!(cli.url.is_none());
    }

    #[test]
    fn cli_parse_global_json_and_url() {
        let cli = Cli::try_parse_from([
            "og",
            "--json",
            "--url",
            "http://example.com",
            "quest",
            "list",
        ])
        .unwrap();
        assert!(cli.json);
        assert_eq!(cli.url.as_deref(), Some("http://example.com"));
    }

    #[test]
    fn cli_parse_quest_new_minimal() {
        let cli =
            Cli::try_parse_from(["og", "quest", "new", "--type", "DEV", "--title", "test"])
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
            "og", "quest", "new",
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
            "og",
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
        let cli = Cli::try_parse_from(["og", "quest", "delete", "DEV-001", "--dry-run"]).unwrap();
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
        let cli = Cli::try_parse_from(["og", "quest", "delete", "DEV-001"]).unwrap();
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
            Cli::try_parse_from(["og", "quest", "parent", "DEV-001", "--detach"]).unwrap();
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
            "og", "quest", "prereq", "add", "DEV-001", "DEV-002",
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
}
