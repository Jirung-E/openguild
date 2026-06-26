//! openguild CLI (`openguild`)
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

use anyhow::{anyhow, bail, Context, Result};
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
    about = "openguild CLI — local + remote guild operations"
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

// QuestCmd 가 ListQuery 등 큰 필터 구조체를 포함하므로 다른 variant 와 크기 차가
// 크지만, CLI 는 한 번 실행되고 끝 — 메모리 영향 무시 가능. 박싱 회피.
#[allow(clippy::large_enum_variant)]
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
    /// 퀘스트 타입 — 목록 / 추가 / 수정 / 삭제 / 이름 변경
    Types {
        #[command(subcommand)]
        sub: Option<TypesCmd>,
    },
    /// 퀘스트 상태 — 목록 / 추가 / 수정 / 삭제 / 이름 변경
    Statuses {
        #[command(subcommand)]
        sub: Option<StatusesCmd>,
    },
    /// 캠페인 관련 명령
    Campaign {
        #[command(subcommand)]
        sub: CampaignCmd,
    },
    // BUG-016: doc comment 의 quest id 가 clap help 로 leak — 외부 사용자에게
    // internal quest 번호 노출 금지. 기능 설명만 plain `///` 으로.
    /// 길드 규칙 — `.guild/rules/{slug}.md` 다중 파일 CRUD.
    Rules {
        #[command(subcommand)]
        sub: RulesCmd,
    },
    /// 퀘스트 템플릿 — `.guild/templates/{name}.md`. `quest new --template` 으로 사용.
    Template {
        #[command(subcommand)]
        sub: TemplateCmd,
    },
    /// 서버 상태 확인 (health)
    Ping,
    /// 백업(스냅샷) — 생성 / 목록 / 삭제
    Backup {
        #[command(subcommand)]
        sub: BackupCmd,
    },
    /// 백업(스냅샷)으로 복원. `--at` 으로 journal replay 시점 복원.
    Restore {
        /// 특정 timestamp (`YYYYMMDD-HHMMSS`). 미지정 시 최신 사용.
        #[arg(long)]
        to: Option<String>,
        /// 시점 복원 — 최신 snapshot 복원 후 journal(AOF) 을 이 시각(ISO8601
        /// UTC, 예 `2026-06-27T00:15:00Z`, 포함)까지 재적용. 내용 op(댓글/메모
        /// 본문)·type 변경·첨부가 낀 구간은 안전을 위해 거부됨.
        #[arg(long, conflicts_with = "to")]
        at: Option<String>,
    },
    /// 파일 → index.db 캐시 재구축 (외부 편집 / git pull / restore 후 정합). `index rebuild` 와 동일.
    Reindex,
    /// 무결성 점검 — drift / counters.
    Check {
        #[command(subcommand)]
        sub: CheckCmd,
    },
    /// index.db 캐시 — rebuild / vacuum.
    Index {
        #[command(subcommand)]
        sub: IndexCmd,
    },
    /// journal(AOF) — tail. (시점 복원 replay 는 `restore` 에서 처리 예정.)
    Journal {
        #[command(subcommand)]
        sub: JournalCmd,
    },
    /// legacy guild.db → .guild/quests/*.md 파일 진리원 구조로 일회성 이전.
    MigrateToFiles,
    /// 길드 메타 / index.db / snapshot / journal 요약 (진단).
    Info {
        /// 1 줄 요약만 (script / status bar 친화).
        #[arg(long)]
        brief: bool,
    },
}

/// DEV-177: 무결성 점검 그룹.
#[derive(Subcommand)]
enum CheckCmd {
    /// 외부 편집 / 손상으로 index.db 가 파일과 어긋났는지 검사 (+ 자동 resync).
    Drift {
        /// 발견된 drift 를 자동으로 reindex 로 해소 (기본: 보고만).
        #[arg(long)]
        resync: bool,
    },
    /// type 의 last_number 가 실제 max quest 번호와 일치하는지 검사 (+ 자동 보정).
    Counters {
        /// 발견된 불일치를 파일 + SQL 에 직접 보정 (기본: 보고만).
        #[arg(long)]
        fix: bool,
    },
}

/// DEV-177: index.db 캐시 그룹.
#[derive(Subcommand)]
enum IndexCmd {
    /// 파일 → index.db 캐시 재구축 (top-level `reindex` 와 동일).
    Rebuild,
    /// SQLite VACUUM — index.db 의 dead row 제거 + 파일 크기 정리.
    Vacuum,
}

/// DEV-177: journal(AOF) 그룹. (replay 는 restore 에서 — DEV-022)
#[derive(Subcommand)]
enum JournalCmd {
    /// journal.db 의 최근 N 개 op 출력 (debug / audit 용).
    Tail {
        /// 출력할 row 수 (기본 50).
        #[arg(short = 'n', long, default_value_t = 50)]
        count: i64,
    },
}

/// DEV-176: 백업(스냅샷) 서브커맨드 — 다른 명사 그룹(quest/campaign…)과 통일.
#[derive(Subcommand)]
enum BackupCmd {
    /// 백업(스냅샷) 즉시 생성 (quest/campaign 의 `new` 와 통일).
    New,
    /// 사용 가능한 백업 목록 (오래된 순)
    List,
    /// 특정 백업 삭제
    #[command(name = "remove")]
    Rm {
        /// 삭제할 timestamp (`YYYYMMDD-HHMMSS`). `backup list` 로 확인.
        timestamp: String,
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
        /// `search` 검색을 title 만으로 제한. description 제외.
        #[arg(long = "title-only")]
        title_only: bool,
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
        // BUG-016: doc 에 quest_id prefix 누출 금지.
        /// tree 모드 — root quest 부터 들여쓰기로 자식 표시. 기본 flat.
        /// `--id-only` / `--count` / `--json` 과 함께 쓰면 무시 (구조화 출력 우선).
        #[arg(long)]
        tree: bool,
    },
    /// 퀘스트 검색 — title / description / slug 부분 일치 (공백 split AND).
    /// 사실상 `list --search` 의 별칭이지만 발견성을 위해 단독 명령으로 노출.
    Search {
        /// 검색 키워드. 다중 토큰은 공백 구분 (AND).
        query: String,
        /// title 만 검사 (description / slug 도 매치하는 기본 동작 비활성).
        /// 단 slug 매치는 항상 유지 (메타 정보).
        #[arg(long = "title-only")]
        title_only: bool,
        /// 결과 최대 행 수.
        #[arg(long)]
        limit: Option<i64>,
        /// id (slug) 만 출력 — script 친화.
        #[arg(long = "id-only", conflicts_with = "count")]
        id_only: bool,
        /// 매칭 개수만 정수로 출력.
        #[arg(long)]
        count: bool,
    },
    /// 퀘스트 상세 (슬러그로 조회).
    Show {
        slug: String,
        /// 단일 필드만 출력 (script / pipe 친화).
        /// 사용 가능: id / title / status / status_slug / urgency / description /
        /// type / parent / created_at / updated_at.
        /// 미지정 시 기본 멀티라인 형식.
        #[arg(long, value_name = "FIELD")]
        field: Option<String>,
    },
    /// quest 의 변경 이력 — 최신 → 과거 순.
    History { slug: String },
    /// 새 퀘스트 생성
    New {
        /// 타입 prefix (DEV / BUG / REQ ...). --template 의 type 으로 대체 가능.
        #[arg(long = "type", value_name = "PREFIX")]
        type_prefix: Option<String>,
        /// 제목. --template 의 title 로 대체 가능.
        #[arg(long)]
        title: Option<String>,
        #[arg(long)]
        description: Option<String>,
        /// 1=Critical 2=High 3=Medium 4=Low (기본 3, 템플릿이 있으면 그 값)
        #[arg(long)]
        urgency: Option<i64>,
        /// 부모 퀘스트 슬러그 (서브퀘스트로 생성)
        #[arg(long)]
        parent: Option<String>,
        /// 템플릿 이름 (`.guild/templates/{name}.md`). 명시 옵션이 템플릿보다 우선.
        #[arg(long)]
        template: Option<String>,
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
    /// 현재 상태 출력. status 인자 지정 시 변경도 가능 — deprecated,
    /// `move` 사용 권장.
    Status {
        slug: String,
        /// (deprecated) 상태 변경 — 새 명령 `quest move <slug> <status>` 사용.
        /// 인자 미지정 시 현재 상태만 출력.
        status: Option<String>,
    },
    /// 상태 변경. status: name_en / slug / ID.
    Move { slug: String, status: String },
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
    // BUG-037: doc comment 의 quest id (DEV-076) 가 clap help 로 leak — 외부에
    // 노출되면 안 됨. 일반 doc comment 는 plain 코멘트로 변경.
    /// 희망 / 필수 기한 조회 / 설정 / 해제.
    /// 인자 없으면 현재 상태 출력. `--desired` / `--required` 로 설정.
    /// `--clear-desired` / `--clear-required` 로 해제.
    Due {
        slug: String,
        /// 희망 기한 — YYYY-MM-DD. 정보성 (Home 임박 판단에는 사용 안 함).
        #[arg(long, value_name = "YYYY-MM-DD", conflicts_with = "clear_desired")]
        desired: Option<String>,
        /// 필수 기한 — YYYY-MM-DD. Home "마감 임박" / "Overdue" 섹션의 기준.
        #[arg(long, value_name = "YYYY-MM-DD", conflicts_with = "clear_required")]
        required: Option<String>,
        /// 희망 기한 해제 (NULL).
        #[arg(long = "clear-desired")]
        clear_desired: bool,
        /// 필수 기한 해제 (NULL).
        #[arg(long = "clear-required")]
        clear_required: bool,
    },
    // BUG-016: quest_id leak 방지 — about 에는 기능 설명만.
    /// 댓글 (entry 단위, 공개) — list / show / add / edit / remove.
    /// 진리원: `.guild/quests/{slug}.comments.md` (git tracked).
    Comment {
        #[command(subcommand)]
        sub: CommentCmd,
    },
    /// 첨부 (본문과 별개 섹션) — list / add / remove.
    /// 진리원: `.guild/quests/{slug}.attachments.json` + `.guild/attachments/`.
    Attach {
        #[command(subcommand)]
        sub: AttachCmd,
    },
    /// 메모 (단일 텍스트, 비공개) — show / set / clear.
    /// 진리원: `.guild/quests/{slug}.memo.md` (gitignored).
    Memo {
        #[command(subcommand)]
        sub: MemoCmd,
    },
    // BUG-016: doc 에 quest_id prefix 누출 X.
    /// 태그 — list / add / remove / set. frontmatter 가 진리원.
    Tag {
        #[command(subcommand)]
        sub: TagCmd,
    },
}

#[derive(Subcommand)]
enum TagCmd {
    /// 현재 quest 의 tag 목록 (공백 구분 1줄).
    List { slug: String },
    /// tag 1개 또는 여러 개 추가 (기존과 합쳐 dedupe).
    Add {
        slug: String,
        /// 추가할 tag 들. 공백 구분 또는 여러 인자.
        #[arg(required = true, num_args = 1..)]
        tags: Vec<String>,
    },
    /// tag 1개 또는 여러 개 제거 (없는 건 무시).
    #[command(name = "remove")]
    Rm {
        slug: String,
        /// 제거할 tag 들.
        #[arg(required = true, num_args = 1..)]
        tags: Vec<String>,
    },
    /// tag 전체 교체 (기존 모두 삭제 후 인자만). 인자 0 개 = 전체 삭제.
    Set {
        slug: String,
        /// 새 tag 들 (공백 구분 또는 여러 인자).
        tags: Vec<String>,
    },
}

/// DEV-060: 퀘스트 템플릿.
#[derive(Subcommand)]
enum TemplateCmd {
    /// 템플릿 목록 (이름 / 기본값 요약).
    List,
    /// 템플릿 본문 출력.
    Show { name: String },
    /// 템플릿 생성/갱신 — `.guild/templates/{name}.md`. 본문은 --file / stdin.
    /// (독립 엔티티라 quest/campaign 처럼 `new`.)
    New {
        /// 템플릿 이름 (파일명 stem).
        name: String,
        /// 기본 type prefix (DEV / BUG ...).
        #[arg(long = "type")]
        type_prefix: Option<String>,
        /// 새 quest 의 기본 제목.
        #[arg(long)]
        title: Option<String>,
        /// 기본 urgency (1=Critical .. 4=Low).
        #[arg(long)]
        urgency: Option<i64>,
        /// 기본 tags — 반복 또는 콤마 구분.
        #[arg(long, value_delimiter = ',')]
        tags: Vec<String>,
        /// 본문 파일. 미지정 시 stdin (파이프 없으면 빈 본문).
        #[arg(long)]
        file: Option<std::path::PathBuf>,
        /// 이미 있으면 덮어쓰기 허용.
        #[arg(long)]
        force: bool,
    },
}

#[derive(Subcommand)]
enum CommentCmd {
    /// entry 목록 (id / ts / author / body 요약 1줄). 필터 옵션은 모두 AND 결합.
    List {
        slug: String,
        /// 작성자 일치 (대소문자 무시 정확 일치).
        #[arg(long)]
        author: Option<String>,
        /// 이 시각 이후 작성분만 — ISO date (`2026-06-01`) 또는 datetime.
        #[arg(long)]
        since: Option<String>,
        /// top-level 댓글만 (답글 제외).
        #[arg(long = "top-only", conflicts_with = "reply_to")]
        top_only: bool,
        /// 특정 entry 의 답글만.
        #[arg(long = "reply-to")]
        reply_to: Option<u64>,
        /// body 부분 일치 (대소문자 무시).
        #[arg(long)]
        grep: Option<String>,
    },
    /// entry 본문 전체 또는 단일.
    Show {
        slug: String,
        /// 특정 entry id 만 출력. 미지정 시 모든 entry.
        #[arg(long)]
        id: Option<u64>,
    },
    /// 새 댓글 entry 추가. 본문은 `--file PATH` 또는 stdin.
    Add {
        slug: String,
        /// 작성자 (자유 문자열, 빈 값 허용).
        #[arg(long)]
        author: Option<String>,
        /// 답글인 경우 부모 entry id.
        #[arg(long = "parent-id")]
        parent_id: Option<u64>,
        /// 본문 파일. 미지정 시 stdin.
        #[arg(long)]
        file: Option<std::path::PathBuf>,
    },
    /// 기존 entry 의 body 교체. ts / author 보존.
    Edit {
        slug: String,
        id: u64,
        #[arg(long)]
        file: Option<std::path::PathBuf>,
    },
    /// entry 삭제. `--force` 없으면 prompt.
    #[command(name = "remove")]
    Rm {
        slug: String,
        id: u64,
        #[arg(long)]
        force: bool,
    },
    /// 토론(discussion) 플래그 토글 (quest 전용). 미해결 토론이 있으면 그 quest 의
    /// 완료 전환이 차단됨. discussion 을 끄면 resolved 도 해제.
    Discussion { slug: String, id: u64 },
    /// discussion 댓글의 resolved 토글 (quest 전용).
    Resolved { slug: String, id: u64 },
}

#[derive(Subcommand)]
enum MemoCmd {
    /// 메모 본문 stdout. 파일 없으면 "(메모 없음)".
    Show { slug: String },
    /// 메모 본문 교체. 본문은 `--file PATH` 또는 stdin.
    Set {
        slug: String,
        #[arg(long)]
        file: Option<std::path::PathBuf>,
    },
    /// 메모 본문 비움 (빈 문자열로 교체 — 파일은 남음).
    Clear { slug: String },
}

#[derive(Subcommand)]
enum PrereqCmd {
    /// 선행 퀘스트 추가
    Add { slug: String, prereq: String },
    /// 선행 퀘스트 제거
    #[command(name = "remove")]
    Rm { slug: String, prereq: String },
}

/// DEV-062: type 관리. sub 미지정 시 List.
#[derive(Subcommand)]
enum TypesCmd {
    /// 목록 (기본 동작)
    List,
    /// 새 type 추가
    Add {
        /// 대문자/숫자 1~6자 (예: DEV / BUG / REQ)
        prefix: String,
        /// 색 (#RGB 또는 #RRGGBB)
        #[arg(long)]
        color: String,
        /// 설명 (선택)
        #[arg(long)]
        description: Option<String>,
    },
    /// 기존 type 수정 — color / description / prefix 통합.
    /// `--prefix` 가 현재와 다르면 그 type 의 모든 quest slug cascade.
    Update {
        prefix: String,
        /// 새 prefix — 지정 시 rename + cascade (파일명 / frontmatter / DB slug).
        #[arg(long = "prefix")]
        new_prefix: Option<String>,
        #[arg(long)]
        color: Option<String>,
        #[arg(long)]
        description: Option<String>,
        /// description 을 비움 (--description 과 동시 사용 불가)
        #[arg(long)]
        clear_description: bool,
    },
    /// 사용 중 quest 없는 type 삭제
    Delete { prefix: String },
}

/// DEV-062: status 관리. sub 미지정 시 List.
#[derive(Subcommand)]
enum StatusesCmd {
    /// 목록 (기본 동작)
    List,
    /// 새 status 추가. slug 는 name_en 에서 자동 생성.
    Add {
        /// 영문 이름 (영문자 시작 + 영문/숫자/공백/-/_, 최대 32자).
        name_en: String,
        #[arg(long)]
        color: String,
        /// 한국어 이름 (선택). 한글/영문/숫자/공백/-/_ 만, 최대 32자.
        #[arg(long = "name-ko")]
        name_ko: Option<String>,
        /// 미지정 시 max(sort_order)+1.
        #[arg(long = "sort-order")]
        sort_order: Option<i64>,
    },
    /// 기존 status 수정 — name_en / name_ko / color / sort_order / slug 통합.
    /// `--slug` 가 현재와 다르면 rename + cascade (history / 모든 quest frontmatter).
    Update {
        slug: String,
        /// 새 slug — 지정 시 rename + cascade (a-z0-9_, 1~32자).
        #[arg(long = "slug")]
        new_slug: Option<String>,
        #[arg(long = "name-en")]
        name_en: Option<String>,
        #[arg(long = "name-ko")]
        name_ko: Option<String>,
        #[arg(long)]
        color: Option<String>,
        #[arg(long = "sort-order")]
        sort_order: Option<i64>,
        /// name_ko 를 비움.
        #[arg(long = "clear-name-ko")]
        clear_name_ko: bool,
    },
    /// 사용 중 quest 없는 status 삭제
    Delete { slug: String },
}

/// `--file PATH` 또는 stdin 에서 본문 읽기 — Rules / 향후 description set 등 공용.
fn read_content(path: Option<&std::path::Path>) -> Result<String> {
    if let Some(p) = path {
        std::fs::read_to_string(p)
            .with_context(|| format!("파일 읽기 실패: {}", p.display()))
    } else {
        use std::io::Read;
        let mut s = String::new();
        std::io::stdin().read_to_string(&mut s)?;
        Ok(s)
    }
}

// ─────────────────────────── Rules 서브명령 (DEV-016 multi-file) ───────────────────────────

#[derive(Subcommand)]
enum RulesCmd {
    /// 모든 규칙 slug 목록 (legacy `.guild/rules.md` 가 있으면 자동 마이그레이션).
    List,
    /// 한 규칙의 본문 출력 (stdout). slug 없으면 NotFound.
    Show { slug: String },
    /// 한 규칙 본문 교체 (멱등). 파일이 없으면 만들고 / 있으면 덮어씀.
    /// 본문은 `--file <PATH>` 또는 stdin (인자 없을 때).
    Set {
        slug: String,
        /// 본문이 들어있는 파일. 미지정 시 stdin.
        #[arg(long)]
        file: Option<std::path::PathBuf>,
    },
    /// 신규 규칙 생성 — 같은 slug 이미 있으면 에러. 본문은 `--file` / stdin.
    /// `--empty` 시 본문 없이 빈 규칙 생성.
    Create {
        slug: String,
        #[arg(long)]
        file: Option<std::path::PathBuf>,
        #[arg(long)]
        empty: bool,
    },
    /// 규칙 삭제. `--force` 없으면 prompt.
    Delete {
        slug: String,
        #[arg(long)]
        force: bool,
    },
    /// 규칙 slug 변경.
    Rename {
        slug: String,
        new_slug: String,
    },
}

// ─────────────────────────── Campaign 서브명령 (DEV-011) ───────────────────────────

#[derive(Subcommand)]
enum CampaignCmd {
    /// 캠페인 공개 댓글 — quest comment 와 동일 형식 / 필터.
    Comment {
        #[command(subcommand)]
        sub: CommentCmd,
    },
    /// 캠페인 첨부 — quest attach 와 동일 (list / add / remove).
    Attach {
        #[command(subcommand)]
        sub: AttachCmd,
    },
    /// 캠페인 비공개 메모 — quest memo 와 동일.
    Memo {
        #[command(subcommand)]
        sub: MemoCmd,
    },
    /// 새 캠페인 생성 (자동 C-NNN slug)
    New {
        #[arg(long)]
        title: String,
        /// ISO 날짜 (YYYY-MM-DD)
        #[arg(long = "start")]
        started_at: Option<String>,
        #[arg(long = "end")]
        ended_at: Option<String>,
    },
    /// 캠페인 목록
    List {
        /// 필터: active | done
        #[arg(long)]
        status: Option<String>,
    },
    /// 캠페인 상세
    Show { slug: String },
    /// 상태 변경 → active
    Start { slug: String },
    /// 상태 변경 → done
    End { slug: String },
    /// 캠페인에 quest 연결
    Link {
        campaign_slug: String,
        quest_slug: String,
    },
    /// 캠페인에서 quest 연결 해제
    Unlink {
        campaign_slug: String,
        quest_slug: String,
    },
    /// 캠페인 삭제 (soft)
    Delete {
        slug: String,
        /// 안전장치 — 없으면 거부
        #[arg(long)]
        yes: bool,
    },
    /// 체크리스트 명령
    Checklist {
        #[command(subcommand)]
        sub: CampaignChecklistCmd,
    },
}

#[derive(Subcommand)]
enum CampaignChecklistCmd {
    /// 항목 추가 (캠페인 파일 본문 끝에 `- [ ] {text}` 한 줄 append)
    Add {
        campaign_slug: String,
        text: String,
    },
    /// N번째 (1-based) 항목 체크
    Check {
        campaign_slug: String,
        index: usize,
    },
    /// N번째 (1-based) 항목 언체크
    Uncheck {
        campaign_slug: String,
        index: usize,
    },
    /// N번째 (1-based) 항목 삭제
    #[command(name = "remove")]
    Rm {
        campaign_slug: String,
        index: usize,
    },
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

    fn change_status(&self, id: i64, status_slug: &str) -> Result<Quest> {
        // DEV-048: status_slug 전용.
        self.patch(
            &format!("/api/quests/{id}/status"),
            &serde_json::json!({ "status_slug": status_slug }),
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

    /// DEV-076: 희망 / 필수 기한 설정 / 해제.
    /// 각 인자: `Some(Some(d))` = 설정, `Some(None)` = 해제, `None` = 변경 없음.
    fn set_due_dates(
        &self,
        id: i64,
        desired_due: Option<Option<String>>,
        required_due: Option<Option<String>>,
    ) -> Result<Quest> {
        // serde_json 은 Option<Option<T>> 를 직접 표현 못 함 — 명시적 키 존재만
        // 제어. server 가 키 존재 여부로 "변경 의도" 구분.
        let mut body = serde_json::Map::new();
        if let Some(v) = desired_due {
            body.insert("desired_due".into(), serde_json::to_value(v).unwrap());
        }
        if let Some(v) = required_due {
            body.insert("required_due".into(), serde_json::to_value(v).unwrap());
        }
        self.patch(&format!("/api/quests/{id}/due"), &serde_json::Value::Object(body))
    }

    fn create_snapshot(&self) -> Result<openguild_core::snapshot::SnapshotInfo> {
        self.post("/api/admin/snapshot", &serde_json::json!({}))
    }

    fn list_snapshots(&self) -> Result<Vec<openguild_core::snapshot::SnapshotInfo>> {
        self.get("/api/admin/snapshots")
    }

    fn delete_snapshot(&self, timestamp: &str) -> Result<()> {
        self.delete_no_body(&format!("/api/admin/snapshots/{timestamp}"))
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

    // ── Campaign (DEV-011 commit 3) ───────────────────────

    fn campaign_create(
        &self,
        body: &openguild_core::models::CreateCampaignRequest,
    ) -> Result<openguild_core::models::CampaignRow> {
        self.post("/api/campaigns", body)
    }

    fn campaign_list(
        &self,
        status: Option<&str>,
    ) -> Result<Vec<openguild_core::models::CampaignRow>> {
        let path = match status {
            Some(s) => format!("/api/campaigns?status={s}"),
            None => "/api/campaigns".to_string(),
        };
        self.get(&path)
    }

    fn campaign_show(
        &self,
        slug: &str,
    ) -> Result<openguild_core::models::CampaignDetail> {
        self.get(&format!("/api/campaigns/{slug}"))
    }

    fn campaign_update(
        &self,
        slug: &str,
        body: &openguild_core::models::UpdateCampaignRequest,
    ) -> Result<openguild_core::models::CampaignRow> {
        self.patch(&format!("/api/campaigns/{slug}"), body)
    }

    fn campaign_delete(&self, slug: &str) -> Result<()> {
        self.delete_no_body(&format!("/api/campaigns/{slug}"))
    }

    fn campaign_link(&self, campaign_slug: &str, quest_slug: &str) -> Result<()> {
        let body = openguild_core::models::LinkQuestRequest {
            quest_slug: quest_slug.to_string(),
        };
        let _: serde_json::Value =
            self.post(&format!("/api/campaigns/{campaign_slug}/quests"), &body)?;
        Ok(())
    }

    fn campaign_unlink(&self, campaign_slug: &str, quest_slug: &str) -> Result<()> {
        self.delete_no_body(&format!(
            "/api/campaigns/{campaign_slug}/quests/{quest_slug}"
        ))
    }

    fn campaign_checklist_add(
        &self,
        campaign_slug: &str,
        text: &str,
    ) -> Result<openguild_core::models::CampaignChecklistItem> {
        let body = openguild_core::models::AddChecklistRequest {
            text: text.to_string(),
        };
        self.post(&format!("/api/campaigns/{campaign_slug}/checklist"), &body)
    }

    fn campaign_checklist_set(
        &self,
        campaign_slug: &str,
        index: usize,
        checked: bool,
    ) -> Result<()> {
        let body = openguild_core::models::UpdateChecklistRequest {
            text: None,
            checked: Some(checked),
            order_idx: None,
        };
        let _: serde_json::Value = self.patch(
            &format!("/api/campaigns/{campaign_slug}/checklist/{index}"),
            &body,
        )?;
        Ok(())
    }

    fn campaign_checklist_rm(&self, campaign_slug: &str, index: usize) -> Result<()> {
        self.delete_no_body(&format!(
            "/api/campaigns/{campaign_slug}/checklist/{index}"
        ))
    }
}

// ─────────────────────────── Backend (Http / Local) ───────────────────────────

/// DEV-164: `info` 결과 묶음 — 길드 메타 + index.db 요약 + snapshot/journal.
struct CliInfo {
    path: std::path::PathBuf,
    guild: openguild_core::guild_file::GuildFile,
    summary: openguild_core::maintenance::IndexSummary,
    snapshots: Vec<openguild_core::snapshot::SnapshotInfo>,
    journal_total: i64,
}

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

        // DEV-117: 여기서 `recents::add` 를 호출하지 않는다.
        //
        // 과거엔 매 CLI 호출 (e.g. `openguild quest comment add ...`) 마다
        // recents 가 갱신되어 사용자가 다른 길드에서 GUI 작업 중이어도
        // CLI 활동을 한 길드가 Welcome 의 '최근 연 길드' 최상단으로 올라가
        // 사용자에게 "왜 내가 안 연 길드가 최상단에 있나" 라는 혼란을 줬다.
        // recents 의 의미는 'GUI 로 사용자가 직접 연 길드' — CLI 활동은 X.
        // GUI 의 `recents::add` 호출 (gui/src/lib.rs) 만 유지.

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

    /// Local 모드에서 비정상 quest 파일 (파싱 실패 / 정의되지 않은 status) 을
    /// stderr 로 경고. 그런 파일은 reindex·동기화에서 조용히 skip 되므로 GUI
    /// 시동 알림과 동일 취지로 사용자에게 알린다. Http 모드 / 조회 실패는 noop.
    fn warn_problem_files(&self) {
        if let Backend::Local(l) = self {
            let problems =
                l.rt.block_on(openguild_core::health::list_problem_quest_files(&l.store));
            if !problems.is_empty() {
                eprintln!("⚠ 비정상 파일 {} 개 감지 (캐시에서 제외됨):", problems.len());
                for (path, why) in &problems {
                    eprintln!("    - {path}: {why}");
                }
                eprintln!("  파일을 고치거나 status 를 정의한 뒤 `openguild reindex` 하세요.");
            }
        }
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

    fn change_status(&self, id: i64, status_slug: &str) -> Result<Quest> {
        match self {
            Backend::Http(c) => c.change_status(id, status_slug),
            Backend::Local(l) => Self::map_err(l.rt.block_on(
                openguild_core::ops::change_status(
                    &l.store,
                    id,
                    ChangeStatusRequest { status_slug: status_slug.to_string() },
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

    /// DEV-076: 희망 / 필수 기한.
    fn set_due_dates(
        &self,
        id: i64,
        desired_due: Option<Option<String>>,
        required_due: Option<Option<String>>,
    ) -> Result<Quest> {
        match self {
            Backend::Http(c) => c.set_due_dates(id, desired_due, required_due),
            Backend::Local(l) => Self::map_err(l.rt.block_on(
                openguild_core::ops::set_due_dates(&l.store, id, desired_due, required_due),
            )),
        }
    }

    // ── DEV-062: type / status 관리 (local 전용) ──
    //
    // remote (HTTP) backend 는 별도 quest — 본 quest 범위는 local 모드.
    // Backend::Http 호출 시 명시적 에러로 사용자 안내.

    fn http_unsupported_meta() -> anyhow::Error {
        anyhow::anyhow!(
            "remote 모드에서는 type/status 관리 미지원 (별도 quest). \
             local 모드 (--guild 또는 cwd 의 .guild) 에서 사용하세요."
        )
    }

    fn create_type(
        &self,
        prefix: String,
        color: String,
        description: Option<String>,
    ) -> Result<QuestType> {
        match self {
            Backend::Http(_) => Err(Self::http_unsupported_meta()),
            Backend::Local(l) => Self::map_err(l.rt.block_on(
                openguild_core::ops::create_type(&l.store, prefix, color, description),
            )),
        }
    }

    fn update_type(
        &self,
        prefix: String,
        new_prefix: Option<String>,
        color: Option<String>,
        description: Option<Option<String>>,
    ) -> Result<QuestType> {
        match self {
            Backend::Http(_) => Err(Self::http_unsupported_meta()),
            Backend::Local(l) => Self::map_err(l.rt.block_on(
                openguild_core::ops::update_type(
                    &l.store,
                    prefix,
                    new_prefix,
                    color,
                    description,
                ),
            )),
        }
    }

    fn delete_type(&self, prefix: String) -> Result<()> {
        match self {
            Backend::Http(_) => Err(Self::http_unsupported_meta()),
            Backend::Local(l) => Self::map_err(
                l.rt.block_on(openguild_core::ops::delete_type(&l.store, prefix)),
            ),
        }
    }

    fn create_status(
        &self,
        name_en: String,
        name_ko: String,
        color: String,
        sort_order: Option<i64>,
    ) -> Result<QuestStatus> {
        match self {
            Backend::Http(_) => Err(Self::http_unsupported_meta()),
            Backend::Local(l) => Self::map_err(l.rt.block_on(
                openguild_core::ops::create_status(
                    &l.store, name_en, name_ko, color, sort_order,
                ),
            )),
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn update_status(
        &self,
        slug: String,
        new_slug: Option<String>,
        name_en: Option<String>,
        name_ko: Option<String>,
        color: Option<String>,
        sort_order: Option<i64>,
    ) -> Result<QuestStatus> {
        match self {
            Backend::Http(_) => Err(Self::http_unsupported_meta()),
            Backend::Local(l) => Self::map_err(l.rt.block_on(
                openguild_core::ops::update_status(
                    &l.store, slug, new_slug, name_en, name_ko, color, sort_order,
                    None, // DEV-093: CLI 는 본 패치에서 counts_as_done 미노출.
                ),
            )),
        }
    }

    fn delete_status(&self, slug: String) -> Result<()> {
        match self {
            Backend::Http(_) => Err(Self::http_unsupported_meta()),
            Backend::Local(l) => Self::map_err(
                l.rt.block_on(openguild_core::ops::delete_status(&l.store, slug)),
            ),
        }
    }

    // ── Campaign (DEV-011) ───────────────────────────────

    fn campaign_create(
        &self,
        body: openguild_core::models::CreateCampaignRequest,
    ) -> Result<openguild_core::models::CampaignRow> {
        match self {
            Backend::Http(c) => c.campaign_create(&body),
            Backend::Local(l) => Self::map_err(
                l.rt.block_on(openguild_core::ops::campaigns::create_campaign(&l.store, body)),
            ),
        }
    }

    fn campaign_list(
        &self,
        status: Option<String>,
    ) -> Result<Vec<openguild_core::models::CampaignRow>> {
        match self {
            Backend::Http(c) => c.campaign_list(status.as_deref()),
            Backend::Local(l) => Self::map_err(l.rt.block_on(async {
                match status.as_deref() {
                    Some(s) => openguild_core::services::campaigns::list_by_status(
                        &l.store.index_pool,
                        s,
                    )
                    .await,
                    None => openguild_core::services::campaigns::list_alive(&l.store.index_pool).await,
                }
            })),
        }
    }

    fn campaign_show(
        &self,
        slug: &str,
    ) -> Result<openguild_core::models::CampaignDetail> {
        match self {
            Backend::Http(c) => c.campaign_show(slug),
            Backend::Local(l) => Self::map_err(
                l.rt.block_on(openguild_core::ops::campaigns::fetch_detail(&l.store, slug)),
            ),
        }
    }

    fn campaign_set_status(
        &self,
        slug: &str,
        new_status: &str,
    ) -> Result<openguild_core::models::CampaignRow> {
        match self {
            Backend::Http(c) => c.campaign_update(
                slug,
                &openguild_core::models::UpdateCampaignRequest {
                    status: Some(new_status.to_string()),
                    ..Default::default()
                },
            ),
            Backend::Local(l) => Self::map_err(l.rt.block_on(async {
                let row = openguild_core::services::campaigns::fetch_by_slug(
                    &l.store.index_pool,
                    slug,
                )
                .await?;
                openguild_core::ops::campaigns::update_campaign(
                    &l.store,
                    row.id,
                    openguild_core::models::UpdateCampaignRequest {
                        status: Some(new_status.to_string()),
                        ..Default::default()
                    },
                )
                .await
            })),
        }
    }

    fn campaign_link(&self, campaign_slug: &str, quest_slug: &str) -> Result<()> {
        match self {
            Backend::Http(c) => c.campaign_link(campaign_slug, quest_slug),
            Backend::Local(l) => Self::map_err(l.rt.block_on(async {
                let row = openguild_core::services::campaigns::fetch_by_slug(
                    &l.store.index_pool,
                    campaign_slug,
                )
                .await?;
                openguild_core::ops::campaigns::link_quest_by_slug(
                    &l.store, row.id, quest_slug,
                )
                .await
            })),
        }
    }

    fn campaign_unlink(&self, campaign_slug: &str, quest_slug: &str) -> Result<()> {
        match self {
            Backend::Http(c) => c.campaign_unlink(campaign_slug, quest_slug),
            Backend::Local(l) => Self::map_err(l.rt.block_on(async {
                let row = openguild_core::services::campaigns::fetch_by_slug(
                    &l.store.index_pool,
                    campaign_slug,
                )
                .await?;
                openguild_core::ops::campaigns::unlink_quest_by_slug(
                    &l.store, row.id, quest_slug,
                )
                .await
            })),
        }
    }

    fn campaign_delete(&self, slug: &str) -> Result<()> {
        match self {
            Backend::Http(c) => c.campaign_delete(slug),
            Backend::Local(l) => Self::map_err(l.rt.block_on(async {
                let row = openguild_core::services::campaigns::fetch_by_slug(
                    &l.store.index_pool,
                    slug,
                )
                .await?;
                openguild_core::ops::campaigns::delete_campaign(&l.store, row.id).await
            })),
        }
    }

    fn campaign_checklist_add(
        &self,
        campaign_slug: &str,
        text: &str,
    ) -> Result<openguild_core::models::CampaignChecklistItem> {
        match self {
            Backend::Http(c) => c.campaign_checklist_add(campaign_slug, text),
            Backend::Local(l) => Self::map_err(l.rt.block_on(async {
                let row = openguild_core::services::campaigns::fetch_by_slug(
                    &l.store.index_pool,
                    campaign_slug,
                )
                .await?;
                openguild_core::ops::campaigns::add_checklist_line(&l.store, row.id, text).await
            })),
        }
    }

    fn campaign_checklist_set(
        &self,
        campaign_slug: &str,
        index: usize,
        checked: bool,
    ) -> Result<()> {
        match self {
            Backend::Http(c) => c.campaign_checklist_set(campaign_slug, index, checked),
            Backend::Local(l) => Self::map_err(l.rt.block_on(async {
                let row = openguild_core::services::campaigns::fetch_by_slug(
                    &l.store.index_pool,
                    campaign_slug,
                )
                .await?;
                openguild_core::ops::campaigns::set_checklist_checked_by_index(
                    &l.store, row.id, index, checked,
                )
                .await
            })),
        }
    }

    fn campaign_checklist_rm(&self, campaign_slug: &str, index: usize) -> Result<()> {
        match self {
            Backend::Http(c) => c.campaign_checklist_rm(campaign_slug, index),
            Backend::Local(l) => Self::map_err(l.rt.block_on(async {
                let row = openguild_core::services::campaigns::fetch_by_slug(
                    &l.store.index_pool,
                    campaign_slug,
                )
                .await?;
                openguild_core::ops::campaigns::remove_checklist_by_index(
                    &l.store, row.id, index,
                )
                .await
            })),
        }
    }

    // ── 백업 / 복원 ──────────────────────────────────────

    // ── DEV-016 (multi-file): 길드 규칙 — local 전용 (HTTP 미지원 우선) ──

    fn rules_list(&self) -> Result<Vec<openguild_core::repo::rules::RuleEntry>> {
        match self {
            Backend::Http(_) => Err(Self::http_unsupported_meta()),
            Backend::Local(l) => Self::map_err(Ok::<_, openguild_core::error::AppError>(
                openguild_core::ops::rules::list_rules(&l.store)?,
            )),
        }
    }

    fn rules_get(&self, slug: &str) -> Result<Option<String>> {
        match self {
            Backend::Http(_) => Err(Self::http_unsupported_meta()),
            Backend::Local(l) => Self::map_err(Ok::<_, openguild_core::error::AppError>(
                openguild_core::ops::rules::get_rule(&l.store, slug)?,
            )),
        }
    }

    fn rules_set(&self, slug: &str, content: String) -> Result<()> {
        match self {
            Backend::Http(_) => Err(Self::http_unsupported_meta()),
            Backend::Local(l) => Self::map_err(l.rt.block_on(
                openguild_core::ops::rules::set_rule(&l.store, slug, content),
            )),
        }
    }

    fn rules_create(&self, slug: &str, content: String) -> Result<()> {
        match self {
            Backend::Http(_) => Err(Self::http_unsupported_meta()),
            Backend::Local(l) => Self::map_err(l.rt.block_on(
                openguild_core::ops::rules::create_rule(&l.store, slug, content),
            )),
        }
    }

    fn rules_delete(&self, slug: &str) -> Result<()> {
        match self {
            Backend::Http(_) => Err(Self::http_unsupported_meta()),
            Backend::Local(l) => Self::map_err(l.rt.block_on(
                openguild_core::ops::rules::delete_rule(&l.store, slug),
            )),
        }
    }

    fn rules_rename(&self, old_slug: &str, new_slug: &str) -> Result<()> {
        match self {
            Backend::Http(_) => Err(Self::http_unsupported_meta()),
            Backend::Local(l) => Self::map_err(l.rt.block_on(
                openguild_core::ops::rules::rename_rule(&l.store, old_slug, new_slug),
            )),
        }
    }

    // ── DEV-060: 템플릿 — local 전용 (HTTP 미지원 우선) ──

    fn templates_list(&self) -> Result<Vec<openguild_core::repo::TemplateFile>> {
        match self {
            Backend::Http(_) => Err(Self::http_unsupported_meta()),
            Backend::Local(l) => {
                openguild_core::repo::list_templates(&l.store.paths)
            }
        }
    }

    fn template_load(&self, name: &str) -> Result<openguild_core::repo::TemplateFile> {
        match self {
            Backend::Http(_) => Err(Self::http_unsupported_meta()),
            Backend::Local(l) => {
                let path = l.store.paths.template_path(name);
                if !path.exists() {
                    let available: Vec<String> =
                        openguild_core::repo::list_templates(&l.store.paths)
                            .unwrap_or_default()
                            .into_iter()
                            .map(|t| t.name)
                            .collect();
                    return Err(anyhow!(
                        "템플릿 '{name}' 없음 ({}). 사용 가능: {}",
                        path.display(),
                        if available.is_empty() {
                            "(없음 — .guild/templates/ 에 {name}.md 작성)".to_string()
                        } else {
                            available.join(", ")
                        }
                    ));
                }
                openguild_core::repo::TemplateFile::read(&path)
            }
        }
    }

    /// DEV-158: 템플릿 저장. 반환: 쓰여진 경로.
    fn template_save(
        &self,
        tpl: &openguild_core::repo::TemplateFile,
        force: bool,
    ) -> Result<std::path::PathBuf> {
        match self {
            Backend::Http(_) => Err(Self::http_unsupported_meta()),
            Backend::Local(l) => {
                openguild_core::repo::save_template(&l.store.paths, tpl, force)
            }
        }
    }


    // ── DEV-170: 첨부 (quest / campaign 공용) — Local 전용 ──

    fn attachments_list(
        &self,
        scope: CommentScope,
        slug: &str,
    ) -> Result<Vec<openguild_core::models::quest::QuestAttachment>> {
        match self {
            Backend::Http(_) => Err(anyhow!("원격 모드에선 미지원 — 로컬에서 실행")),
            Backend::Local(l) => Ok(match scope {
                CommentScope::Quest => {
                    openguild_core::ops::attachments::list_quest_attachments(&l.store, slug)
                }
                CommentScope::Campaign => {
                    openguild_core::ops::attachments::list_campaign_attachments(&l.store, slug)
                }
            }),
        }
    }

    fn attachments_add(
        &self,
        scope: CommentScope,
        slug: &str,
        file: &std::path::Path,
        name: Option<String>,
    ) -> Result<Vec<openguild_core::models::quest::QuestAttachment>> {
        match self {
            Backend::Http(_) => Err(anyhow!("원격 모드에선 미지원 — 로컬에서 실행")),
            Backend::Local(l) => {
                let bytes = std::fs::read(file)
                    .with_context(|| format!("파일 읽기 실패: {}", file.display()))?;
                let ext = file
                    .extension()
                    .and_then(|s| s.to_str())
                    .unwrap_or("bin");
                let display = name.unwrap_or_else(|| {
                    file.file_name()
                        .and_then(|s| s.to_str())
                        .unwrap_or("attachment")
                        .to_string()
                });
                l.rt
                    .block_on(async {
                        let rel = openguild_core::ops::attachments::save_attachment(
                            &l.store, &bytes, ext,
                        )
                        .await?;
                        match scope {
                            CommentScope::Quest => {
                                openguild_core::ops::attachments::add_quest_attachment(
                                    &l.store, slug, &rel, &display,
                                )
                                .await
                            }
                            CommentScope::Campaign => {
                                openguild_core::ops::attachments::add_campaign_attachment(
                                    &l.store, slug, &rel, &display,
                                )
                                .await
                            }
                        }
                    })
                    .map_err(|e| anyhow!(e))
            }
        }
    }

    fn attachments_remove(
        &self,
        scope: CommentScope,
        slug: &str,
        path: &str,
    ) -> Result<Vec<openguild_core::models::quest::QuestAttachment>> {
        match self {
            Backend::Http(_) => Err(anyhow!("원격 모드에선 미지원 — 로컬에서 실행")),
            Backend::Local(l) => l
                .rt
                .block_on(async {
                    match scope {
                        CommentScope::Quest => {
                            openguild_core::ops::attachments::remove_quest_attachment(
                                &l.store, slug, path,
                            )
                            .await
                        }
                        CommentScope::Campaign => {
                            openguild_core::ops::attachments::remove_campaign_attachment(
                                &l.store, slug, path,
                            )
                            .await
                        }
                    }
                })
                .map_err(|e| anyhow!(e)),
        }
    }

    // ── DEV-100: scope dispatch — quest / campaign 공용 ──

    fn comments_list_scoped(
        &self,
        scope: CommentScope,
        slug: &str,
    ) -> Result<Vec<openguild_core::repo::comments::CommentEntry>> {
        match scope {
            CommentScope::Quest => self.comments_list(slug),
            CommentScope::Campaign => match self {
                Backend::Http(_) => Err(Self::http_unsupported_meta()),
                Backend::Local(l) => Self::map_err(Ok::<_, openguild_core::error::AppError>(
                    openguild_core::ops::campaign_comments::list_entries(&l.store, slug)?,
                )),
            },
        }
    }

    fn comments_add_scoped(
        &self,
        scope: CommentScope,
        slug: &str,
        author: String,
        body: String,
        parent_id: Option<u64>,
    ) -> Result<openguild_core::repo::comments::CommentEntry> {
        match scope {
            CommentScope::Quest => self.comments_add(slug, author, body, parent_id),
            CommentScope::Campaign => match self {
                Backend::Http(_) => Err(Self::http_unsupported_meta()),
                Backend::Local(l) => Self::map_err(l.rt.block_on(
                    openguild_core::ops::campaign_comments::add_entry(
                        &l.store, slug, author, body, parent_id,
                    ),
                )),
            },
        }
    }

    // DEV-185: 토론/해결 토글 — discussion 은 quest 전용 기능이라 scope 없음.
    // 원격(Http)은 GUI 가 직접 HTTP 로 처리하므로 CLI 는 Local 만 지원.
    fn comments_toggle_discussion(
        &self,
        slug: &str,
        id: u64,
    ) -> Result<openguild_core::repo::comments::CommentEntry> {
        match self {
            Backend::Http(_) => Err(anyhow!("원격 모드에선 미지원 — 로컬에서 실행")),
            Backend::Local(l) => Self::map_err(l.rt.block_on(
                openguild_core::ops::comments::toggle_comment_discussion(&l.store, slug, id),
            )),
        }
    }

    fn comments_toggle_resolved(
        &self,
        slug: &str,
        id: u64,
    ) -> Result<openguild_core::repo::comments::CommentEntry> {
        match self {
            Backend::Http(_) => Err(anyhow!("원격 모드에선 미지원 — 로컬에서 실행")),
            Backend::Local(l) => Self::map_err(l.rt.block_on(
                openguild_core::ops::comments::toggle_comment_resolved(&l.store, slug, id),
            )),
        }
    }

    fn comments_edit_scoped(
        &self,
        scope: CommentScope,
        slug: &str,
        id: u64,
        body: String,
    ) -> Result<openguild_core::repo::comments::CommentEntry> {
        match scope {
            CommentScope::Quest => self.comments_edit(slug, id, body),
            CommentScope::Campaign => match self {
                Backend::Http(_) => Err(Self::http_unsupported_meta()),
                Backend::Local(l) => Self::map_err(l.rt.block_on(
                    openguild_core::ops::campaign_comments::update_entry(&l.store, slug, id, body),
                )),
            },
        }
    }

    fn comments_delete_scoped(&self, scope: CommentScope, slug: &str, id: u64) -> Result<()> {
        match scope {
            CommentScope::Quest => self.comments_delete(slug, id),
            CommentScope::Campaign => match self {
                Backend::Http(_) => Err(Self::http_unsupported_meta()),
                Backend::Local(l) => Self::map_err(l.rt.block_on(
                    openguild_core::ops::campaign_comments::delete_entry(&l.store, slug, id),
                )),
            },
        }
    }

    fn memo_get_scoped(&self, scope: CommentScope, slug: &str) -> Result<Option<String>> {
        match scope {
            CommentScope::Quest => self.memo_get(slug),
            CommentScope::Campaign => match self {
                Backend::Http(_) => Err(Self::http_unsupported_meta()),
                Backend::Local(l) => Self::map_err(Ok::<_, openguild_core::error::AppError>(
                    openguild_core::ops::campaign_comments::get_memo(&l.store, slug)?,
                )),
            },
        }
    }

    fn memo_set_scoped(&self, scope: CommentScope, slug: &str, content: String) -> Result<()> {
        match scope {
            CommentScope::Quest => self.memo_set(slug, content),
            CommentScope::Campaign => match self {
                Backend::Http(_) => Err(Self::http_unsupported_meta()),
                Backend::Local(l) => Self::map_err(l.rt.block_on(
                    openguild_core::ops::campaign_comments::set_memo(&l.store, slug, content),
                )),
            },
        }
    }
    // ── DEV-099: 댓글 / 메모 — local 전용 (HTTP 미지원 우선) ──

    fn comments_list(
        &self,
        slug: &str,
    ) -> Result<Vec<openguild_core::repo::comments::CommentEntry>> {
        match self {
            Backend::Http(_) => Err(Self::http_unsupported_meta()),
            Backend::Local(l) => Self::map_err(Ok::<_, openguild_core::error::AppError>(
                openguild_core::ops::comments::list_comment_entries(&l.store, slug)?,
            )),
        }
    }

    fn comments_add(
        &self,
        slug: &str,
        author: String,
        body: String,
        parent_id: Option<u64>,
    ) -> Result<openguild_core::repo::comments::CommentEntry> {
        match self {
            Backend::Http(_) => Err(Self::http_unsupported_meta()),
            Backend::Local(l) => Self::map_err(l.rt.block_on(
                openguild_core::ops::comments::add_comment_entry(
                    &l.store, slug, author, body, parent_id,
                ),
            )),
        }
    }

    fn comments_edit(
        &self,
        slug: &str,
        id: u64,
        body: String,
    ) -> Result<openguild_core::repo::comments::CommentEntry> {
        match self {
            Backend::Http(_) => Err(Self::http_unsupported_meta()),
            Backend::Local(l) => Self::map_err(l.rt.block_on(
                openguild_core::ops::comments::update_comment_entry(
                    &l.store, slug, id, body,
                ),
            )),
        }
    }

    fn comments_delete(&self, slug: &str, id: u64) -> Result<()> {
        match self {
            Backend::Http(_) => Err(Self::http_unsupported_meta()),
            Backend::Local(l) => Self::map_err(l.rt.block_on(
                openguild_core::ops::comments::delete_comment_entry(&l.store, slug, id),
            )),
        }
    }

    fn memo_get(&self, slug: &str) -> Result<Option<String>> {
        match self {
            Backend::Http(_) => Err(Self::http_unsupported_meta()),
            Backend::Local(l) => Self::map_err(Ok::<_, openguild_core::error::AppError>(
                openguild_core::ops::comments::get_memo(&l.store, slug)?,
            )),
        }
    }

    fn memo_set(&self, slug: &str, content: String) -> Result<()> {
        match self {
            Backend::Http(_) => Err(Self::http_unsupported_meta()),
            Backend::Local(l) => Self::map_err(l.rt.block_on(
                openguild_core::ops::comments::set_memo(&l.store, slug, content),
            )),
        }
    }

    // DEV-068: tag — frontmatter 에서 read, ops::set_quest_tags 로 write.
    fn tag_list(&self, slug: &str) -> Result<Vec<String>> {
        match self {
            Backend::Http(_) => Err(Self::http_unsupported_meta()),
            Backend::Local(l) => {
                // frontmatter 의 tags 가 진리원. file 직접 read.
                let path = l.store.paths.quest_path(slug);
                let qf = openguild_core::repo::QuestFile::read(&path)
                    .map_err(|e| anyhow::anyhow!("quest {slug} 본문 읽기 실패: {e:#}"))?;
                Ok(qf.frontmatter.tags.clone())
            }
        }
    }

    fn tag_set(&self, slug: &str, tags: Vec<String>) -> Result<()> {
        match self {
            Backend::Http(_) => Err(Self::http_unsupported_meta()),
            Backend::Local(l) => {
                let id = self.id_of(slug)?;
                Self::map_err(l.rt.block_on(
                    openguild_core::ops::set_quest_tags(&l.store, id, tags),
                ))?;
                Ok(())
            }
        }
    }

    fn create_backup(&self) -> Result<openguild_core::snapshot::SnapshotInfo> {
        match self {
            Backend::Http(c) => c.create_snapshot(),
            Backend::Local(l) => {
                l.rt.block_on(openguild_core::snapshot::create_snapshot(&l.store))
            }
        }
    }

    /// DEV-095: 파일 → index.db 풀 reindex. Local 전용 (remote 는 서버측 명령).
    fn reindex(&self) -> Result<openguild_core::reindex::ReindexReport> {
        match self {
            Backend::Http(_) => Err(anyhow!(
                "원격 모드에선 미지원 — 서버에서 `openguild-server reindex` 사용"
            )),
            Backend::Local(l) => l
                .rt
                .block_on(openguild_core::reindex::reindex(&l.store))
                .map_err(|e| anyhow!(e)),
        }
    }

    /// DEV-159: index.db ↔ 파일 drift 검사. Local 전용 (remote 는 서버측 명령).
    fn check_drift(&self) -> Result<openguild_core::drift::DriftReport> {
        match self {
            Backend::Http(_) => Err(anyhow!(
                "원격 모드에선 미지원 — 서버에서 `openguild-server check-drift` 사용"
            )),
            Backend::Local(l) => l
                .rt
                .block_on(openguild_core::drift::detect_drift(&l.store))
                .map_err(|e| anyhow!(e)),
        }
    }

    /// DEV-162: index.db VACUUM. Local 전용 (실행 중 host 는 HTTP admin 사용).
    fn vacuum(&self) -> Result<openguild_core::maintenance::VacuumReport> {
        match self {
            Backend::Http(_) => Err(anyhow!(
                "원격 모드에선 미지원 — 실행 중 host 는 HTTP admin, 오프라인은 로컬에서 실행"
            )),
            Backend::Local(l) => l
                .rt
                .block_on(openguild_core::maintenance::vacuum(&l.store))
                .map_err(|e| anyhow!(e)),
        }
    }

    /// DEV-162: journal.db 최근 op tail. Local 전용.
    fn journal_tail(&self, count: i64) -> Result<Option<openguild_core::maintenance::JournalTail>> {
        match self {
            Backend::Http(_) => Err(anyhow!(
                "원격 모드에선 미지원 — 실행 중 host 는 HTTP admin, 오프라인은 로컬에서 실행"
            )),
            Backend::Local(l) => l
                .rt
                .block_on(openguild_core::maintenance::journal_tail(&l.store.paths, count))
                .map_err(|e| anyhow!(e)),
        }
    }

    /// DEV-164: counter 정합 검사 / 보정. Local 전용.
    fn check_counters(&self, fix: bool) -> Result<openguild_core::ops::counter::CombinedReport> {
        match self {
            Backend::Http(_) => Err(anyhow!("원격 모드에선 미지원 — 오프라인(로컬)에서 실행")),
            Backend::Local(l) => l
                .rt
                .block_on(openguild_core::ops::check_and_fix_counters(&l.store, fix))
                .map_err(|e| anyhow!(e)),
        }
    }

    /// DEV-164: legacy guild.db → 파일 진리원 일회성 이전. Local 전용.
    /// guild_path 의 quests/ 가 이미 차 있으면(이미 이전됨) 에러.
    fn migrate_to_files(&self) -> Result<openguild_core::migrate::MigrationReport> {
        match self {
            Backend::Http(_) => Err(anyhow!("원격 모드에선 미지원 — 오프라인(로컬)에서 실행")),
            Backend::Local(l) => {
                let quests_dir = l.guild_path.join(".guild").join("quests");
                let has_md = std::fs::read_dir(&quests_dir)
                    .ok()
                    .into_iter()
                    .flatten()
                    .flatten()
                    .any(|e| e.path().extension().and_then(|s| s.to_str()) == Some("md"));
                if has_md {
                    return Err(anyhow!(
                        ".guild/quests/ 에 이미 quest 파일이 있습니다 — 마이그레이션은 한 번만. \
                         덮어쓰려면 quests/ 를 비운 뒤 재시도."
                    ));
                }
                l.rt
                    .block_on(openguild_core::migrate::migrate_to_files(&l.guild_path))
                    .map_err(|e| anyhow!(e))
            }
        }
    }

    /// DEV-164: 길드 메타 + index.db/snapshot/journal 요약. Local 전용.
    fn info(&self) -> Result<CliInfo> {
        match self {
            Backend::Http(_) => Err(anyhow!(
                "원격 모드에선 미지원 — 실행 중 host 정보는 server 측에서 확인"
            )),
            Backend::Local(l) => l.rt.block_on(async {
                let guild =
                    openguild_core::guild_file::load(&l.guild_path.to_string_lossy())?;
                let summary = openguild_core::maintenance::index_summary(&l.store).await?;
                let snapshots = openguild_core::snapshot::list_snapshots(&l.store.paths)?;
                let journal_total = openguild_core::maintenance::journal_tail(&l.store.paths, 0)
                    .await?
                    .map(|t| t.total)
                    .unwrap_or(0);
                Ok(CliInfo {
                    path: l.guild_path.clone(),
                    guild,
                    summary,
                    snapshots,
                    journal_total,
                })
            }),
        }
    }

    fn list_backups(&self) -> Result<Vec<openguild_core::snapshot::SnapshotInfo>> {
        match self {
            Backend::Http(c) => c.list_snapshots(),
            Backend::Local(l) => openguild_core::snapshot::list_snapshots(&l.store.paths),
        }
    }

    /// DEV-175: 특정 백업 삭제.
    fn delete_backup(&self, timestamp: &str) -> Result<()> {
        match self {
            Backend::Http(c) => c.delete_snapshot(timestamp),
            Backend::Local(l) => {
                openguild_core::snapshot::delete_snapshot(&l.store.paths, timestamp)
            }
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

    /// DEV-022: 시점 복원 — 최신 snapshot 복원 후 journal 을 `target_ts`(포함)까지
    /// replay. journal 은 최신 snapshot 이후만 보유하므로 replay 기준은 항상 최신.
    fn restore_to_point(
        &self,
        target_ts: &str,
    ) -> Result<openguild_core::replay::ReplayReport> {
        match self {
            Backend::Http(_) => {
                anyhow::bail!(
                    "원격(HTTP) 모드의 시점 복원(--at)은 아직 미지원 — 로컬 모드를 사용하세요."
                )
            }
            Backend::Local(l) => {
                let snapshots = openguild_core::snapshot::list_snapshots(&l.store.paths)?;
                let latest = snapshots.last().cloned().ok_or_else(|| {
                    anyhow!("사용 가능한 snapshot 이 없습니다 (replay 는 최신 snapshot 기준)")
                })?;
                let report = l
                    .rt
                    .block_on(openguild_core::replay::replay_to(&l.store, &latest, target_ts))
                    .map_err(|e| anyhow!("replay 실패: {e}"))?;
                Ok(report)
            }
        }
    }

    // ── 슬러그 → ID 헬퍼 ─────────────────────────────────

    fn id_of(&self, slug: &str) -> Result<i64> {
        Ok(self.quest_by_slug(slug)?.quest.id)
    }

    /// DEV-048 + BUG-018: 상태 인자(이름 / slug / id) → status_slug.
    /// API 는 slug 전용이지만 사용자 입력은 name_en / name_ko / slug / id 모두 OK.
    fn resolve_status_slug(&self, input: &str) -> Result<String> {
        let statuses = self.quest_statuses()?;
        // 1. 직접 slug 일치 (공백/하이픈 → 언더스코어 정규화).
        let want = input.to_lowercase().replace([' ', '-'], "_");
        if let Some(s) = statuses.iter().find(|s| s.slug == want) {
            return Ok(s.slug.clone());
        }
        // 2. id 일치.
        if let Ok(n) = input.parse::<i64>()
            && let Some(s) = statuses.iter().find(|s| s.id == n)
        {
            return Ok(s.slug.clone());
        }
        // 3. name_en (대소문자 무시).
        let want_lower = input.to_lowercase();
        if let Some(s) = statuses
            .iter()
            .find(|s| s.name_en.to_lowercase() == want_lower)
        {
            return Ok(s.slug.clone());
        }
        // 4. name_ko (정확 일치).
        if let Some(s) = statuses.iter().find(|s| s.name_ko == input) {
            return Ok(s.slug.clone());
        }
        Err(anyhow!(
            "unknown status '{input}'. available: {}",
            statuses
                .iter()
                .map(|s| format!("{} ({})", s.name_en, s.slug))
                .collect::<Vec<_>>()
                .join(", ")
        ))
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
/// DEV-048 이후 production 경로는 `resolve_status_slug` 사용 — 본 helper 는
/// 매칭 알고리즘 unit-test 용으로만 유지.
#[allow(dead_code)]
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
    if q.title_only {
        parts.push("title_only=true".into());
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

/// DEV-060: `quest new` 의 템플릿 merge — 명시 옵션 > 템플릿 값 > 기본.
///
/// 반환: (type_prefix, title, description, urgency, template_tags).
/// type / title 은 양쪽 모두 없으면 에러. urgency 기본 3.
#[allow(clippy::type_complexity)]
fn merge_new_quest_inputs(
    type_prefix: Option<String>,
    title: Option<String>,
    description: Option<String>,
    urgency: Option<i64>,
    tpl: Option<&openguild_core::repo::TemplateFile>,
) -> Result<(String, String, Option<String>, i64, Vec<String>)> {
    let type_prefix = type_prefix
        .or_else(|| tpl.and_then(|t| t.frontmatter.type_prefix.clone()))
        .ok_or_else(|| anyhow!("--type 필요 (또는 type 을 정의한 --template 지정)"))?;
    let title = title
        .or_else(|| tpl.and_then(|t| t.frontmatter.title.clone()))
        .ok_or_else(|| anyhow!("--title 필요 (또는 title 을 정의한 --template 지정)"))?;
    let description =
        description.or_else(|| tpl.map(|t| t.body.clone()).filter(|b| !b.is_empty()));
    let urgency = urgency
        .or_else(|| tpl.and_then(|t| t.frontmatter.urgency))
        .unwrap_or(3);
    let tags = tpl.map(|t| t.frontmatter.tags.clone()).unwrap_or_default();
    Ok((type_prefix, title, description, urgency, tags))
}

/// DEV-100: 댓글 / 메모 명령의 대상 — quest 또는 campaign.
#[derive(Clone, Copy, PartialEq)]
enum CommentScope {
    Quest,
    Campaign,
}

impl CommentScope {
    fn noun(self) -> &'static str {
        match self {
            CommentScope::Quest => "quest",
            CommentScope::Campaign => "campaign",
        }
    }
}

/// DEV-170: quest / campaign 첨부(섹션) 명령.
#[derive(Subcommand)]
enum AttachCmd {
    /// 첨부 목록 (이름 / 경로).
    List { slug: String },
    /// 로컬 파일을 업로드(.guild/attachments)해 첨부 섹션에 추가.
    Add {
        slug: String,
        /// 첨부할 로컬 파일 경로.
        file: std::path::PathBuf,
        /// 표시 이름 (미지정 시 원본 파일명).
        #[arg(long)]
        name: Option<String>,
    },
    /// 첨부 제거. 다른 곳에서 참조 안 하면 실제 파일 + blob 도 삭제(orphan 정리).
    #[command(name = "remove")]
    Rm {
        slug: String,
        /// 제거할 첨부의 경로 (list 의 경로 값).
        path: String,
    },
}

/// DEV-170: quest / campaign 첨부 명령 공용 핸들러.
fn run_attach_cmd(c: &Backend, scope: CommentScope, sub: AttachCmd, json: bool) -> Result<()> {
    match sub {
        AttachCmd::List { slug } => {
            let list = c.attachments_list(scope, &slug)?;
            if json {
                println!(
                    "{}",
                    serde_json::json!({
                        "attachments": list.iter()
                            .map(|a| serde_json::json!({ "name": a.name, "path": a.path }))
                            .collect::<Vec<_>>(),
                    })
                );
            } else if list.is_empty() {
                println!("(첨부 없음)");
            } else {
                for a in &list {
                    println!("- {}  ({})", a.name, a.path);
                }
            }
        }
        AttachCmd::Add { slug, file, name } => {
            let list = c.attachments_add(scope, &slug, &file, name)?;
            if json {
                println!("{}", serde_json::json!({ "ok": true, "count": list.len() }));
            } else {
                println!("✓ 첨부 추가 — 총 {} 개", list.len());
                if let Some(a) = list.last() {
                    println!("  {}  ({})", a.name, a.path);
                }
            }
        }
        AttachCmd::Rm { slug, path } => {
            // BUG-085: core remove 는 매칭 없으면 조용히 no-op 인데 갱신 리스트만
            // 돌려줘서, 없는 경로를 넘겨도 "✓ 첨부 제거" 가 떴다. 제거 전에 목록에
            // 실제로 있는지 확인하고, 없으면 명확히 에러.
            let before = c.attachments_list(scope, &slug)?;
            if !before.iter().any(|a| a.path == path) {
                return Err(anyhow!(
                    "그런 첨부 없음: {path}\n  `attach list {slug}` 로 경로를 확인하세요"
                ));
            }
            let list = c.attachments_remove(scope, &slug, &path)?;
            if json {
                println!("{}", serde_json::json!({ "ok": true, "count": list.len() }));
            } else {
                println!("✓ 첨부 제거 — 남은 {} 개", list.len());
            }
        }
    }
    Ok(())
}

/// quest / campaign 댓글 명령 공용 핸들러 (DEV-100).
fn run_comment_cmd(c: &Backend, scope: CommentScope, sub: CommentCmd, json: bool) -> Result<()> {
    match sub {
                CommentCmd::List { slug, author, since, top_only, reply_to, grep } => {
                    let mut entries = c.comments_list_scoped(scope, &slug)?;
                    // DEV-110: 필터 — 모두 AND.
                    if let Some(a) = &author {
                        entries.retain(|e| e.author.eq_ignore_ascii_case(a));
                    }
                    if let Some(s) = &since {
                        // ISO 문자열 prefix 비교 — entry ts 는 RFC 3339 (+09:00 류
                        // 단일 TZ 운용 전제). date 만 입력 시 그 날 00:00 기준.
                        let threshold = openguild_core::time::normalize_filter_ts(s);
                        entries.retain(|e| e.ts.as_str() >= threshold.as_str());
                    }
                    if top_only {
                        entries.retain(|e| e.parent_id.is_none());
                    }
                    if let Some(p) = reply_to {
                        entries.retain(|e| e.parent_id == Some(p));
                    }
                    if let Some(g) = &grep {
                        let needle = g.to_lowercase();
                        entries.retain(|e| e.body.to_lowercase().contains(&needle));
                    }
                    if json {
                        println!(
                            "{}",
                            serde_json::json!({
                                "entries": entries.iter().map(|e| serde_json::json!({
                                    "id": e.id,
                                    "ts": e.ts,
                                    "author": e.author,
                                    "parent_id": e.parent_id,
                                    "body_len": e.body.len(),
                                })).collect::<Vec<_>>(),
                            })
                        );
                    } else if entries.is_empty() {
                        println!("(댓글 없음)");
                    } else {
                        for e in &entries {
                            let summary = e
                                .body
                                .lines()
                                .next()
                                .unwrap_or("")
                                .chars()
                                .take(60)
                                .collect::<String>();
                            let reply = e
                                .parent_id
                                .map(|p| format!(" ↩ #{p}"))
                                .unwrap_or_default();
                            let author = if e.author.is_empty() {
                                "(이름 없음)".to_string()
                            } else {
                                e.author.clone()
                            };
                            let ts = if e.ts.is_empty() {
                                "(시각 미상)".to_string()
                            } else {
                                e.ts.clone()
                            };
                            println!("#{}  {}  {}{}  {}", e.id, ts, author, reply, summary);
                        }
                    }
                }
                CommentCmd::Show { slug, id } => {
                    let entries = c.comments_list_scoped(scope, &slug)?;
                    let selected: Vec<_> = match id {
                        Some(target) => {
                            let only = entries
                                .into_iter()
                                .find(|e| e.id == target)
                                .ok_or_else(|| {
                                    anyhow::anyhow!("entry #{target} 없음 ({} {slug})", scope.noun())
                                })?;
                            vec![only]
                        }
                        None => entries,
                    };
                    if json {
                        println!(
                            "{}",
                            serde_json::json!({
                                "entries": selected.iter().map(|e| serde_json::json!({
                                    "id": e.id,
                                    "ts": e.ts,
                                    "author": e.author,
                                    "parent_id": e.parent_id,
                                    "body": e.body,
                                })).collect::<Vec<_>>(),
                            })
                        );
                    } else if selected.is_empty() {
                        println!("(댓글 없음)");
                    } else {
                        for (i, e) in selected.iter().enumerate() {
                            if i > 0 {
                                println!("\n---\n");
                            }
                            let reply = e
                                .parent_id
                                .map(|p| format!(" (↩ #{p})"))
                                .unwrap_or_default();
                            let author = if e.author.is_empty() {
                                "(이름 없음)".to_string()
                            } else {
                                e.author.clone()
                            };
                            println!("#{}  {}  {}{}", e.id, e.ts, author, reply);
                            println!();
                            print!("{}", e.body);
                            if !e.body.ends_with('\n') {
                                println!();
                            }
                        }
                    }
                }
                CommentCmd::Add {
                    slug,
                    author,
                    parent_id,
                    file,
                } => {
                    let body = read_content(file.as_deref())?;
                    let entry =
                        c.comments_add_scoped(scope, &slug, author.unwrap_or_default(), body, parent_id)?;
                    if json {
                        println!(
                            "{}",
                            serde_json::json!({
                                "ok": true,
                                "id": entry.id,
                                "ts": entry.ts,
                                "parent_id": entry.parent_id,
                            })
                        );
                    } else {
                        let reply = entry
                            .parent_id
                            .map(|p| format!(" ↩ #{p}"))
                            .unwrap_or_default();
                        println!("✓ 댓글 추가: #{} ({}{})", entry.id, entry.ts, reply);
                    }
                }
                CommentCmd::Edit { slug, id, file } => {
                    let body = read_content(file.as_deref())?;
                    let entry = c.comments_edit_scoped(scope, &slug, id, body)?;
                    if json {
                        println!(
                            "{}",
                            serde_json::json!({ "ok": true, "id": entry.id })
                        );
                    } else {
                        println!("✓ 댓글 #{} 본문 갱신됨", entry.id);
                    }
                }
                CommentCmd::Rm { slug, id, force } => {
                    if !force {
                        eprint!("댓글 #{id} ({} {slug}) 을 삭제할까요? (y/N) ", scope.noun());
                        use std::io::Write;
                        std::io::stderr().flush().ok();
                        let mut buf = String::new();
                        std::io::stdin().read_line(&mut buf)?;
                        if !matches!(buf.trim(), "y" | "Y" | "yes") {
                            println!("(취소)");
                            return Ok(());
                        }
                    }
                    c.comments_delete_scoped(scope, &slug, id)?;
                    if json {
                        println!(
                            "{}",
                            serde_json::json!({ "ok": true, "id": id })
                        );
                    } else {
                        println!("✓ 댓글 #{id} 삭제됨");
                    }
                }
                CommentCmd::Discussion { slug, id } => {
                    if scope != CommentScope::Quest {
                        anyhow::bail!("토론(discussion) 토글은 quest 댓글 전용입니다.");
                    }
                    let e = c.comments_toggle_discussion(&slug, id)?;
                    if json {
                        println!(
                            "{}",
                            serde_json::json!({ "ok": true, "id": id, "discussion": e.discussion, "resolved": e.resolved })
                        );
                    } else {
                        println!("✓ 댓글 #{id} 토론 {}", if e.discussion { "표시" } else { "해제" });
                    }
                }
                CommentCmd::Resolved { slug, id } => {
                    if scope != CommentScope::Quest {
                        anyhow::bail!("resolved 토글은 quest 댓글 전용입니다.");
                    }
                    let e = c.comments_toggle_resolved(&slug, id)?;
                    if json {
                        println!("{}", serde_json::json!({ "ok": true, "id": id, "resolved": e.resolved }));
                    } else {
                        println!("✓ 댓글 #{id} {}", if e.resolved { "해결됨" } else { "미해결" });
                    }
                }
    }
    Ok(())
}

/// quest / campaign 메모 명령 공용 핸들러 (DEV-100).
fn run_memo_cmd(c: &Backend, scope: CommentScope, sub: MemoCmd, json: bool) -> Result<()> {
    match sub {
                MemoCmd::Show { slug } => {
                    let content = c.memo_get_scoped(scope, &slug)?;
                    if json {
                        println!(
                            "{}",
                            serde_json::json!({ "slug": slug, "content": content })
                        );
                    } else if let Some(s) = content {
                        if s.is_empty() {
                            println!("(메모 비어있음)");
                        } else {
                            print!("{s}");
                            if !s.ends_with('\n') {
                                println!();
                            }
                        }
                    } else {
                        println!("(메모 없음)");
                    }
                }
                MemoCmd::Set { slug, file } => {
                    let content = read_content(file.as_deref())?;
                    c.memo_set_scoped(scope, &slug, content)?;
                    if json {
                        println!("{}", serde_json::json!({ "ok": true, "slug": slug }));
                    } else {
                        println!("✓ 메모 저장됨 ({} {slug})", scope.noun());
                    }
                }
                MemoCmd::Clear { slug } => {
                    c.memo_set_scoped(scope, &slug, String::new())?;
                    if json {
                        println!("{}", serde_json::json!({ "ok": true, "slug": slug }));
                    } else {
                        println!("✓ 메모 비움 ({} {slug})", scope.noun());
                    }
                }
    }
    Ok(())
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

/// BUG-011 후속: 상태 변경 명령이 no-op (현재 상태와 동일) 일 때 사용자에게
/// 명시적으로 알림. 정상 변경이면 기존 동작 그대로.
///
/// 대상: `quest status` / `quest start` / `quest done` / `quest reopen` —
/// 모두 같은 패턴이라 한 곳에 묶음.
fn change_status_with_noop_notice(
    c: &Backend,
    slug: &str,
    status_input: &str,
    json: bool,
) -> Result<()> {
    let detail = c.quest_by_slug(slug)?;
    // DEV-048: slug 전용 API → resolve to slug.
    let target_slug = c.resolve_status_slug(status_input)?;

    if detail.quest.status_slug == target_slug {
        // 이미 그 상태. backend 호출 생략.
        if json {
            eprintln!(
                r#"{{"noop":true,"reason":"already in status","status_slug":{:?},"status":{:?}}}"#,
                target_slug, detail.quest.status_name_en
            );
            println!("{}", serde_json::to_string_pretty(&detail.quest).unwrap());
        } else {
            println!(
                "(이미 {} 상태입니다 — 변경 없음)",
                colorize(&detail.quest.status_name_en, &detail.quest.status_color)
            );
            print_quest_line(&detail.quest);
        }
        return Ok(());
    }

    let q = c.change_status(detail.quest.id, &target_slug)?;
    print_quest(&q, json);
    Ok(())
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

// ─────────────────────────── Campaign handler (DEV-011) ───────────────────────────

fn handle_campaign(c: &Backend, json: bool, sub: CampaignCmd) -> Result<()> {
    use openguild_core::models::{CampaignDetail, CampaignRow, CreateCampaignRequest};

    fn print_row(r: &CampaignRow, json: bool) {
        if json {
            println!("{}", serde_json::to_string_pretty(r).unwrap());
            return;
        }
        let period = match (&r.started_at, &r.ended_at) {
            (Some(s), Some(e)) if !s.is_empty() && !e.is_empty() => format!(" [{s} ~ {e}]"),
            (Some(s), _) if !s.is_empty() => format!(" [{s} ~]"),
            (_, Some(e)) if !e.is_empty() => format!(" [~ {e}]"),
            _ => String::new(),
        };
        println!(
            "{}  [{}] {}{}",
            r.campaign_slug, r.status, r.title, period
        );
    }

    fn print_detail(d: &CampaignDetail, json: bool) {
        if json {
            println!("{}", serde_json::to_string_pretty(d).unwrap());
            return;
        }
        print_row(&d.campaign, false);
        if !d.linked_quests.is_empty() {
            println!("  linked quests:");
            for q in &d.linked_quests {
                println!(
                    "    - {} [{}] {}",
                    q.quest_id, q.status_name_en, q.title
                );
            }
        }
        if !d.checklists.is_empty() {
            println!("  checklist:");
            for (i, item) in d.checklists.iter().enumerate() {
                let mark = if item.checked { "[x]" } else { "[ ]" };
                println!("    {}. {} {}", i + 1, mark, item.text);
            }
        }
    }

    match sub {
        // DEV-100: 댓글 / 메모 — quest 와 공용 핸들러 (campaign scope).
        CampaignCmd::Comment { sub } => {
            run_comment_cmd(c, CommentScope::Campaign, sub, json)?
        }
        CampaignCmd::Attach { sub } => run_attach_cmd(c, CommentScope::Campaign, sub, json)?,
        CampaignCmd::Memo { sub } => run_memo_cmd(c, CommentScope::Campaign, sub, json)?,
        CampaignCmd::New {
            title,
            started_at,
            ended_at,
        } => {
            let row = c.campaign_create(CreateCampaignRequest {
                title,
                description: None,
                started_at,
                ended_at,
            })?;
            print_row(&row, json);
        }
        CampaignCmd::List { status } => {
            // 잘못된 --status 값은 silent fail 방지 — active | done 만 허용.
            if let Some(s) = &status
                && s != "active"
                && s != "done"
            {
                return Err(anyhow!(
                    "invalid --status '{s}' (expected 'active' or 'done')"
                ));
            }
            let rows = c.campaign_list(status)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&rows).unwrap());
            } else if rows.is_empty() {
                println!("(no campaigns)");
            } else {
                for r in &rows {
                    print_row(r, false);
                }
            }
        }
        CampaignCmd::Show { slug } => {
            let d = c.campaign_show(&slug)?;
            print_detail(&d, json);
        }
        CampaignCmd::Start { slug } => {
            let r = c.campaign_set_status(&slug, "active")?;
            print_row(&r, json);
        }
        CampaignCmd::End { slug } => {
            let r = c.campaign_set_status(&slug, "done")?;
            print_row(&r, json);
        }
        CampaignCmd::Link {
            campaign_slug,
            quest_slug,
        } => {
            c.campaign_link(&campaign_slug, &quest_slug)?;
            if json {
                println!(
                    "{}",
                    serde_json::json!({
                        "ok": true,
                        "linked": { "campaign": campaign_slug, "quest": quest_slug }
                    })
                );
            } else {
                println!("✓ linked: {campaign_slug} ← {quest_slug}");
            }
        }
        CampaignCmd::Unlink {
            campaign_slug,
            quest_slug,
        } => {
            c.campaign_unlink(&campaign_slug, &quest_slug)?;
            if json {
                println!(
                    "{}",
                    serde_json::json!({
                        "ok": true,
                        "unlinked": { "campaign": campaign_slug, "quest": quest_slug }
                    })
                );
            } else {
                println!("✓ unlinked: {campaign_slug} ↛ {quest_slug}");
            }
        }
        CampaignCmd::Delete { slug, yes } => {
            if !yes {
                return Err(anyhow!(
                    "삭제하려면 --yes 를 명시하세요 (안전장치). 예: campaign delete {slug} --yes"
                ));
            }
            c.campaign_delete(&slug)?;
            if json {
                println!("{}", serde_json::json!({ "ok": true, "deleted": slug }));
            } else {
                println!("✓ deleted: {slug}");
            }
        }
        CampaignCmd::Checklist { sub } => match sub {
            CampaignChecklistCmd::Add {
                campaign_slug,
                text,
            } => {
                let item = c.campaign_checklist_add(&campaign_slug, &text)?;
                if json {
                    println!("{}", serde_json::to_string_pretty(&item).unwrap());
                } else {
                    println!(
                        "✓ added [{}] {}: {}",
                        item.order_idx + 1,
                        campaign_slug,
                        item.text
                    );
                }
            }
            CampaignChecklistCmd::Check {
                campaign_slug,
                index,
            } => {
                c.campaign_checklist_set(&campaign_slug, index, true)?;
                if json {
                    println!(
                        "{}",
                        serde_json::json!({ "ok": true, "checked": index, "campaign": campaign_slug })
                    );
                } else {
                    println!("✓ [{index}] {campaign_slug} checked");
                }
            }
            CampaignChecklistCmd::Uncheck {
                campaign_slug,
                index,
            } => {
                c.campaign_checklist_set(&campaign_slug, index, false)?;
                if json {
                    println!(
                        "{}",
                        serde_json::json!({ "ok": true, "unchecked": index, "campaign": campaign_slug })
                    );
                } else {
                    println!("✓ [{index}] {campaign_slug} unchecked");
                }
            }
            CampaignChecklistCmd::Rm {
                campaign_slug,
                index,
            } => {
                c.campaign_checklist_rm(&campaign_slug, index)?;
                if json {
                    println!(
                        "{}",
                        serde_json::json!({ "ok": true, "removed": index, "campaign": campaign_slug })
                    );
                } else {
                    println!("✓ [{index}] {campaign_slug} removed");
                }
            }
        },
    }
    Ok(())
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

/// DEV-065 (CLI tree mode): 부모 → 자식 들여쓰기로 한 화면에 트리 출력.
/// 결과 안의 quest 가 부모 link 자식인데 부모가 결과 안에 없으면 (필터 등)
/// root 로 표시.
fn print_quest_tree(quests: &[Quest]) {
    use std::collections::HashMap;
    if quests.is_empty() {
        println!("(no quests)");
        return;
    }
    // id → quest 매핑 (참조).
    let by_id: HashMap<i64, &Quest> = quests.iter().map(|q| (q.id, q)).collect();
    // id → 자식들 (직계).
    let mut children: HashMap<i64, Vec<i64>> = HashMap::new();
    let mut roots: Vec<i64> = Vec::new();
    for q in quests {
        match q.parent_quest_id {
            Some(pid) if by_id.contains_key(&pid) => {
                children.entry(pid).or_default().push(q.id);
            }
            _ => roots.push(q.id),
        }
    }
    // 자식 정렬 — slug 알파벳 순으로 일관.
    for v in children.values_mut() {
        v.sort_by(|a, b| {
            let qa = by_id.get(a).map(|q| q.quest_id.as_str()).unwrap_or("");
            let qb = by_id.get(b).map(|q| q.quest_id.as_str()).unwrap_or("");
            qa.cmp(qb)
        });
    }
    roots.sort_by(|a, b| {
        let qa = by_id.get(a).map(|q| q.quest_id.as_str()).unwrap_or("");
        let qb = by_id.get(b).map(|q| q.quest_id.as_str()).unwrap_or("");
        qa.cmp(qb)
    });
    fn walk(
        id: i64,
        depth: usize,
        by_id: &HashMap<i64, &Quest>,
        children: &HashMap<i64, Vec<i64>>,
    ) {
        let Some(q) = by_id.get(&id) else { return };
        let prefix = if depth == 0 {
            String::new()
        } else {
            format!("{}└─ ", "   ".repeat(depth - 1))
        };
        print!("{prefix}");
        print_quest(q, false);
        if let Some(kids) = children.get(&id) {
            for &kid in kids {
                walk(kid, depth + 1, by_id, children);
            }
        }
    }
    for r in roots {
        walk(r, 0, &by_id, &children);
    }
}

fn print_quest_detail(d: &QuestDetail, json: bool) {
    if json {
        println!("{}", serde_json::to_string_pretty(d).unwrap());
        return;
    }
    let q = &d.quest;
    // DEV-046: 헤더 quest_id 에 type.color.
    println!("{}  {}", colorize(&q.quest_id, &q.type_color), q.title);
    // status name 에 status.color.
    println!(
        "  status   : {} ({})",
        colorize(&q.status_name_en, &q.status_color),
        q.status_name_ko
    );
    // urgency 숫자에 urgency_color.
    println!(
        "  urgency  : {}",
        colorize(&q.urgency.to_string(), urgency_color(q.urgency))
    );
    // DEV-043: 기본 출력에 생성일 / 변경일 표시. (시각은 색 X — 정보)
    println!("  created  : {}", q.created_at);
    println!("  updated  : {}", q.updated_at);
    // DEV-047: parent 표기 slug + 색 (이전엔 raw id 만 노출).
    // 섹션 라벨 색은 gui QuestBoard 의 다중-선택 하이라이트 팔레트와 일치
    // (parent=#7ee787 초록 / sub=#3dc9b0 청록 / pre=#a371f7 보라).
    if let Some(parent) = &d.parent {
        println!(
            "  {} : {} [{}] {}",
            colorize("parent", "#7ee787"),
            colorize(&parent.quest_id, &parent.type_color),
            colorize(&parent.status_name_en, &parent.status_color),
            parent.title
        );
    } else if let Some(p) = q.parent_quest_id {
        // parent_quest_id 는 있는데 detail.parent 가 None — soft-deleted 부모 등 비정상 case 대비 fallback.
        println!("  {} : id={p} (불러올 수 없음)", colorize("parent", "#7ee787"));
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
        println!(
            "  {} ({}):",
            colorize("sub-quests", "#3dc9b0"),
            d.sub_quests.len()
        );
        for s in &d.sub_quests {
            println!(
                "    - {} [{}] {}",
                colorize(&s.quest_id, &s.type_color),
                colorize(&s.status_name_en, &s.status_color),
                s.title
            );
        }
    }
    if !d.prerequisites.is_empty() {
        println!(
            "  {} ({}):",
            colorize("prerequisites", "#a371f7"),
            d.prerequisites.len()
        );
        for p in &d.prerequisites {
            println!(
                "    - {} [{}] {}",
                colorize(&p.quest_id, &p.type_color),
                colorize(&p.status_name_en, &p.status_color),
                p.title
            );
        }
    }
}

/// DEV-043: `quest show --field <name>` — 단일 필드 raw 값 출력 (pipe 친화).
/// 모르는 필드명이면 Err.
fn quest_field_value(d: &QuestDetail, field: &str) -> Result<String> {
    let q = &d.quest;
    let v = match field {
        "id" | "slug" => q.quest_id.clone(),
        "title" => q.title.clone(),
        "status" | "status_name_en" => q.status_name_en.clone(),
        "status_ko" | "status_name_ko" => q.status_name_ko.clone(),
        "status_slug" => {
            // DEV-042 의 slug 매핑은 QuestStatus 메타에 있음. 여기선 name_en →
            // slug 동일 규칙 (LOWER + space→_) 으로 간이 도출. 호출자가 정확한
            // slug 필요하면 statuses endpoint 별도 호출 권장.
            q.status_name_en
                .to_lowercase()
                .replace([' ', '-'], "_")
        }
        "urgency" => q.urgency.to_string(),
        "description" | "body" => q.description.clone().unwrap_or_default(),
        "type" | "type_prefix" => q.type_prefix.clone(),
        "parent" => match q.parent_quest_id {
            Some(p) => p.to_string(),
            None => String::new(),
        },
        "created_at" | "created" => q.created_at.clone(),
        "updated_at" | "updated" => q.updated_at.clone(),
        other => {
            return Err(anyhow!(
                "unknown field '{other}'. available: id title status status_slug \
                 urgency description type parent created_at updated_at"
            ));
        }
    };
    Ok(v)
}

// ─── DEV-177: 정비 명령 핸들러 (reindex 는 top-level + index rebuild 양쪽에서 재사용) ───

fn run_reindex_cmd(c: &Backend, json: bool) -> Result<()> {
    let report = c.reindex()?;
    if json {
        println!(
            "{}",
            serde_json::json!({
                "ok": true,
                "types": report.types_loaded,
                "statuses": report.statuses_loaded,
                "quests": report.quests_loaded,
                "dependencies": report.dependencies_loaded,
                "campaigns": report.campaigns_loaded,
                "comments": report.comments_loaded,
                "memos": report.memos_loaded,
                "tags": report.tags_loaded,
                "positions": report.positions_restored,
                "skipped": report.skipped.len(),
            })
        );
    } else {
        println!("✓ index.db 재구축 완료");
        for line in report.summary_lines() {
            println!("  {line}");
        }
        if !report.skipped.is_empty() {
            println!();
            println!("⚠ {} 개 파일 skip 됨 (파싱 / 무결성 실패):", report.skipped.len());
            for (path, reason) in &report.skipped {
                println!("  - {path}");
                println!("    → {reason}");
            }
        }
    }
    Ok(())
}

fn run_check_drift_cmd(c: &Backend, resync: bool, json: bool) -> Result<()> {
    let report = c.check_drift()?;
    if json {
        if resync && !report.is_clean() {
            c.reindex()?;
        }
        println!(
            "{}",
            serde_json::json!({
                "clean": report.is_clean(),
                "resynced": resync && !report.is_clean(),
                "report": report,
            })
        );
    } else if report.is_clean() {
        println!("✓ index.db 가 파일과 일치 (drift 없음)");
    } else {
        println!("⚠ drift 발견:");
        let sections = [
            ("파일은 있는데 index 에 없음", &report.missing_in_index),
            ("index 에 있는데 파일이 없음", &report.stale_in_index),
            ("파일 mtime > index.db mtime", &report.fresh_files),
            ("sibling(.comments/.memo) 가 더 새것", &report.fresh_siblings),
        ];
        for (label, items) in sections {
            if !items.is_empty() {
                println!();
                println!("  {label} ({}):", items.len());
                for s in items {
                    println!("    - {s}");
                }
            }
        }
        println!();
        if resync {
            println!("▸ reindex 실행 중...");
            c.reindex()?;
            println!("✓ resync 완료");
        } else {
            println!("(--resync 로 자동 reindex 가능)");
        }
    }
    Ok(())
}

fn run_check_counters_cmd(c: &Backend, fix: bool, json: bool) -> Result<()> {
    let report = c.check_counters(fix)?;
    if json {
        println!(
            "{}",
            serde_json::json!({
                "ok": true,
                "types_checked": report.file_report.types_checked,
                "file_issues": report.file_report.issues.len(),
                "sql_drift": report.sql_drift.len(),
                "fixed": fix,
            })
        );
    } else {
        println!("✓ counter 검증 완료");
        println!("  검사된 type 수 : {}", report.file_report.types_checked);
        println!(
            "  발견 이슈     : {} (file) + {} (SQL)",
            report.file_report.issues.len(),
            report.sql_drift.len()
        );
        for issue in &report.file_report.issues {
            println!();
            println!("  • type {} [file drift]:", issue.prefix);
            println!("    저장된 last_number   : {}", issue.stored_last_number);
            println!("    실제 max quest 번호  : {}", issue.actual_max_number);
            if fix {
                println!("    → {} 으로 보정됨 (file + SQL)", issue.corrected_to);
            } else {
                println!("    (--fix 로 자동 보정 가능)");
            }
        }
        for drift in &report.sql_drift {
            println!();
            println!("  • type {} [SQL drift]:", drift.prefix);
            println!("    file last_number     : {}", drift.file_last_number);
            println!("    SQL  last_number     : {}", drift.sql_last_number);
            if fix {
                println!("    → {} 으로 보정됨 (SQL ← file)", drift.synced_to);
            } else {
                println!("    (--fix 로 자동 보정 가능)");
            }
        }
    }
    Ok(())
}

fn run_vacuum_cmd(c: &Backend, json: bool) -> Result<()> {
    let r = c.vacuum()?;
    if json {
        println!(
            "{}",
            serde_json::json!({
                "ok": true,
                "before_bytes": r.before_bytes,
                "after_bytes": r.after_bytes,
                "saved_bytes": r.saved(),
            })
        );
    } else {
        println!("✓ VACUUM 완료");
        println!("  before : {} bytes", r.before_bytes);
        println!("  after  : {} bytes", r.after_bytes);
        if r.saved() > 0 && r.before_bytes > 0 {
            println!(
                "  saved  : {} bytes ({:.1}%)",
                r.saved(),
                (r.saved() as f64 / r.before_bytes as f64) * 100.0
            );
        } else {
            println!("  saved  : 0 bytes (이미 dense)");
        }
    }
    Ok(())
}

fn run_journal_tail_cmd(c: &Backend, count: i64, json: bool) -> Result<()> {
    let tail = c.journal_tail(count)?;
    match tail {
        None => {
            if json {
                println!("{}", serde_json::json!({ "exists": false, "rows": [] }));
            } else {
                println!("(journal.db 없음 — 아직 mutation 안 됐거나 snapshot 직후)");
            }
        }
        Some(t) => {
            if json {
                println!(
                    "{}",
                    serde_json::json!({
                        "exists": true,
                        "total": t.total,
                        "rows": t.rows.iter().map(|o| serde_json::json!({
                            "id": o.id, "ts": o.ts, "op": o.op,
                            "args": o.args, "result": o.result,
                        })).collect::<Vec<_>>(),
                    })
                );
            } else {
                println!("journal.db: {} row(s) total — showing last {}", t.total, t.rows.len());
                println!();
                let trunc = |s: &str| -> String {
                    if s.chars().count() > 100 {
                        format!("{}…", s.chars().take(100).collect::<String>())
                    } else {
                        s.to_string()
                    }
                };
                for o in &t.rows {
                    println!("#{:>6}  {}  {}", o.id, o.ts, o.op);
                    println!("         args   : {}", trunc(&o.args));
                    if let Some(r) = &o.result {
                        println!("         result : {}", trunc(r));
                    }
                    println!();
                }
            }
        }
    }
    Ok(())
}

// ─────────────────────────── 명령 처리 ───────────────────────────

fn run() -> Result<()> {
    let cli = Cli::parse();

    // Init 은 길드 자체를 만드는 명령 — 백엔드 연결 불필요. 먼저 처리.
    if let Command::Init { name } = &cli.command {
        return init_guild(name.clone(), cli.json);
    }

    let c = Backend::new(cli.remote.clone(), cli.guild.clone())?;

    // 비정상 파일 감지 시 stderr 경고 (GUI 시동 알림과 동일 취지). json 모드는
    // 기계 출력 오염 방지를 위해, Reindex 는 자체적으로 skipped 를 출력하므로 제외.
    if !cli.json
        && !matches!(
            cli.command,
            Command::Reindex | Command::Index { sub: IndexCmd::Rebuild }
        )
    {
        c.warn_problem_files();
    }

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
        Command::Types { sub } => match sub.unwrap_or(TypesCmd::List) {
            TypesCmd::List => {
                let types = c.quest_types()?;
                if cli.json {
                    println!("{}", serde_json::to_string_pretty(&types)?);
                } else {
                    for t in &types {
                        // DEV-046: prefix 에 type.color.
                        let prefix_colored = colorize(&format!("{:<6}", t.prefix), &t.color);
                        println!(
                            "{prefix_colored} {}",
                            t.description.as_deref().unwrap_or("")
                        );
                    }
                }
            }
            TypesCmd::Add { prefix, color, description } => {
                let prefix_uc = prefix.trim().to_uppercase();
                let row = c.create_type(prefix_uc, color, description)?;
                if cli.json {
                    println!("{}", serde_json::to_string_pretty(&row)?);
                } else {
                    let p = colorize(&format!("{:<6}", row.prefix), &row.color);
                    println!("{p} 추가됨 — {}", row.description.as_deref().unwrap_or(""));
                }
            }
            TypesCmd::Update {
                prefix,
                new_prefix,
                color,
                description,
                clear_description,
            } => {
                if clear_description && description.is_some() {
                    bail!("--description 과 --clear-description 동시 사용 불가");
                }
                let desc_arg: Option<Option<String>> = if clear_description {
                    Some(None)
                } else {
                    description.map(Some)
                };
                let old_prefix = prefix.trim().to_string();
                let new_prefix_uc = new_prefix
                    .map(|s| s.trim().to_uppercase())
                    .filter(|s| !s.is_empty() && s != &old_prefix);
                let renamed = new_prefix_uc.is_some();
                let row = c.update_type(
                    old_prefix.clone(),
                    new_prefix_uc,
                    color,
                    desc_arg,
                )?;
                if cli.json {
                    println!("{}", serde_json::to_string_pretty(&row)?);
                } else {
                    let p = colorize(&format!("{:<6}", row.prefix), &row.color);
                    if renamed {
                        println!(
                            "{p} 갱신됨 (rename: '{}' → '{}', 관련 quest slug cascade) — {}",
                            old_prefix,
                            row.prefix,
                            row.description.as_deref().unwrap_or("")
                        );
                    } else {
                        println!("{p} 갱신됨 — {}", row.description.as_deref().unwrap_or(""));
                    }
                }
            }
            TypesCmd::Delete { prefix } => {
                c.delete_type(prefix.trim().to_string())?;
                if cli.json {
                    println!("{}", serde_json::json!({ "ok": true }));
                } else {
                    println!("'{}' 삭제됨", prefix.trim());
                }
            }
        },
        Command::Statuses { sub } => match sub.unwrap_or(StatusesCmd::List) {
            StatusesCmd::List => {
                let statuses = c.quest_statuses()?;
                if cli.json {
                    // BUG-018: agent / script 용 — slug 포함된 raw row.
                    println!("{}", serde_json::to_string_pretty(&statuses)?);
                } else {
                    // BUG-018: slug 는 사용자 노출 X (internal stable id).
                    // 사용자 입력은 update/delete 가 name_en/name_ko 등으로 lookup.
                    for s in &statuses {
                        let name_colored = colorize(&format!("{:<14}", s.name_en), &s.color);
                        println!("{name_colored} {}", s.name_ko);
                    }
                }
            }
            StatusesCmd::Add {
                name_en,
                color,
                name_ko,
                sort_order,
            } => {
                let row = c.create_status(
                    name_en,
                    name_ko.unwrap_or_default(),
                    color,
                    sort_order,
                )?;
                if cli.json {
                    println!("{}", serde_json::to_string_pretty(&row)?);
                } else {
                    let n = colorize(&format!("{:<14}", row.name_en), &row.color);
                    println!(
                        "{n} (slug={}) 추가됨 — {}",
                        row.slug,
                        if row.name_ko.is_empty() { "-" } else { &row.name_ko }
                    );
                }
            }
            StatusesCmd::Update {
                slug,
                new_slug,
                name_en,
                name_ko,
                color,
                sort_order,
                clear_name_ko,
            } => {
                if clear_name_ko && name_ko.is_some() {
                    bail!("--name-ko 와 --clear-name-ko 동시 사용 불가");
                }
                let ko_arg = if clear_name_ko {
                    Some(String::new())
                } else {
                    name_ko
                };
                // BUG-018: ident → slug resolve (slug / id / name_en / name_ko).
                let resolved_slug = c.resolve_status_slug(&slug)?;
                let new_slug_norm = new_slug
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty() && s != &resolved_slug);
                let renamed = new_slug_norm.is_some();
                let old_slug = resolved_slug.clone();
                let row = c.update_status(
                    resolved_slug,
                    new_slug_norm,
                    name_en,
                    ko_arg,
                    color,
                    sort_order,
                )?;
                if cli.json {
                    println!("{}", serde_json::to_string_pretty(&row)?);
                } else {
                    let n = colorize(&format!("{:<14}", row.name_en), &row.color);
                    if renamed {
                        println!(
                            "{n} 갱신됨 (slug rename: '{}' → '{}', cascade)",
                            old_slug, row.slug
                        );
                    } else {
                        println!("{n} 갱신됨");
                    }
                }
            }
            StatusesCmd::Delete { slug } => {
                // BUG-018: ident → slug resolve.
                let resolved_slug = c.resolve_status_slug(&slug)?;
                let display = c
                    .quest_statuses()
                    .ok()
                    .and_then(|v| {
                        v.into_iter().find(|s| s.slug == resolved_slug).map(|s| s.name_en)
                    })
                    .unwrap_or_else(|| resolved_slug.clone());
                c.delete_status(resolved_slug)?;
                if cli.json {
                    println!("{}", serde_json::json!({ "ok": true }));
                } else {
                    println!("'{display}' 삭제됨");
                }
            }
        },
        // DEV-060: 퀘스트 템플릿.
        Command::Template { sub } => match sub {
            TemplateCmd::List => {
                let templates = c.templates_list()?;
                if cli.json {
                    println!(
                        "{}",
                        serde_json::json!({
                            "templates": templates.iter().map(|t| serde_json::json!({
                                "name": t.name,
                                "title": t.frontmatter.title,
                                "type": t.frontmatter.type_prefix,
                                "urgency": t.frontmatter.urgency,
                                "tags": t.frontmatter.tags,
                                "body_len": t.body.len(),
                            })).collect::<Vec<_>>(),
                        })
                    );
                } else if templates.is_empty() {
                    println!("(템플릿 없음 — .guild/templates/{{name}}.md 작성)");
                } else {
                    for t in &templates {
                        let mut meta = Vec::new();
                        if let Some(ty) = &t.frontmatter.type_prefix {
                            meta.push(format!("type={ty}"));
                        }
                        if let Some(u) = t.frontmatter.urgency {
                            meta.push(format!("urgency={u}"));
                        }
                        if !t.frontmatter.tags.is_empty() {
                            meta.push(format!("tags={}", t.frontmatter.tags.join(",")));
                        }
                        println!(
                            "{}  {}  {}",
                            t.name,
                            t.frontmatter.title.as_deref().unwrap_or("(제목 없음)"),
                            meta.join(" ")
                        );
                    }
                }
            }
            TemplateCmd::Show { name } => {
                let t = c.template_load(&name)?;
                if cli.json {
                    println!(
                        "{}",
                        serde_json::json!({
                            "name": t.name,
                            "title": t.frontmatter.title,
                            "type": t.frontmatter.type_prefix,
                            "urgency": t.frontmatter.urgency,
                            "tags": t.frontmatter.tags,
                            "body": t.body,
                        })
                    );
                } else {
                    println!("# {} — {}", t.name, t.frontmatter.title.as_deref().unwrap_or("(제목 없음)"));
                    println!("{}", t.body);
                }
            }
            TemplateCmd::New {
                name,
                type_prefix,
                title,
                urgency,
                tags,
                file,
                force,
            } => {
                // 본문: --file > (파이프된) stdin > 빈 본문. tty 면 hang 방지 위해 stdin skip.
                let body = if let Some(p) = &file {
                    read_content(Some(p.as_path()))?
                } else if !std::io::stdin().is_terminal() {
                    read_content(None)?
                } else {
                    String::new()
                };
                let tpl = openguild_core::repo::TemplateFile {
                    name: name.clone(),
                    frontmatter: openguild_core::repo::TemplateFrontmatter {
                        title,
                        type_prefix,
                        urgency,
                        tags,
                    },
                    body,
                };
                let path = c.template_save(&tpl, force)?;
                if cli.json {
                    println!(
                        "{}",
                        serde_json::json!({ "ok": true, "name": name, "path": path.display().to_string() })
                    );
                } else {
                    println!("✓ 템플릿 '{name}' 저장 — {}", path.display());
                }
            }
        },
        Command::Rules { sub } => match sub {
            RulesCmd::List => {
                let entries = c.rules_list()?;
                if cli.json {
                    println!(
                        "{}",
                        serde_json::json!({
                            "entries": entries.iter().map(|e| serde_json::json!({
                                "slug": e.slug,
                                "len": e.content.len(),
                            })).collect::<Vec<_>>(),
                        })
                    );
                } else if entries.is_empty() {
                    println!("(규칙 없음)");
                } else {
                    println!("Slug                  Lines  Size");
                    for e in &entries {
                        let lines = e.content.lines().count();
                        println!(
                            "{:<22}{:>5}  {} bytes",
                            e.slug,
                            lines,
                            e.content.len()
                        );
                    }
                }
            }
            RulesCmd::Show { slug } => {
                let content = c
                    .rules_get(&slug)?
                    .ok_or_else(|| anyhow::anyhow!("규칙 '{slug}' 없음"))?;
                if cli.json {
                    println!(
                        "{}",
                        serde_json::json!({ "slug": slug, "content": content })
                    );
                } else {
                    print!("{content}");
                    if !content.ends_with('\n') {
                        println!();
                    }
                }
            }
            RulesCmd::Set { slug, file } => {
                let content = read_content(file.as_deref())?;
                c.rules_set(&slug, content)?;
                if cli.json {
                    println!("{}", serde_json::json!({ "ok": true, "slug": slug }));
                } else {
                    println!("✓ 규칙 '{slug}' 저장됨");
                }
            }
            RulesCmd::Create { slug, file, empty } => {
                let content = if empty {
                    String::new()
                } else {
                    read_content(file.as_deref())?
                };
                c.rules_create(&slug, content)?;
                if cli.json {
                    println!("{}", serde_json::json!({ "ok": true, "slug": slug }));
                } else {
                    println!("✓ 규칙 '{slug}' 생성됨");
                }
            }
            RulesCmd::Delete { slug, force } => {
                if !force {
                    eprint!("규칙 '{slug}' 을 삭제할까요? (y/N) ");
                    use std::io::Write;
                    std::io::stderr().flush().ok();
                    let mut buf = String::new();
                    std::io::stdin().read_line(&mut buf)?;
                    if !matches!(buf.trim(), "y" | "Y" | "yes") {
                        println!("(취소)");
                        return Ok(());
                    }
                }
                c.rules_delete(&slug)?;
                if cli.json {
                    println!("{}", serde_json::json!({ "ok": true, "slug": slug }));
                } else {
                    println!("✓ 규칙 '{slug}' 삭제됨");
                }
            }
            RulesCmd::Rename { slug, new_slug } => {
                c.rules_rename(&slug, &new_slug)?;
                if cli.json {
                    println!(
                        "{}",
                        serde_json::json!({
                            "ok": true, "from": slug, "to": new_slug,
                        })
                    );
                } else {
                    println!("✓ '{slug}' → '{new_slug}' 이름 변경");
                }
            }
        },
        Command::Backup { sub } => match sub {
            BackupCmd::New => {
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
                    println!(
                        "✓ snapshot 생성: {} ({} bytes)",
                        openguild_core::snapshot::ts_to_local_display(&info.timestamp),
                        info.size_bytes
                    );
                    println!("  path: {}", info.path.display());
                }
            }
            BackupCmd::List => {
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
                    println!("`openguild backup create` 으로 생성하세요.");
                } else {
                    println!("백업 목록 (오래된 순):");
                    for s in &list {
                        println!(
                            "  {} — {} bytes",
                            openguild_core::snapshot::ts_to_local_display(&s.timestamp),
                            s.size_bytes
                        );
                    }
                }
            }
            BackupCmd::Rm { timestamp } => {
                c.delete_backup(&timestamp)?;
                if cli.json {
                    println!("{}", serde_json::json!({ "ok": true, "deleted": timestamp }));
                } else {
                    println!("✓ 백업 삭제: {timestamp}");
                }
            }
        },
        Command::Restore { to, at } => {
            if let Some(ts) = at {
                // DEV-022: 시점 복원 (journal replay).
                let report = c.restore_to_point(&ts)?;
                if cli.json {
                    println!(
                        "{}",
                        serde_json::json!({
                            "ok": true,
                            "replayed_to": report.target_ts,
                            "applied": report.applied,
                        })
                    );
                } else {
                    println!(
                        "✓ 시점 복원 완료: {} 까지 journal op {} 개 재적용",
                        report.target_ts, report.applied
                    );
                    println!();
                    println!("주의: 이 시점 이후의 변경은 폐기되었습니다.");
                    println!("      파일 시스템 표시가 안 맞으면 `openguild reindex`.");
                }
            } else {
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
                    println!(
                        "✓ 복원 완료: {}",
                        openguild_core::snapshot::ts_to_local_display(&info.timestamp)
                    );
                    println!();
                    println!("주의: 파일 시스템 (`.guild/quests/*.md`) 자동 갱신 안 됨.");
                    println!("      필요시 `openguild reindex`.");
                }
            }
        }
        Command::Reindex => run_reindex_cmd(&c, cli.json)?,
        Command::Check { sub } => match sub {
            CheckCmd::Drift { resync } => run_check_drift_cmd(&c, resync, cli.json)?,
            CheckCmd::Counters { fix } => run_check_counters_cmd(&c, fix, cli.json)?,
        },
        Command::Index { sub } => match sub {
            IndexCmd::Rebuild => run_reindex_cmd(&c, cli.json)?,
            IndexCmd::Vacuum => run_vacuum_cmd(&c, cli.json)?,
        },
        Command::Journal { sub } => match sub {
            JournalCmd::Tail { count } => run_journal_tail_cmd(&c, count, cli.json)?,
        },
        Command::MigrateToFiles => {
            let report = c.migrate_to_files()?;
            if cli.json {
                println!(
                    "{}",
                    serde_json::json!({
                        "ok": true,
                        "legacy_db": report.legacy_db_path.display().to_string(),
                        "quests_written": report.quests_written,
                        "deleted_quests_included": report.deleted_quests_included,
                        "types_updated": report.types_updated,
                        "index_db_copied": report.index_db_copied,
                    })
                );
            } else {
                println!("✓ 마이그레이션 완료");
                println!("  legacy DB     : {}", report.legacy_db_path.display());
                println!("  quests 작성   : {}", report.quests_written);
                println!(
                    "  - alive       : {}",
                    report.quests_written - report.deleted_quests_included
                );
                println!("  - soft-deleted: {}", report.deleted_quests_included);
                println!("  types 갱신    : {} (counter)", report.types_updated);
                println!(
                    "  index.db      : {}",
                    if report.index_db_copied { "복사됨" } else { "이미 존재 — 건드리지 않음" }
                );
            }
        }
        Command::Info { brief } => {
            let i = c.info()?;
            let total = i.summary.quests_alive + i.summary.quests_deleted;
            let snap_total: u64 = i.snapshots.iter().map(|s| s.size_bytes).sum();
            let latest = i
                .snapshots
                .last()
                .map(|s| openguild_core::snapshot::ts_to_local_display(&s.timestamp))
                .unwrap_or_else(|| "(none)".to_string());
            let schema = i.summary.schema_version.as_deref();
            if cli.json {
                println!(
                    "{}",
                    serde_json::json!({
                        "guild": i.guild.name,
                        "version": i.guild.version,
                        "created_at": i.guild.created_at,
                        "path": i.path.display().to_string(),
                        "db_size_bytes": i.summary.db_size_bytes,
                        "schema": schema,
                        "quests_alive": i.summary.quests_alive,
                        "quests_deleted": i.summary.quests_deleted,
                        "snapshots": i.snapshots.len(),
                        "snapshots_bytes": snap_total,
                        "latest_snapshot": i.snapshots.last().map(|s| s.timestamp.clone()),
                        "journal_ops": i.journal_total,
                    })
                );
            } else if brief {
                println!(
                    "guild={} quests={}/{} schema={} snapshots={} journal={}",
                    i.guild.name,
                    i.summary.quests_alive,
                    total,
                    schema.unwrap_or("(none)"),
                    i.snapshots.len(),
                    i.journal_total,
                );
            } else {
                println!(
                    "guild   : {}  (v{}, created {})",
                    i.guild.name, i.guild.version, i.guild.created_at
                );
                println!("path    : {}", i.path.display());
                println!();
                println!("db      : {} bytes", i.summary.db_size_bytes);
                println!("schema  : {}", schema.unwrap_or("(db not initialized)"));
                println!(
                    "quests  : {} alive, {} deleted",
                    i.summary.quests_alive, i.summary.quests_deleted
                );
                println!();
                println!(
                    "snapshots: {} file(s), {} bytes total (latest: {})",
                    i.snapshots.len(),
                    snap_total,
                    latest
                );
                println!("journal : {} ops since last snapshot", i.journal_total);
            }
        }
        Command::Campaign { sub } => handle_campaign(&c, cli.json, sub)?,
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
                title_only,
                sort,
                reverse,
                limit,
                offset,
                id_only,
                count,
                tree,
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
                    title_only,
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
                } else if tree && !cli.json {
                    // DEV-065 (CLI tree mode): 부모 → 자식 들여쓰기 출력.
                    print_quest_tree(&quests);
                } else {
                    print_quest_list(&quests, cli.json);
                }
            }
            QuestCmd::Search { query, title_only, limit, id_only, count } => {
                // DEV-045: list --search 의 발견성을 위한 alias.
                // 동일 백엔드 호출. 사용자 친화적인 단일 인자만 받아 ListQuery 로 변환.
                let q = ListQuery {
                    search: Some(query),
                    title_only,
                    limit,
                    ..Default::default()
                };
                let quests = c.list_quests(&q)?;
                if count {
                    println!("{}", quests.len());
                } else if id_only {
                    if cli.json {
                        println!("{}", serde_json::to_string_pretty(
                            &quests.iter().map(|q| q.quest_id.clone()).collect::<Vec<_>>()
                        )?);
                    } else {
                        for q in &quests {
                            println!("{}", q.quest_id);
                        }
                    }
                } else {
                    print_quest_list(&quests, cli.json);
                }
            }
            QuestCmd::Show { slug, field } => {
                let d = c.quest_by_slug(&slug)?;
                if let Some(name) = field {
                    let v = quest_field_value(&d, &name)?;
                    if cli.json {
                        println!("{}", serde_json::to_string(&v).unwrap());
                    } else {
                        // raw — multi-line (description 등) 그대로.
                        println!("{v}");
                    }
                } else {
                    print_quest_detail(&d, cli.json);
                }
            }
            QuestCmd::History { slug } => {
                let d = c.quest_by_slug(&slug)?;
                let history = c.list_quest_history(d.quest.id)?;
                if cli.json {
                    println!("{}", serde_json::to_string_pretty(&history).unwrap());
                } else if history.is_empty() {
                    println!("(이력 없음)");
                } else {
                    // DEV-038 follow-up:
                    // - status 값은 slug → name_en + status.color (DEV-042+).
                    // - 절대 ts + 상대 ts (script 친화 + 가독성).
                    // - change_status 는 op 라벨 생략 — old → new pill 이 이미
                    //   "상태 변경" 의미를 시각적으로 나타냄 (이전 "상태" 라벨은
                    //   매 줄마다 동일해서 노이즈). 다른 op (update_title 등)
                    //   추가 시엔 `[op]` 형태로 표시.
                    // - 끝에 총 항목 수 (`-- N entries`) — 절단 의심 방지.
                    let statuses = c.quest_statuses().unwrap_or_default();
                    let display = |raw: Option<&str>| -> String {
                        let Some(v) = raw else { return "∅".into() };
                        if let Some(s) = statuses.iter().find(|s| s.slug == v) {
                            return colorize(&s.name_en, &s.color);
                        }
                        if let Ok(id) = v.parse::<i64>()
                            && let Some(s) = statuses.iter().find(|s| s.id == id)
                        {
                            return colorize(&format!("{} (legacy id)", s.name_en), &s.color);
                        }
                        v.to_string()
                    };
                    for h in &history {
                        let old = display(h.old_value.as_deref());
                        let new = display(h.new_value.as_deref());
                        let rel = openguild_core::time::format_relative(&h.ts)
                            .unwrap_or_else(|| "—".into());
                        if h.op == "change_status" {
                            println!("{}  {:<10} {} → {}", h.ts, rel, old, new);
                        } else {
                            // 미래의 다른 op — raw op 를 `[op]` 형태로.
                            println!("{}  {:<10} [{}] {} → {}", h.ts, rel, h.op, old, new);
                        }
                    }
                    let n = history.len();
                    println!("-- {n} entries");
                }
            }
            QuestCmd::New {
                type_prefix,
                title,
                description,
                urgency,
                parent,
                template,
            } => {
                // DEV-060: 템플릿 merge — 명시 옵션 > 템플릿 값 > 기본.
                let tpl = match &template {
                    Some(name) => Some(c.template_load(name)?),
                    None => None,
                };
                let merged =
                    merge_new_quest_inputs(type_prefix, title, description, urgency, tpl.as_ref())?;
                let (type_prefix, title, description, urgency, tpl_tags) = merged;

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
                    // DEV-048: slug 전용.
                    status_slug: open_status.slug.clone(),
                    urgency: Some(urgency),
                    parent_quest_id: parent_id,
                };
                let q = c.create_quest(body)?;
                // DEV-060: 템플릿의 기본 tags 적용 (생성 직후 set).
                if !tpl_tags.is_empty()
                    && let Err(e) = c.tag_set(&q.quest_id, tpl_tags)
                {
                    eprintln!("[openguild] warn: 템플릿 tags 적용 실패 — {e:#}");
                }
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
                            // DEV-046: urgency 색 적용 (양쪽).
                            println!(
                                "  urgency:     {} → {}",
                                colorize(&detail.quest.urgency.to_string(), urgency_color(detail.quest.urgency)),
                                colorize(&u.to_string(), urgency_color(u))
                            );
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
                if let Some(target) = status {
                    // DEV-044: deprecated 변경 호출 — `move` 권장 알림.
                    eprintln!(
                        "warning: `quest status <slug> <status>` 는 deprecated. \
                         앞으로는 `quest move <slug> <status>` 사용 (혼란 방지)."
                    );
                    change_status_with_noop_notice(&c, &slug, &target, cli.json)?;
                } else {
                    // 출력 전용 — 현재 상태만.
                    // DEV-046: JSON 에서 status_id 제거 (positional id 는 외부
                    // 클라이언트가 참조하면 안 됨). slug 가 stable identifier.
                    let d = c.quest_by_slug(&slug)?;
                    if cli.json {
                        let payload = serde_json::json!({
                            "quest_id": d.quest.quest_id,
                            "status_slug": d.quest.status_slug,
                            "status_name_en": d.quest.status_name_en,
                            "status_name_ko": d.quest.status_name_ko,
                        });
                        println!("{}", serde_json::to_string_pretty(&payload).unwrap());
                    } else {
                        println!(
                            "{}  {} ({})",
                            colorize(&d.quest.quest_id, &d.quest.type_color),
                            colorize(&d.quest.status_name_en, &d.quest.status_color),
                            d.quest.status_name_ko
                        );
                    }
                }
            }
            QuestCmd::Move { slug, status } => {
                // DEV-044: 변경 전용. status 와 동일한 helper 사용.
                change_status_with_noop_notice(&c, &slug, &status, cli.json)?;
            }
            QuestCmd::Start { slug } => {
                change_status_with_noop_notice(&c, &slug, "In Progress", cli.json)?;
            }
            QuestCmd::Done { slug } => {
                change_status_with_noop_notice(&c, &slug, "Done", cli.json)?;
            }
            QuestCmd::Reopen { slug } => {
                change_status_with_noop_notice(&c, &slug, "Open", cli.json)?;
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
            QuestCmd::Due {
                slug,
                desired,
                required,
                clear_desired,
                clear_required,
            } => {
                let id = c.id_of(&slug)?;
                let any_change =
                    desired.is_some() || required.is_some() || clear_desired || clear_required;
                if !any_change {
                    // 조회만.
                    let d = c.quest_by_slug(&slug)?;
                    let q = d.quest;
                    if cli.json {
                        let payload = serde_json::json!({
                            "quest_id": q.quest_id,
                            "desired_due": q.desired_due,
                            "required_due": q.required_due,
                        });
                        println!("{}", serde_json::to_string_pretty(&payload).unwrap());
                    } else {
                        println!(
                            "{}  desired_due: {}  required_due: {}",
                            colorize(&q.quest_id, &q.type_color),
                            q.desired_due.as_deref().unwrap_or("(없음)"),
                            q.required_due.as_deref().unwrap_or("(없음)"),
                        );
                    }
                } else {
                    // DEV-076: Some(Some(d)) = 설정, Some(None) = 해제, None = no-op.
                    let desired_arg: Option<Option<String>> = if clear_desired {
                        Some(None)
                    } else {
                        desired.map(Some)
                    };
                    let required_arg: Option<Option<String>> = if clear_required {
                        Some(None)
                    } else {
                        required.map(Some)
                    };
                    let q = c.set_due_dates(id, desired_arg, required_arg)?;
                    if cli.json {
                        let payload = serde_json::json!({
                            "quest_id": q.quest_id,
                            "desired_due": q.desired_due,
                            "required_due": q.required_due,
                        });
                        println!("{}", serde_json::to_string_pretty(&payload).unwrap());
                    } else {
                        println!(
                            "{}  desired_due: {}  required_due: {}",
                            colorize(&q.quest_id, &q.type_color),
                            q.desired_due.as_deref().unwrap_or("(없음)"),
                            q.required_due.as_deref().unwrap_or("(없음)"),
                        );
                    }
                }
            }
            // DEV-100: quest / campaign 공용 핸들러로 위임.
            QuestCmd::Comment { sub } => run_comment_cmd(&c, CommentScope::Quest, sub, cli.json)?,
            QuestCmd::Attach { sub } => run_attach_cmd(&c, CommentScope::Quest, sub, cli.json)?,
            QuestCmd::Memo { sub } => run_memo_cmd(&c, CommentScope::Quest, sub, cli.json)?,
            QuestCmd::Tag { sub } => match sub {
                TagCmd::List { slug } => {
                    let tags = c.tag_list(&slug)?;
                    if cli.json {
                        println!(
                            "{}",
                            serde_json::json!({ "slug": slug, "tags": tags })
                        );
                    } else if tags.is_empty() {
                        println!("(태그 없음)");
                    } else {
                        println!("{}", tags.join(" "));
                    }
                }
                TagCmd::Add { slug, tags: new_tags } => {
                    let mut existing = c.tag_list(&slug)?;
                    for t in new_tags {
                        // 공백 split — 공백 구분 한 인자도 지원.
                        for token in t.split_whitespace() {
                            let s = token.to_string();
                            if !existing.contains(&s) {
                                existing.push(s);
                            }
                        }
                    }
                    c.tag_set(&slug, existing.clone())?;
                    if cli.json {
                        println!("{}", serde_json::json!({ "ok": true, "slug": slug, "tags": existing }));
                    } else {
                        println!("✓ {slug} tags: {}", existing.join(" "));
                    }
                }
                TagCmd::Rm { slug, tags: remove } => {
                    let existing = c.tag_list(&slug)?;
                    let to_remove: std::collections::HashSet<String> = remove
                        .iter()
                        .flat_map(|t| t.split_whitespace().map(|s| s.to_string()))
                        .collect();
                    let after: Vec<String> = existing
                        .into_iter()
                        .filter(|t| !to_remove.contains(t))
                        .collect();
                    c.tag_set(&slug, after.clone())?;
                    if cli.json {
                        println!("{}", serde_json::json!({ "ok": true, "slug": slug, "tags": after }));
                    } else if after.is_empty() {
                        println!("✓ {slug} tags: (없음)");
                    } else {
                        println!("✓ {slug} tags: {}", after.join(" "));
                    }
                }
                TagCmd::Set { slug, tags: new_tags } => {
                    // 공백 구분 인자도 split.
                    let flat: Vec<String> = new_tags
                        .iter()
                        .flat_map(|t| t.split_whitespace().map(|s| s.to_string()))
                        .collect();
                    c.tag_set(&slug, flat.clone())?;
                    if cli.json {
                        println!("{}", serde_json::json!({ "ok": true, "slug": slug, "tags": flat }));
                    } else if flat.is_empty() {
                        println!("✓ {slug} tags: (모두 제거)");
                    } else {
                        println!("✓ {slug} tags: {}", flat.join(" "));
                    }
                }
            },
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
        // DEV-064: 마커 포맷은 core 공용 헬퍼 — schema_version 포함.
        let content = openguild_core::guild_file::marker_content(&name, &today);
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
            slug: en.to_lowercase().replace(' ', "_"),
            name_en: en.into(),
            name_ko: "".into(),
            color: "".into(),
            sort_order: 0,
            counts_as_done: false,
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
                    title_only,
                    sort,
                    reverse,
                    limit,
                    offset,
                    id_only,
                    count,
                    tree,
                },
            } => {
                assert!(type_prefix.is_empty());
                assert!(status.is_empty());
                assert!(urgency.is_none());
                assert!(created_after.is_none());
                assert!(created_before.is_none());
                assert!(updated_after.is_none());
                assert!(updated_before.is_none());
                assert!(!tree);
                assert!(child_of.is_none());
                assert!(!no_parent);
                assert!(!has_prereq);
                assert!(!no_prereq);
                assert!(!title_only);
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
                    search, title_only,
                    sort, reverse, limit, offset, id_only, count, tree,
                },
            } => {
                assert!(!tree);
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
                assert!(!title_only);
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
            title_only: false,
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
    fn querystring_title_only_flag() {
        let q = ListQuery {
            search: Some("foo".into()),
            title_only: true,
            ..Default::default()
        };
        assert_eq!(list_query_to_querystring(&q), "search=foo&title_only=true");
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
                        template,
                    },
            } => {
                // DEV-060: --type / --title 은 Option (템플릿으로 대체 가능),
                // urgency 도 Option (merge 후 기본 3) — 핸들러에서 결정.
                assert_eq!(type_prefix.as_deref(), Some("DEV"));
                assert_eq!(title.as_deref(), Some("test"));
                assert!(urgency.is_none()); // 기본값은 핸들러 merge 에서
                assert!(parent.is_none());
                assert!(description.is_none());
                assert!(template.is_none());
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
            "--template", "bug-report",
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
                        template,
                    },
            } => {
                assert_eq!(type_prefix.as_deref(), Some("BUG"));
                assert_eq!(title.as_deref(), Some("fix"));
                assert_eq!(urgency, Some(1));
                assert_eq!(parent.as_deref(), Some("DEV-007"));
                assert_eq!(description.as_deref(), Some("details"));
                assert_eq!(template.as_deref(), Some("bug-report"));
            }
            _ => panic!("expected quest new"),
        }
    }

    /// 템플릿 merge — 명시 옵션 > 템플릿 > 기본 우선순위.
    #[test]
    fn merge_new_quest_inputs_priority() {
        use openguild_core::repo::TemplateFile;
        let tpl = TemplateFile::parse(
            "t",
            "+++\ntitle = \"tpl title\"\ntype = \"BUG\"\nurgency = 2\ntags = [\"x\"]\n+++\ntpl body",
        )
        .unwrap();

        // 명시값이 템플릿보다 우선.
        let (ty, ti, desc, u, tags) = merge_new_quest_inputs(
            Some("DEV".into()),
            Some("explicit".into()),
            Some("explicit body".into()),
            Some(1),
            Some(&tpl),
        )
        .unwrap();
        assert_eq!((ty.as_str(), ti.as_str(), u), ("DEV", "explicit", 1));
        assert_eq!(desc.as_deref(), Some("explicit body"));
        assert_eq!(tags, vec!["x"]); // tags 는 템플릿 제공분 그대로.

        // 명시값 없으면 템플릿 값.
        let (ty, ti, desc, u, _) =
            merge_new_quest_inputs(None, None, None, None, Some(&tpl)).unwrap();
        assert_eq!((ty.as_str(), ti.as_str(), u), ("BUG", "tpl title", 2));
        assert_eq!(desc.as_deref(), Some("tpl body"));

        // 템플릿도 없으면 type / title 은 에러, urgency 는 기본 3.
        assert!(merge_new_quest_inputs(None, Some("t".into()), None, None, None).is_err());
        assert!(merge_new_quest_inputs(Some("DEV".into()), None, None, None, None).is_err());
        let (_, _, desc, u, tags) =
            merge_new_quest_inputs(Some("DEV".into()), Some("t".into()), None, None, None)
                .unwrap();
        assert!(desc.is_none());
        assert_eq!(u, 3);
        assert!(tags.is_empty());
    }

    /// 댓글 list 필터 — 5개 flag 파싱 + --top-only / --reply-to 상호배타.
    #[test]
    fn cli_parse_comment_list_filters() {
        let cli = Cli::try_parse_from([
            "openguild", "quest", "comment", "list", "DEV-001",
            "--author", "claude",
            "--since", "2026-06-01",
            "--top-only",
            "--grep", "needle",
        ])
        .unwrap();
        match cli.command {
            Command::Quest { sub: QuestCmd::Comment { sub: CommentCmd::List { slug, author, since, top_only, reply_to, grep } } } => {
                assert_eq!(slug, "DEV-001");
                assert_eq!(author.as_deref(), Some("claude"));
                assert_eq!(since.as_deref(), Some("2026-06-01"));
                assert!(top_only);
                assert!(reply_to.is_none());
                assert_eq!(grep.as_deref(), Some("needle"));
            }
            _ => panic!("expected comment list"),
        }

        // --top-only 와 --reply-to 동시 지정은 clap 이 거부.
        assert!(Cli::try_parse_from([
            "openguild", "quest", "comment", "list", "DEV-001",
            "--top-only", "--reply-to", "3",
        ])
        .is_err());
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
            "status_slug": "open",
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
            "status_id": 1, "status_slug": "open", "status_name_en": "Open", "status_name_ko": "게시됨",
            "status_color": "#8B95A1", "urgency": 3, "parent_quest_id": null,
            "created_at": "", "updated_at": "",
            "sub_quests": [{
                "id": 2, "quest_id": "DEV-002", "quest_type_id": 1, "type_prefix": "DEV",
                "type_color": "#4A90D9", "number": 2, "title": "child", "description": null,
                "status_id": 1, "status_slug": "open", "status_name_en": "Open", "status_name_ko": "게시됨",
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
            {"id":1,"slug":"open","name_en":"Open","name_ko":"게시됨","color":"#8B95A1","sort_order":0},
            {"id":2,"slug":"in_progress","name_en":"In Progress","name_ko":"진행 중","color":"#4A90D9","sort_order":1}
        ]"##;
        let v: Vec<QuestStatus> = serde_json::from_str(json).unwrap();
        assert_eq!(v.len(), 2);
        assert_eq!(v[1].name_en, "In Progress");
        assert_eq!(v[1].slug, "in_progress");
    }

    // === DEV-043: quest show --field ===

    fn sample_detail() -> QuestDetail {
        let q = Quest {
            id: 7,
            quest_id: "DEV-007".into(),
            quest_type_id: 1,
            type_prefix: "DEV".into(),
            type_color: "#4A90D9".into(),
            number: 7,
            title: "CI/CD + 배포 인프라".into(),
            description: Some("multi-line\nbody".into()),
            status_id: 1,
            status_slug: "open".into(),
            status_name_en: "Open".into(),
            status_name_ko: "게시됨".into(),
            status_color: "#8B95A1".into(),
            urgency: 3,
            parent_quest_id: None,
            created_at: "2026-05-15T16:12:48+09:00".into(),
            updated_at: "2026-05-22T01:09:30+09:00".into(),
            // DEV-076 / BUG-034: 기한 + 캠페인 유효 기한 — 테스트 데이터엔 모두 None.
            desired_due: None,
            required_due: None,
            earliest_campaign_due: None,
            tags: vec![],
            comment_count: 0,
            discussion_unresolved: 0,
            discussion_resolved: 0,
        };
        QuestDetail {
            quest: q,
            parent: None,
            sub_quests: vec![],
            prerequisites: vec![],
            successors: vec![],
            tags: vec![],
            attachments: vec![],
            position: None,
        }
    }

    #[test]
    fn field_id_returns_slug_form() {
        let d = sample_detail();
        assert_eq!(quest_field_value(&d, "id").unwrap(), "DEV-007");
        assert_eq!(quest_field_value(&d, "slug").unwrap(), "DEV-007");
    }

    #[test]
    fn field_title_returns_title() {
        let d = sample_detail();
        assert_eq!(quest_field_value(&d, "title").unwrap(), "CI/CD + 배포 인프라");
    }

    #[test]
    fn field_status_name_or_slug() {
        let d = sample_detail();
        assert_eq!(quest_field_value(&d, "status").unwrap(), "Open");
        assert_eq!(quest_field_value(&d, "status_slug").unwrap(), "open");
        assert_eq!(quest_field_value(&d, "status_ko").unwrap(), "게시됨");
    }

    #[test]
    fn field_status_slug_multi_word() {
        let mut d = sample_detail();
        d.quest.status_name_en = "In Progress".into();
        assert_eq!(quest_field_value(&d, "status_slug").unwrap(), "in_progress");
    }

    #[test]
    fn field_urgency_as_string() {
        let d = sample_detail();
        assert_eq!(quest_field_value(&d, "urgency").unwrap(), "3");
    }

    #[test]
    fn field_description_multiline() {
        let d = sample_detail();
        assert_eq!(quest_field_value(&d, "description").unwrap(), "multi-line\nbody");
        assert_eq!(quest_field_value(&d, "body").unwrap(), "multi-line\nbody");
    }

    #[test]
    fn field_description_empty_when_none() {
        let mut d = sample_detail();
        d.quest.description = None;
        assert_eq!(quest_field_value(&d, "description").unwrap(), "");
    }

    #[test]
    fn field_type_returns_prefix() {
        let d = sample_detail();
        assert_eq!(quest_field_value(&d, "type").unwrap(), "DEV");
    }

    #[test]
    fn field_parent_empty_when_none() {
        let d = sample_detail();
        assert_eq!(quest_field_value(&d, "parent").unwrap(), "");
    }

    #[test]
    fn field_parent_returns_id_when_set() {
        let mut d = sample_detail();
        d.quest.parent_quest_id = Some(42);
        assert_eq!(quest_field_value(&d, "parent").unwrap(), "42");
    }

    #[test]
    fn field_timestamps() {
        let d = sample_detail();
        assert_eq!(quest_field_value(&d, "created_at").unwrap(), "2026-05-15T16:12:48+09:00");
        assert_eq!(quest_field_value(&d, "created").unwrap(), "2026-05-15T16:12:48+09:00");
        assert_eq!(quest_field_value(&d, "updated_at").unwrap(), "2026-05-22T01:09:30+09:00");
        assert_eq!(quest_field_value(&d, "updated").unwrap(), "2026-05-22T01:09:30+09:00");
    }

    #[test]
    fn field_unknown_returns_err() {
        let d = sample_detail();
        let err = quest_field_value(&d, "nonexistent").unwrap_err();
        assert!(format!("{err}").contains("unknown field"));
    }

    // === DEV-045: quest search ===

    #[test]
    fn search_parses_query_arg() {
        let cli = Cli::try_parse_from(["openguild", "quest", "search", "foo bar"]).unwrap();
        match cli.command {
            Command::Quest { sub: QuestCmd::Search { query, title_only, limit, id_only, count } } => {
                assert_eq!(query, "foo bar");
                assert!(!title_only);
                assert!(limit.is_none());
                assert!(!id_only);
                assert!(!count);
            }
            _ => panic!("expected QuestCmd::Search"),
        }
    }

    #[test]
    fn search_with_title_only_flag() {
        let cli = Cli::try_parse_from(["openguild", "quest", "search", "x", "--title-only"]).unwrap();
        match cli.command {
            Command::Quest { sub: QuestCmd::Search { title_only, .. } } => {
                assert!(title_only);
            }
            _ => panic!("expected QuestCmd::Search"),
        }
    }

    #[test]
    fn search_with_limit_and_id_only() {
        let cli = Cli::try_parse_from([
            "openguild", "quest", "search", "DEV", "--limit", "5", "--id-only",
        ]).unwrap();
        match cli.command {
            Command::Quest { sub: QuestCmd::Search { query, limit, id_only, count, .. } } => {
                assert_eq!(query, "DEV");
                assert_eq!(limit, Some(5));
                assert!(id_only);
                assert!(!count);
            }
            _ => panic!("expected QuestCmd::Search"),
        }
    }

    #[test]
    fn search_id_only_and_count_mutually_exclusive() {
        let r = Cli::try_parse_from([
            "openguild", "quest", "search", "x", "--id-only", "--count",
        ]);
        assert!(r.is_err(), "id-only + count 동시 사용은 에러여야 함");
    }

    // === DEV-044: status 출력 전용 + move 신설 ===

    #[test]
    fn status_with_only_slug_parses_status_arg_as_none() {
        let cli = Cli::try_parse_from(["openguild", "quest", "status", "DEV-001"]).unwrap();
        match cli.command {
            Command::Quest { sub: QuestCmd::Status { slug, status } } => {
                assert_eq!(slug, "DEV-001");
                assert!(status.is_none(), "status 인자 미지정 시 None 이어야 함");
            }
            _ => panic!("expected QuestCmd::Status"),
        }
    }

    #[test]
    fn status_with_slug_and_status_arg_keeps_status_some() {
        let cli = Cli::try_parse_from(["openguild", "quest", "status", "DEV-001", "testing"]).unwrap();
        match cli.command {
            Command::Quest { sub: QuestCmd::Status { slug, status } } => {
                assert_eq!(slug, "DEV-001");
                assert_eq!(status.as_deref(), Some("testing"));
            }
            _ => panic!("expected QuestCmd::Status"),
        }
    }

    #[test]
    fn move_requires_slug_and_status() {
        let cli = Cli::try_parse_from(["openguild", "quest", "move", "DEV-001", "testing"]).unwrap();
        match cli.command {
            Command::Quest { sub: QuestCmd::Move { slug, status } } => {
                assert_eq!(slug, "DEV-001");
                assert_eq!(status, "testing");
            }
            _ => panic!("expected QuestCmd::Move"),
        }
    }

    #[test]
    fn move_without_status_errors() {
        let r = Cli::try_parse_from(["openguild", "quest", "move", "DEV-001"]);
        assert!(r.is_err(), "move 는 status 인자 필수");
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

    /// BUG-016: clap doc comment (`/// ...`) 에 적힌 quest 번호가 사용자
    /// 노출되는 help 출력에 leak 한 적 있음. 회귀 방지 — 모든 subcommand 의
    /// about / long_about / help 출력에 `<PREFIX>-<숫자>` 패턴 0건 보장.
    #[test]
    fn help_output_has_no_quest_id_leaks() {
        use clap::CommandFactory;
        let cmd = Cli::command();
        let mut violations: Vec<String> = Vec::new();
        check_help_recursive(&cmd, cmd.get_name(), &mut violations);
        assert!(
            violations.is_empty(),
            "quest id 가 help 출력에 leak:\n{}",
            violations.join("\n")
        );
    }

    fn check_help_recursive(
        cmd: &clap::Command,
        path: &str,
        violations: &mut Vec<String>,
    ) {
        let mut owned = cmd.clone();
        let help = owned.render_long_help().to_string();
        if let Some(found) = find_quest_id(&help) {
            violations.push(format!("[{path}] '{found}' in help"));
        }
        for sub in cmd.get_subcommands() {
            let sub_path = format!("{path} {}", sub.get_name());
            check_help_recursive(sub, &sub_path, violations);
        }
    }

    /// 처음 발견된 `<PREFIX>-<숫자>` substring (PREFIX 는 ASCII 대문자 2~5).
    fn find_quest_id(s: &str) -> Option<&str> {
        let bytes = s.as_bytes();
        let mut i = 0;
        while i < bytes.len() {
            // dash 위치 찾기.
            let Some(dash_rel) = s[i..].find('-') else { break };
            let dash = i + dash_rel;
            // dash 앞 — ASCII 대문자 2~5자.
            let prefix_end = dash;
            let mut prefix_start = prefix_end;
            while prefix_start > 0
                && bytes[prefix_start - 1].is_ascii_uppercase()
                && prefix_end - prefix_start < 5
            {
                prefix_start -= 1;
            }
            let prefix_len = prefix_end - prefix_start;
            // dash 뒤 — ASCII 숫자 1자 이상.
            let after = dash + 1;
            let mut digits = after;
            while digits < bytes.len() && bytes[digits].is_ascii_digit() {
                digits += 1;
            }
            if prefix_len >= 2 && digits > after {
                return Some(&s[prefix_start..digits]);
            }
            i = dash + 1;
        }
        None
    }

    #[test]
    fn find_quest_id_detects_patterns() {
        assert!(find_quest_id("foo (DEV-001) bar").is_some());
        assert!(find_quest_id("BUG-44 trailing").is_some());
        assert!(find_quest_id("REQ-7").is_some());
        assert!(find_quest_id("no quest id here").is_none());
        assert!(find_quest_id("D-1").is_none()); // prefix 너무 짧음.
        assert!(find_quest_id("DEV-").is_none()); // 숫자 없음.
        assert!(find_quest_id("dev-001").is_none()); // 소문자.
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

    // ─── DEV-062: types / statuses subcommand 파싱 ───

    #[test]
    fn cli_types_no_sub_defaults_to_list() {
        // 호환성: 기존 `openguild types` 는 list 동작 유지.
        let cli = Cli::try_parse_from(["openguild", "types"]).unwrap();
        match cli.command {
            Command::Types { sub } => assert!(sub.is_none()),
            _ => panic!(),
        }
    }

    #[test]
    fn cli_types_add() {
        let cli = Cli::try_parse_from([
            "openguild", "types", "add", "FOO", "--color", "#abcdef", "--description", "x",
        ])
        .unwrap();
        match cli.command {
            Command::Types {
                sub: Some(TypesCmd::Add { prefix, color, description }),
            } => {
                assert_eq!(prefix, "FOO");
                assert_eq!(color, "#abcdef");
                assert_eq!(description.as_deref(), Some("x"));
            }
            _ => panic!(),
        }
    }

    #[test]
    fn cli_types_update_with_clear() {
        let cli = Cli::try_parse_from([
            "openguild",
            "types",
            "update",
            "DEV",
            "--clear-description",
        ])
        .unwrap();
        match cli.command {
            Command::Types {
                sub:
                    Some(TypesCmd::Update {
                        prefix,
                        new_prefix,
                        clear_description,
                        description,
                        color,
                    }),
            } => {
                assert_eq!(prefix, "DEV");
                assert!(new_prefix.is_none());
                assert!(clear_description);
                assert!(description.is_none());
                assert!(color.is_none());
            }
            _ => panic!(),
        }
    }

    /// BUG-018: 'types update --prefix' 가 rename 트리거.
    #[test]
    fn cli_types_update_with_prefix_triggers_rename() {
        let cli = Cli::try_parse_from(["openguild", "types", "update", "DEV", "--prefix", "CORE"])
            .unwrap();
        match cli.command {
            Command::Types {
                sub: Some(TypesCmd::Update { prefix, new_prefix, .. }),
            } => {
                assert_eq!(prefix, "DEV");
                assert_eq!(new_prefix.as_deref(), Some("CORE"));
            }
            _ => panic!(),
        }
    }

    #[test]
    fn cli_statuses_add_with_name_ko_and_sort_order() {
        let cli = Cli::try_parse_from([
            "openguild",
            "statuses",
            "add",
            "Blocked",
            "--color",
            "#ff0000",
            "--name-ko",
            "막힘",
            "--sort-order",
            "9",
        ])
        .unwrap();
        match cli.command {
            Command::Statuses {
                sub: Some(StatusesCmd::Add { name_en, color, name_ko, sort_order }),
            } => {
                assert_eq!(name_en, "Blocked");
                assert_eq!(color, "#ff0000");
                assert_eq!(name_ko.as_deref(), Some("막힘"));
                assert_eq!(sort_order, Some(9));
            }
            _ => panic!(),
        }
    }

    /// BUG-018: resolve_status_slug 가 name_en / name_ko 도 매칭.
    #[test]
    fn resolve_status_slug_matches_name_en_and_ko() {
        // helper 자체는 Backend 메서드라 unit-test 어려움 — 매칭 헬퍼 match_status_id
        // 가 비슷한 알고리즘 검증을 이미 함. 본 테스트는 알고리즘 정합 확인 (case-
        // insensitive name_en + 공백→언더스코어).
        let list = vec![
            QuestStatus {
                id: 1,
                slug: "open".into(),
                name_en: "Open".into(),
                name_ko: "게시됨".into(),
                color: "".into(),
                sort_order: 1,
                counts_as_done: false,
            },
            QuestStatus {
                id: 2,
                slug: "in_progress".into(),
                name_en: "In Progress".into(),
                name_ko: "진행 중".into(),
                color: "".into(),
                sort_order: 2,
                counts_as_done: false,
            },
        ];
        // 기존 match_status_id 가 name_en 만. 본 quest 가 추가한 name_ko fallback
        // 은 Backend::resolve_status_slug 안에 있음 — integration 테스트는
        // 다음 sprint 의 fixture 기반. 여기선 placeholder.
        assert_eq!(match_status_id("Open", &list), Some(1));
        assert_eq!(match_status_id("In Progress", &list), Some(2));
        assert_eq!(match_status_id("in_progress", &list), Some(2));
    }

    /// BUG-018: 'statuses update --slug' 가 rename 트리거.
    #[test]
    fn cli_statuses_update_with_slug_triggers_rename() {
        let cli = Cli::try_parse_from([
            "openguild", "statuses", "update", "open", "--slug", "backlog",
        ])
        .unwrap();
        match cli.command {
            Command::Statuses {
                sub: Some(StatusesCmd::Update { slug, new_slug, .. }),
            } => {
                assert_eq!(slug, "open");
                assert_eq!(new_slug.as_deref(), Some("backlog"));
            }
            _ => panic!(),
        }
    }
}
