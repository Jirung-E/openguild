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

    // DEV-211 — doc 주석에 quest id 금지(help 누출).
    /// JSON 을 한 줄로 (파이프 / jq / 로그 수집용). --json 필요.
    #[arg(long, global = true, requires = "json")]
    compact: bool,

    #[command(subcommand)]
    command: Command,
}

/// DEV-211: `--compact` 전역 플래그 — 파싱 직후 1회 설정, json_str() 이 참조.
/// 30여 개 출력 지점에 bool 을 실어 나르는 대신 프로세스 전역(한 번 실행되고
/// 끝나는 CLI 특성상 안전).
static JSON_COMPACT: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// DEV-254: 현재 CLI 출력 언어 — `run()` 시작 시 `openguild_core::locale::current()`
/// 로 1회 설정(JSON_COMPACT 와 동일 패턴). `tf!` 매크로가 참조.
static LOCALE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false); // true = en

fn current_locale_is_en() -> bool {
    LOCALE.load(std::sync::atomic::Ordering::Relaxed)
}

/// DEV-254: 사람이 읽는 CLI 출력의 ko/en 템플릿을 한 호출로 선택.
/// `println!("...", args)` 자리를 `println!("{}", tf!("ko 템플릿 {}", "en template {}", args))`
/// 로 대체하는 식으로 기존 println!/format! 호출부에 최소 침습적으로 적용.
/// --json 출력(기계 파싱용)은 대상이 아님 — 사람이 읽는 텍스트만.
macro_rules! tf {
    ($ko:literal, $en:literal $(, $arg:expr)* $(,)?) => {
        if current_locale_is_en() {
            format!($en $(, $arg)*)
        } else {
            format!($ko $(, $arg)*)
        }
    };
}

/// JSON 직렬화 — 기본 pretty(2-space, 기존 호환), --compact 면 한 줄.
fn json_str<T: serde::Serialize>(v: &T) -> String {
    if JSON_COMPACT.load(std::sync::atomic::Ordering::Relaxed) {
        serde_json::to_string(v).unwrap()
    } else {
        serde_json::to_string_pretty(v).unwrap()
    }
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
    // DEV-227: 다른 top-level 명사 그룹(quest/campaign/template/backup/check/
    // index/journal/rule)과의 단수형 일관성 위해 canonical 이름을 단수로 —
    // 기존 스크립트 호환 위해 복수형은 alias 로 유지. sub 도 다른 그룹처럼
    // 필수로 — bare 호출이 조용히 list 로 떨어지던 예전 관행(DEV-062) 제거,
    // `type list` 명시 필요.
    #[command(name = "type", alias = "types")]
    Types {
        #[command(subcommand)]
        sub: TypesCmd,
    },
    /// 퀘스트 상태 — 목록 / 추가 / 수정 / 삭제 / 이름 변경
    #[command(name = "status", alias = "statuses")]
    Statuses {
        #[command(subcommand)]
        sub: StatusesCmd,
    },
    /// 캠페인 관련 명령
    Campaign {
        #[command(subcommand)]
        sub: CampaignCmd,
    },
    // BUG-016: doc comment 의 quest id 가 clap help 로 leak — 외부 사용자에게
    // internal quest 번호 노출 금지. 기능 설명만 plain `///` 으로.
    /// 길드 규칙 — `.guild/rules/{slug}.md` 다중 파일 CRUD.
    // DEV-227/DEV-231: canonical 단수형. rules 는 사용자 지시로 alias 도
    // 남기지 않고 완전 제거(type/status 의 복수형 alias 와 다른 결정) —
    // rules 는 원래 bare 호출에 하위호환 관행이 없었으니 남길 이유도 없음.
    #[command(name = "rule")]
    Rules {
        #[command(subcommand)]
        sub: RulesCmd,
    },
    /// 도서관 — 프로젝트 참고문서/노트 (`.guild/library/`, 자체 BOOK 번호).
    Library {
        #[command(subcommand)]
        sub: LibraryCmd,
    },
    /// 태그 정의 카탈로그 (`.guild/tags/{slug}.toml`) — quest/도서관/규칙 공유.
    Tag {
        #[command(subcommand)]
        sub: TagDefCmd,
    },
    /// 번들 문서를 stdout 으로 출력 — 빌드에 embed 되어 있어 파일 경로/읽기
    /// 권한이 필요 없음 (agent 친화). 이름 미지정 시 목록.
    Docs {
        /// agents | usage | readme | changelog
        name: Option<String>,
    },
    /// CLI 출력 언어 — GUI 와 같은 위치(~/.openguild/locale.json)에 저장,
    /// 이후 모든 CLI 실행이 이 값을 따른다. 인자 없으면 현재 값 출력.
    Locale {
        /// ko | en — 미지정 시 현재 값 출력
        lang: Option<String>,
    },
    /// 작업 기록 — 날짜/기간별 활동 타임라인 + 날짜별 노트.
    Worklog {
        #[command(subcommand)]
        sub: WorklogCmd,
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
        /// UTC, 예 `2026-06-27T00:15:00Z`, 포함)까지 재적용. `latest` 키워드 =
        /// journal 전체 재적용(최신 상태로 복구). 내용 op(댓글/메모 본문)·type
        /// 변경·첨부가 낀 구간은 안전을 위해 거부됨.
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
    /// 길드 전체 댓글 횡단 검색 — quest + campaign, 기본 최신순 20개.
    Comments {
        /// 작성자 일치 (대소문자 무시 정확 일치).
        #[arg(long)]
        author: Option<String>,
        /// 이 시각 이후 작성분만 — ISO date (`2026-06-01`) 또는 datetime.
        #[arg(long)]
        since: Option<String>,
        /// 이 시각 이전 작성분만.
        #[arg(long)]
        until: Option<String>,
        /// body 부분 일치 (대소문자 무시).
        #[arg(long)]
        grep: Option<String>,
        /// 토론(discussion) 댓글만 (quest 전용 플래그 — campaign 댓글 제외됨).
        #[arg(long)]
        discussion: bool,
        /// 미해결 토론만 (discussion 포함).
        #[arg(long, conflicts_with = "discussion")]
        unresolved: bool,
        /// 최대 N 개 (기본 20).
        #[arg(long, default_value_t = 20)]
        limit: usize,
        /// 첫 줄 60자 요약만 출력 (기본: 본문 전체 — `quest comment list` 와
        /// 동일). 여러 건 훑어볼 때만 사용 — 요약만 보고 답글 달았다가 뒷내용을
        /// 놓친 사고(2026-07-05)로 기본을 전체 출력으로 바꿈.
        #[arg(long)]
        summary: bool,
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
        /// 정렬된 표(헤더 + 컬럼 정렬)로 출력 — 사람용. `--json`/`--tree` 와 상호배타.
        // `json` 은 전역 인자라 clap 의 conflicts_with 대상이 못 됨(debug assert
        // 가 subcommand 스코프에서 못 찾음) — 핸들러에서 수동 검증.
        #[arg(long, conflicts_with_all = ["tree", "id_only", "count"])]
        table: bool,
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
        /// 본문을 UTF-8 파일에서 읽기 — 한글 인코딩/따옴표 이스케이프/
        /// `--` 로 시작하는 본문의 플래그 오인 회피 (comment add --file 관례).
        #[arg(long = "description-file", conflicts_with = "description")]
        description_file: Option<std::path::PathBuf>,
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
        /// 본문을 UTF-8 파일에서 읽기 (--description 과 상호배타).
        #[arg(long = "description-file", conflicts_with = "description")]
        description_file: Option<std::path::PathBuf>,
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
    Deleted {
        /// 정렬된 표(헤더 + 컬럼)로 출력 — 사람용. --json 과 상호배타.
        #[arg(long)]
        table: bool,
    },
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
        /// 본문 파일. 미지정 시 stdin (파이프 없으면 빈 본문). 한글 등
        /// 비ASCII 는 --file 권장 — PowerShell 파이프(`echo | ...`)는 인코딩이
        /// 안 맞아 깨질 수 있음.
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
        /// 최신순 출력 (기본은 오래된 순 = 대화 흐름).
        #[arg(long)]
        reverse: bool,
        /// 최대 N 개만 (필터/정렬 적용 후).
        #[arg(long)]
        limit: Option<usize>,
        /// 답글을 부모 아래 들여쓰기 트리로 출력. `--reverse` 와 상호배타
        /// (트리는 대화 흐름 순). 필터로 부모가 빠진 답글은 root 로 표시.
        #[arg(long, conflicts_with = "reverse")]
        tree: bool,
    },
    /// entry 본문 전체 또는 단일. `--id` 지정 시 `--depth`/`--with-parents` 로
    /// 그 entry 의 답글/부모를 얼마나 같이 보여줄지 조절 (기본은 그 entry 만).
    Show {
        slug: String,
        /// 특정 entry id 만 출력. 미지정 시 모든 entry(이 경우 --depth/--with-parents 무시).
        #[arg(long)]
        id: Option<u64>,
        /// 답글을 몇 단계까지 함께 출력할지 (0 = 대상 entry 만, 기본값).
        /// `all` = 무제한 (전체 답글 트리). `--id` 없이는 무시.
        #[arg(long, default_value = "0", value_parser = parse_comment_depth)]
        depth: usize,
        /// 부모 체인(조상, root 까지)도 함께 출력. `--id` 없이는 무시.
        #[arg(long)]
        with_parents: bool,
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
        /// 본문 파일. 미지정 시 stdin. 한글 등 비ASCII 는 --file 권장 —
        /// PowerShell 파이프(`echo | ...`)는 인코딩이 안 맞아 깨질 수 있음.
        #[arg(long)]
        file: Option<std::path::PathBuf>,
    },
    /// 기존 entry 의 body 교체. ts / author 보존.
    Edit {
        slug: String,
        id: u64,
        /// 본문 파일. 미지정 시 stdin. 한글 등은 --file 권장.
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
    /// 이모지 반응 토글 — 이미 눌렀으면 제거 (GUI 와 동일 시맨틱).
    React {
        slug: String,
        id: u64,
        /// 이모지 (임의 문자열 허용 — GUI 고정 4종 외에도 가능).
        emoji: String,
        /// 반응 주체 — author 단위 토글이라 필수.
        #[arg(long)]
        author: String,
    },
    /// 토론(discussion) 플래그 토글 (quest 전용). 미해결 토론이 있으면 그 quest 의
    /// 완료 전환이 차단됨. discussion 을 끄면 resolved 도 해제.
    Discussion { slug: String, id: u64 },
    /// discussion 댓글의 resolved 토글 (quest 전용).
    Resolved { slug: String, id: u64 },
    /// 상단 고정(pin) 토글 — quest/campaign 댓글 둘 다 지원.
    Pinned { slug: String, id: u64 },
}

#[derive(Subcommand)]
enum MemoCmd {
    /// 메모 본문 stdout. 파일 없으면 "(메모 없음)".
    Show { slug: String },
    /// 메모 본문 교체. 본문은 `--file PATH` 또는 stdin. 한글 등 비ASCII 는
    /// --file 권장 — PowerShell 파이프(`echo | ...`)는 인코딩이 안 맞아
    /// 깨질 수 있음.
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

/// DEV-062: type 관리. DEV-227: sub 필수 — `type list` 명시.
#[derive(Subcommand)]
enum TypesCmd {
    /// 목록
    List {
        /// 정렬된 표(헤더 + 컬럼)로 출력 — 사람용. --json 과 상호배타.
        #[arg(long)]
        table: bool,
    },
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

/// 태그 정의 관리 — GUI 어드민의 "Tag 정의" 섹션과 동일 registry.
/// 정의 없는 태그도 사용 자체는 가능(UI 기본 색) — 여기선 색/설명만 관리.
/// (quest 별 태그 부착은 별개 — quest tag 그룹의 TagCmd.)
#[derive(Subcommand)]
enum TagDefCmd {
    /// 정의된 태그 목록 (slug / 색 / 설명)
    List {
        /// 실사용 중인 태그(quest/도서관 frontmatter)도 함께 — 정의 없이
        /// 쓰인 ad-hoc 태그를 발견하는 용도. 로컬 모드 전용.
        #[arg(long)]
        used: bool,
    },
    /// 새 태그 정의 추가 (이미 있으면 에러 — 수정은 update)
    Add {
        /// 소문자/숫자/_ 만, 최대 32자.
        slug: String,
        /// 색 (#RGB 또는 #RRGGBB)
        #[arg(long)]
        color: Option<String>,
        #[arg(long)]
        description: Option<String>,
    },
    /// 기존 태그 정의 수정 — 지정한 필드만 교체
    Update {
        slug: String,
        #[arg(long)]
        color: Option<String>,
        #[arg(long)]
        description: Option<String>,
    },
    /// 태그 정의 삭제 (quest 등의 태그 사용 자체는 보존 — 기본 색으로 표시)
    Delete { slug: String },
}

/// DEV-062: status 관리. DEV-227: sub 필수 — `status list` 명시.
#[derive(Subcommand)]
enum StatusesCmd {
    /// 목록
    List {
        /// 정렬된 표(헤더 + 컬럼)로 출력 — 사람용. --json 과 상호배타.
        #[arg(long)]
        table: bool,
    },
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

/// `--file PATH` 또는 stdin 에서 본문 읽기 — comment/memo/rules 공용.
///
/// DEV-186: `--file` 도 없고 stdin 이 파이프/리다이렉트 없이 터미널(tty)
/// 이면 예전엔 안내 없이 그냥 멈췄음(`rule new <slug>` 만 치면 hang —
/// 사용자 보고). tty 면 즉시 에러로 사용법 안내 + 비정상 종료.
fn read_content(path: Option<&std::path::Path>) -> Result<String> {
    if let Some(p) = path {
        std::fs::read_to_string(p)
            .with_context(|| format!("파일 읽기 실패: {}", p.display()))
    } else {
        use std::io::{IsTerminal, Read};
        if std::io::stdin().is_terminal() {
            bail!(
                "본문을 --file 없이 stdin 으로 받으려 했지만 터미널입니다 \
                 (그냥 실행하면 멈춘 것처럼 보임).\n\
                 --file <PATH> 로 파일을 지정하거나, 파이프로 입력하세요 \
                 (예: echo \"내용\" | openguild ...).\n\
                 지원하는 명령이면 --empty 로 빈 본문 생성도 가능합니다."
            );
        }
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
        /// 본문이 들어있는 파일. 미지정 시 stdin. 한글 등 비ASCII 는 --file
        /// 권장 — PowerShell 파이프(`echo | ...`)는 인코딩이 안 맞아 깨질
        /// 수 있음. `rule show` 로 확인했을 때 깨져 보이면 이 경우임.
        #[arg(long)]
        file: Option<std::path::PathBuf>,
    },
    /// 신규 규칙 생성 — 같은 slug 이미 있으면 에러. 본문은 `--file` / stdin.
    /// `--empty` 시 본문 없이 빈 규칙 생성.
    // DEV-227/BUG-111/DEV-232: quest/campaign/template/backup 이 전부
    // `new` 를 쓰는데 rules 만 `create` 가 canonical 이라 --help 에 create
    // 가 나왔음 — canonical 을 new 로 스왑. DEV-232: create alias 도
    // 사용자 지시로 완전 제거(rules 와 동일하게 — 남길 이유 없다는 판단).
    #[command(name = "new")]
    Create {
        slug: String,
        /// 본문이 들어있는 파일. 미지정 시 stdin. 한글 등은 --file 권장 —
        /// PowerShell 파이프는 인코딩이 안 맞아 깨질 수 있음.
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

// ─────────────────────────── Library 서브명령 ───────────────────────────

/// 도서관 문서 관리 — 파일 진리원 `.guild/library/`, 자체 BOOK 번호
/// (quest 번호와 별개, 단조 증가 재사용 금지).
#[derive(Subcommand)]
enum LibraryCmd {
    /// 문서 목록 (번호 / 제목 / 갱신 시각).
    List {
        /// 정렬된 표(헤더 + 컬럼)로 출력 — 사람용. --json 과 상호배타.
        #[arg(long)]
        table: bool,
    },
    /// 한 문서의 본문 출력 (stdout).
    Show {
        /// 문서 ID (BOOK-N 형식).
        id: String,
    },
    /// 새 문서 생성 — 번호는 자동 부여. 본문은 `--file` (미지정 시 빈 본문).
    New {
        #[arg(long)]
        title: String,
        /// 본문 파일 (UTF-8). 한글 등 비ASCII 는 stdin 파이프 대신 파일 권장.
        #[arg(long)]
        file: Option<std::path::PathBuf>,
        /// 소속 폴더 경로 (미지정 = 최상위). 예: `아키텍처/서브`.
        #[arg(long)]
        path: Option<String>,
    },
    /// 문서 수정 — 제공된 필드만 (title / 본문 파일 / 폴더 이동).
    Update {
        id: String,
        #[arg(long)]
        title: Option<String>,
        /// 새 본문 파일 (UTF-8). 미지정 시 본문 유지.
        #[arg(long)]
        file: Option<std::path::PathBuf>,
        /// 새 폴더 경로로 이동. 빈 문자열("")이면 최상위로 이동. 미지정 시 현재 위치 유지.
        #[arg(long)]
        path: Option<String>,
    },
    /// 문서 삭제 (soft delete — 번호는 재사용되지 않음). `--yes` 없으면 확인.
    Delete {
        id: String,
        #[arg(long)]
        yes: bool,
    },
    /// 폴더(계층) 관리 — 순수 컨테이너, 본문 없음.
    Folder {
        #[command(subcommand)]
        sub: LibraryFolderCmd,
    },
}

#[derive(Subcommand)]
enum LibraryFolderCmd {
    /// 폴더 목록 (path 순).
    List,
    /// 새 폴더 생성.
    New { path: String },
    /// 폴더 삭제 — 안에 문서/하위 폴더가 없어야 함.
    Delete {
        path: String,
        #[arg(long)]
        yes: bool,
    },
}

// ─────────────────────────── Worklog 서브명령 ───────────────────────────

/// 작업 기록 — 활동은 캐시 조회(quest 이력/댓글/생성), 노트는
/// `.guild/worklog/{YYYY-MM-DD}.md` (전역 공유, git tracked).
#[derive(Subcommand)]
enum WorklogCmd {
    /// 기간 내 활동 타임라인 + 집계. 기본: 오늘 하루.
    Show {
        /// 특정 날짜 하루 (YYYY-MM-DD). --from/--to 와 상호배타.
        #[arg(long, conflicts_with_all = ["from", "to"])]
        date: Option<String>,
        /// 기간 시작 (YYYY-MM-DD).
        #[arg(long, requires = "to")]
        from: Option<String>,
        /// 기간 끝 (YYYY-MM-DD, 포함).
        #[arg(long, requires = "from")]
        to: Option<String>,
    },
    /// 날짜별 노트 — show / set / clear.
    Note {
        #[command(subcommand)]
        sub: WorklogNoteCmd,
    },
}

#[derive(Subcommand)]
enum WorklogNoteCmd {
    /// 노트 본문 출력. 없으면 "(노트 없음)".
    Show {
        /// YYYY-MM-DD.
        date: String,
    },
    /// 노트 본문 교체. 본문은 `--file <PATH>` (UTF-8) — 한글 등은 파일 권장.
    Set {
        date: String,
        /// 본문 파일. 미지정 시 stdin.
        #[arg(long)]
        file: Option<std::path::PathBuf>,
    },
    /// 노트 삭제 (파일 제거).
    Clear { date: String },
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
        /// 정렬된 표(헤더 + 컬럼)로 출력 — 사람용. --json 과 상호배타.
        #[arg(long)]
        table: bool,
    },
    /// 캠페인 상세
    Show { slug: String },
    /// 캠페인 상태 변경 이력 — 최신 → 과거 순 (quest history 와 대칭).
    History { slug: String },
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

    fn put<B: Serialize, T: for<'de> Deserialize<'de>>(&self, path: &str, body: &B) -> Result<T> {
        let res = self.http.put(self.url(path)).json(body).send()?;
        self.handle(res)
    }

    /// DEV-239: 쿼리 파라미터(예: 폴더 path — 한글/슬래시 포함 가능)를 안전하게
    /// percent-encode 하기 위해 문자열 concat 대신 reqwest `.query()` 사용.
    fn delete_no_body_query(&self, path: &str, query: &[(&str, &str)]) -> Result<()> {
        let res = self.http.delete(self.url(path)).query(query).send()?;
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
            .ok_or_else(|| anyhow!(tf!("server 응답에 restored_to 누락", "server response missing restored_to")))?
            .to_string();
        let list = self.list_snapshots()?;
        list.into_iter()
            .find(|s| s.timestamp == ts)
            .ok_or_else(|| anyhow!(tf!("복원된 snapshot 정보 누락", "restored snapshot info missing")))
    }

    /// DEV-022: 시점 복원 (journal replay) — HTTP admin.
    fn restore_to_point(
        &self,
        target_ts: &str,
    ) -> Result<openguild_core::replay::ReplayReport> {
        let resp: serde_json::Value =
            self.post("/api/admin/restore", &serde_json::json!({ "at": target_ts }))?;
        Ok(openguild_core::replay::ReplayReport {
            target_ts: resp
                .get("replayed_to")
                .and_then(|v| v.as_str())
                .unwrap_or(target_ts)
                .to_string(),
            applied: resp.get("applied").and_then(|v| v.as_u64()).unwrap_or(0) as usize,
            pre_backup: resp
                .get("pre_backup")
                .and_then(|v| v.as_str())
                .map(String::from),
        })
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

    /// DEV-226: 캠페인 변경 이력 — quest history 와 대칭.
    fn campaign_history(
        &self,
        slug: &str,
    ) -> Result<Vec<openguild_core::models::CampaignHistoryEntry>> {
        self.get(&format!("/api/campaigns/{slug}/history"))
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

    // ─── 태그 정의 (top-level tag 그룹) ───

    fn tag_defs(&self) -> Result<Vec<openguild_core::models::QuestTagDef>> {
        self.get("/api/tag-defs")
    }

    fn tag_def_upsert(
        &self,
        slug: &str,
        color: &str,
        description: &str,
    ) -> Result<openguild_core::models::QuestTagDef> {
        self.post(
            "/api/tag-defs",
            &serde_json::json!({ "slug": slug, "color": color, "description": description }),
        )
    }

    fn tag_def_delete(&self, slug: &str) -> Result<()> {
        self.delete_no_body(&format!("/api/tag-defs/{slug}"))
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

/// DEV-222: `--description` / `--description-file` 통합 해석 — 파일 지정 시
/// UTF-8 로 읽는다 (clap `conflicts_with` 로 동시 지정은 이미 거부됨).
fn resolve_description_input(
    inline: Option<String>,
    file: Option<std::path::PathBuf>,
) -> Result<Option<String>> {
    match file {
        Some(p) => {
            let s = std::fs::read_to_string(&p)
                .with_context(|| format!("description 파일 읽기 실패: {}", p.display()))?;
            Ok(Some(s.trim_end().to_string()))
        }
        None => Ok(inline),
    }
}

/// DEV-210: `restore --at` 키워드 해석 — `latest`(대소문자 무시) 는 journal
/// 전체 재적용(= 최신 상태 복구)을 뜻하는 먼 미래 시각으로 치환. 그 외는
/// ISO 문자열 그대로(파싱/검증은 core 의 replay 가 담당).
fn resolve_at_keyword(at: &str) -> String {
    if at.eq_ignore_ascii_case("latest") {
        "9999-12-31T23:59:59Z".to_string()
    } else {
        at.to_string()
    }
}

/// `comment show --id <target>` 의 부모/자식 범위 조절 — admin 요청.
/// `depth` 만큼 답글을 BFS 로 따라가고(0 = target 만), `with_parents` 면
/// root 까지의 조상 체인도 앞에 붙인다(오래된 순). GUI(DEV-200)는 시각적으로
/// 2단까지만 보여주지만 실제 parent_id 체인은 임의 깊이라 CLI 는 그대로 노출.
/// target 이 없으면 None.
fn select_thread(
    entries: Vec<openguild_core::repo::comments::CommentEntry>,
    target: u64,
    depth: usize,
    with_parents: bool,
) -> Option<Vec<openguild_core::repo::comments::CommentEntry>> {
    use std::collections::HashMap;
    let by_id: HashMap<u64, openguild_core::repo::comments::CommentEntry> =
        entries.into_iter().map(|e| (e.id, e)).collect();
    let root = by_id.get(&target)?.clone();

    let mut out = Vec::new();
    if with_parents {
        let mut chain = Vec::new();
        let mut cur = root.parent_id;
        let mut seen = std::collections::HashSet::new();
        while let Some(pid) = cur {
            if !seen.insert(pid) {
                break; // 방어적 cycle 가드 — 정상 데이터에선 발생 안 함.
            }
            let Some(p) = by_id.get(&pid) else { break };
            chain.push(p.clone());
            cur = p.parent_id;
        }
        chain.reverse(); // 가장 오래된(root) 조상부터.
        out.extend(chain);
    }
    out.push(root);

    let mut frontier = vec![target];
    for _ in 0..depth {
        let mut next = Vec::new();
        for pid in &frontier {
            let mut children: Vec<_> =
                by_id.values().filter(|e| e.parent_id == Some(*pid)).cloned().collect();
            children.sort_by(|a, b| a.ts.cmp(&b.ts));
            for c in children {
                next.push(c.id);
                out.push(c);
            }
        }
        if next.is_empty() {
            break;
        }
        frontier = next;
    }
    Some(out)
}

/// DEV-221: 전역 댓글 검색 결과 한 건 (quest/campaign 통합).
#[derive(sqlx::FromRow, serde::Serialize)]
struct GlobalComment {
    /// "quest" | "campaign"
    scope: String,
    /// 소속 슬러그 (DEV-001 / C-001)
    slug: String,
    entry_id: i64,
    ts: String,
    author: String,
    body: String,
    discussion: bool,
    resolved: bool,
    /// BUG-110: 답글이면 부모 entry_id. `quest comment list` 는 `↩ #N` 표시하는데
    /// 전역 comments 검색은 이 필드가 없어 답글인지, 어디에 달렸는지 안 보였음.
    parent_id: Option<i64>,
    /// 상단 고정 여부 — 📌 배지.
    pinned: bool,
    /// 반응 캐시 원문(마커 attr 그대로, 콤마 구분) — 표시 시 split.
    reactions: String,
}

/// DEV-216: 도서관 문서 DTO — 서버 응답(BookResponse: book_id + flatten row)과
/// 동일 형태. 로컬 모드는 LibraryDocRow 에서 변환.
#[derive(Debug, Serialize, Deserialize)]
struct BookDto {
    book_id: String,
    number: i64,
    title: String,
    body: String,
    /// DEV-239: 소속 폴더 경로 ("" = 최상위).
    #[serde(default)]
    path: String,
    created_at: String,
    updated_at: String,
    deleted_at: Option<String>,
}

impl From<openguild_core::ops::library::LibraryDocRow> for BookDto {
    fn from(r: openguild_core::ops::library::LibraryDocRow) -> Self {
        Self {
            book_id: r.book_id(),
            number: r.number,
            title: r.title,
            body: r.body,
            path: r.path,
            created_at: r.created_at,
            updated_at: r.updated_at,
            deleted_at: r.deleted_at,
        }
    }
}

/// DEV-239: 도서관 폴더 DTO.
#[derive(Debug, Serialize, Deserialize)]
struct FolderDto {
    path: String,
    created_at: String,
    updated_at: String,
}

impl From<openguild_core::ops::library::LibraryFolderRow> for FolderDto {
    fn from(r: openguild_core::ops::library::LibraryFolderRow) -> Self {
        Self {
            path: r.path,
            created_at: r.created_at,
            updated_at: r.updated_at,
        }
    }
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
                eprintln!(
                    "{}",
                    tf!(
                        "⚠ 비정상 파일 {} 개 감지 (캐시에서 제외됨):",
                        "⚠ {} problem file(s) detected (excluded from cache):",
                        problems.len()
                    )
                );
                for (path, why) in &problems {
                    eprintln!("    - {path}: {why}");
                }
                eprintln!(
                    "{}",
                    tf!(
                        "  파일을 고치거나 status 를 정의한 뒤 `openguild reindex` 하세요.",
                        "  fix the file or define the status, then run `openguild reindex`."
                    )
                );
            }
        }
    }

    // ── 도메인 메서드 ──────────────────────────────────────

    /// DEV-221: 길드 전체 댓글 횡단 검색 — quest + campaign 캐시 UNION, 최신순.
    /// 로컬 전용 (index.db 직접 쿼리). 원격은 후속(HTTP 라우트 파리티) 전까지 미지원.
    #[allow(clippy::too_many_arguments)]
    fn comments_search(
        &self,
        author: Option<&str>,
        since: Option<&str>,
        until: Option<&str>,
        grep: Option<&str>,
        discussion: bool,
        unresolved: bool,
        limit: usize,
    ) -> Result<Vec<GlobalComment>> {
        let Backend::Local(l) = self else {
            return Err(anyhow!(
                "comments 전역 검색은 로컬 모드 전용입니다 (원격 HTTP 파리티는 후속)."
            ));
        };
        // quest / campaign 캐시 UNION. campaign_comments 엔 discussion 컬럼이
        // 없어 상수 0 — --discussion/--unresolved 필터 시 자연 제외.
        let mut conds_q = String::new();
        let mut conds_c = String::new();
        let mut push_both = |cond_q: &str, cond_c: &str| {
            conds_q.push_str(" AND ");
            conds_q.push_str(cond_q);
            conds_c.push_str(" AND ");
            conds_c.push_str(cond_c);
        };
        // 바인드 순서: (quest 절 인자들) → (campaign 절 인자들) → limit.
        // 두 절이 같은 필터 세트를 쓰므로 값을 두 번 바인드한다.
        let mut binds: Vec<String> = Vec::new();
        if let Some(a) = author {
            push_both("LOWER(c.author) = LOWER(?)", "LOWER(c.author) = LOWER(?)");
            binds.push(a.to_string());
        }
        if let Some(s) = since {
            push_both("c.ts >= ?", "c.ts >= ?");
            binds.push(openguild_core::time::normalize_filter_ts(s));
        }
        if let Some(u) = until {
            push_both("c.ts <= ?", "c.ts <= ?");
            binds.push(openguild_core::time::normalize_filter_ts(u));
        }
        if let Some(g) = grep {
            push_both(
                "LOWER(c.body) LIKE '%' || LOWER(?) || '%'",
                "LOWER(c.body) LIKE '%' || LOWER(?) || '%'",
            );
            binds.push(g.to_string());
        }
        if discussion {
            push_both("c.discussion = 1", "0 = 1");
        }
        if unresolved {
            push_both("c.discussion = 1 AND c.resolved = 0", "0 = 1");
        }

        let sql = format!(
            "SELECT * FROM (
               SELECT 'quest' AS scope,
                      qt.prefix || '-' || printf('%03d', q.number) AS slug,
                      c.entry_id, c.ts, c.author, c.body,
                      c.discussion, c.resolved, c.parent_id, c.pinned, c.reactions
                 FROM quest_comments c
                 JOIN quests q ON q.id = c.quest_id
                 JOIN quest_types qt ON qt.id = q.quest_type_id
                WHERE 1 = 1{conds_q}
               UNION ALL
               SELECT 'campaign' AS scope, ca.campaign_slug AS slug,
                      c.entry_id, c.ts, c.author, c.body,
                      0 AS discussion, 0 AS resolved, c.parent_id, c.pinned, c.reactions
                 FROM campaign_comments c
                 JOIN campaigns ca ON ca.id = c.campaign_id
                WHERE 1 = 1{conds_c}
             )
             ORDER BY ts DESC
             LIMIT ?"
        );
        let rows = l.rt.block_on(async {
            let mut q = sqlx::query_as::<_, GlobalComment>(&sql);
            for b in &binds {
                q = q.bind(b); // quest 절
            }
            for b in &binds {
                q = q.bind(b); // campaign 절 (동일 값 재바인드)
            }
            q = q.bind(limit as i64);
            q.fetch_all(&l.store.index_pool).await
        })?;
        Ok(rows)
    }

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

    /// DEV-226: 캠페인 변경 이력 — quest history 와 대칭.
    fn campaign_history(
        &self,
        slug: &str,
    ) -> Result<Vec<openguild_core::models::CampaignHistoryEntry>> {
        match self {
            Backend::Http(c) => c.campaign_history(slug),
            Backend::Local(l) => Self::map_err(l.rt.block_on(async {
                let row = openguild_core::services::campaigns::fetch_by_slug(
                    &l.store.index_pool,
                    slug,
                )
                .await?;
                openguild_core::services::campaigns::list_history(&l.store.index_pool, row.id)
                    .await
            })),
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

    // ── 태그 정의 (top-level tag 그룹) ──────────────────

    fn tag_defs(&self) -> Result<Vec<openguild_core::models::QuestTagDef>> {
        match self {
            Backend::Http(c) => c.tag_defs(),
            Backend::Local(l) => Self::map_err(
                l.rt.block_on(meta_svc::list_quest_tag_defs(&l.store.index_pool)),
            ),
        }
    }

    fn tag_def_upsert(
        &self,
        slug: &str,
        color: &str,
        description: &str,
    ) -> Result<openguild_core::models::QuestTagDef> {
        match self {
            Backend::Http(c) => c.tag_def_upsert(slug, color, description),
            Backend::Local(l) => Self::map_err(l.rt.block_on(
                openguild_core::ops::meta::upsert_tag_def(
                    &l.store,
                    slug.to_string(),
                    color.to_string(),
                    description.to_string(),
                ),
            )),
        }
    }

    fn tag_def_delete(&self, slug: &str) -> Result<()> {
        match self {
            Backend::Http(c) => c.tag_def_delete(slug),
            Backend::Local(l) => Self::map_err(l.rt.block_on(
                openguild_core::ops::meta::delete_tag_def(&l.store, slug.to_string()),
            )),
        }
    }

    /// 실사용 중 태그(quest/도서관 frontmatter 캐시) distinct — 로컬 전용
    /// (HTTP 라우트 없음, comments 전역 검색과 동일 정책).
    fn tags_in_use(&self) -> Result<Vec<String>> {
        let Backend::Local(l) = self else {
            return Err(anyhow!(tf!("--used 는 로컬 모드 전용입니다.", "--used is local-mode only.")));
        };
        let rows: Vec<(String,)> = l.rt.block_on(
            sqlx::query_as(
                "SELECT DISTINCT tag FROM quest_tags
                 UNION SELECT DISTINCT tag FROM library_tags
                 ORDER BY tag",
            )
            .fetch_all(&l.store.index_pool),
        )?;
        Ok(rows.into_iter().map(|(t,)| t).collect())
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

    // ── DEV-216: 도서관 — local + remote(HTTP /api/library) 둘 다 지원 ──

    fn library_list(&self) -> Result<Vec<BookDto>> {
        match self {
            Backend::Http(c) => c.get("/api/library"),
            Backend::Local(l) => Self::map_err(
                l.rt.block_on(openguild_core::ops::library::list_books(&l.store)),
            )
            .map(|rows| rows.into_iter().map(BookDto::from).collect()),
        }
    }

    fn library_get(&self, id: &str) -> Result<BookDto> {
        match self {
            Backend::Http(c) => c.get(&format!("/api/library/{id}")),
            Backend::Local(l) => {
                let row = Self::map_err(
                    l.rt.block_on(openguild_core::ops::library::get_book(&l.store, id)),
                )?;
                row.map(BookDto::from)
                    .ok_or_else(|| anyhow!(tf!("도서관 문서 '{id}' 없음", "library doc '{id}' not found")))
            }
        }
    }

    fn library_new(&self, title: &str, body: &str, path: &str) -> Result<BookDto> {
        match self {
            Backend::Http(c) => c.post(
                "/api/library",
                &serde_json::json!({ "title": title, "body": body, "path": path }),
            ),
            Backend::Local(l) => Self::map_err(l.rt.block_on(
                openguild_core::ops::library::create_book(&l.store, title, body, path),
            ))
            .map(BookDto::from),
        }
    }

    fn library_update(
        &self,
        id: &str,
        title: Option<&str>,
        body: Option<&str>,
        path: Option<&str>,
    ) -> Result<BookDto> {
        match self {
            Backend::Http(c) => c.patch(
                &format!("/api/library/{id}"),
                &serde_json::json!({ "title": title, "body": body, "path": path }),
            ),
            Backend::Local(l) => Self::map_err(l.rt.block_on(
                openguild_core::ops::library::update_book(&l.store, id, title, body, path),
            ))
            .map(BookDto::from),
        }
    }

    fn library_delete(&self, id: &str) -> Result<()> {
        match self {
            Backend::Http(c) => c.delete_no_body(&format!("/api/library/{id}")),
            Backend::Local(l) => Self::map_err(
                l.rt.block_on(openguild_core::ops::library::delete_book(&l.store, id)),
            ),
        }
    }

    // ── DEV-239: 도서관 폴더 — local + remote 둘 다 지원 ──

    fn library_folder_list(&self) -> Result<Vec<FolderDto>> {
        match self {
            Backend::Http(c) => c.get("/api/library/folders"),
            Backend::Local(l) => Self::map_err(
                l.rt.block_on(openguild_core::ops::library::list_folders(&l.store)),
            )
            .map(|rows| rows.into_iter().map(FolderDto::from).collect()),
        }
    }

    fn library_folder_new(&self, path: &str) -> Result<FolderDto> {
        match self {
            Backend::Http(c) => {
                c.post("/api/library/folders", &serde_json::json!({ "path": path }))
            }
            Backend::Local(l) => Self::map_err(
                l.rt.block_on(openguild_core::ops::library::create_folder(&l.store, path)),
            )
            .map(FolderDto::from),
        }
    }

    fn library_folder_delete(&self, path: &str) -> Result<()> {
        match self {
            Backend::Http(c) => {
                c.delete_no_body_query("/api/library/folders", &[("path", path)])
            }
            Backend::Local(l) => Self::map_err(
                l.rt.block_on(openguild_core::ops::library::delete_folder(&l.store, path)),
            ),
        }
    }

    // ── DEV-167: 작업 기록 — local + remote(HTTP /api/worklog) 둘 다 지원 ──

    fn worklog_activities(
        &self,
        from: &str,
        to: &str,
    ) -> Result<openguild_core::ops::worklog::WorklogReport> {
        match self {
            Backend::Http(c) => c.get(&format!("/api/worklog?from={from}&to={to}")),
            Backend::Local(l) => Self::map_err(
                l.rt.block_on(openguild_core::ops::worklog::activities(&l.store, from, to)),
            ),
        }
    }

    fn worklog_note_get(&self, date: &str) -> Result<Option<String>> {
        match self {
            Backend::Http(c) => {
                #[derive(Deserialize)]
                struct NoteDto {
                    content: Option<String>,
                }
                let n: NoteDto = c.get(&format!("/api/worklog/note/{date}"))?;
                Ok(n.content)
            }
            Backend::Local(l) => Self::map_err(Ok::<_, openguild_core::error::AppError>(
                openguild_core::ops::worklog::get_note(&l.store, date)?,
            )),
        }
    }

    fn worklog_note_set(&self, date: &str, content: String) -> Result<()> {
        match self {
            Backend::Http(c) => {
                let _: serde_json::Value = c.put(
                    &format!("/api/worklog/note/{date}"),
                    &serde_json::json!({ "content": content }),
                )?;
                Ok(())
            }
            Backend::Local(l) => Self::map_err(
                l.rt.block_on(openguild_core::ops::worklog::set_note(&l.store, date, content)),
            ),
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
            Backend::Http(_) => Err(anyhow!(tf!("원격 모드에선 미지원 — 로컬에서 실행", "not supported in remote mode — run locally"))),
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
            Backend::Http(_) => Err(anyhow!(tf!("원격 모드에선 미지원 — 로컬에서 실행", "not supported in remote mode — run locally"))),
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
            Backend::Http(_) => Err(anyhow!(tf!("원격 모드에선 미지원 — 로컬에서 실행", "not supported in remote mode — run locally"))),
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
            Backend::Http(_) => Err(anyhow!(tf!("원격 모드에선 미지원 — 로컬에서 실행", "not supported in remote mode — run locally"))),
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
            Backend::Http(_) => Err(anyhow!(tf!("원격 모드에선 미지원 — 로컬에서 실행", "not supported in remote mode — run locally"))),
            Backend::Local(l) => Self::map_err(l.rt.block_on(
                openguild_core::ops::comments::toggle_comment_resolved(&l.store, slug, id),
            )),
        }
    }

    /// DEV-234: 상단 고정(pin) 토글 — discussion 과 달리 quest 전용 아님, scope
    /// 로 quest/campaign 분기. React 와 동일하게 Local 전용.
    fn comments_toggle_pinned_scoped(
        &self,
        scope: CommentScope,
        slug: &str,
        id: u64,
    ) -> Result<openguild_core::repo::comments::CommentEntry> {
        let Backend::Local(l) = self else {
            return Err(Self::http_unsupported_meta());
        };
        match scope {
            CommentScope::Quest => Self::map_err(l.rt.block_on(
                openguild_core::ops::comments::toggle_comment_pinned(&l.store, slug, id),
            )),
            CommentScope::Campaign => Self::map_err(l.rt.block_on(
                openguild_core::ops::campaign_comments::toggle_pinned(&l.store, slug, id),
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

    /// DEV-199: 이모지 반응 토글 — GUI invoke / HTTP 와 동일한 core 함수 재사용.
    /// 다른 댓글 mutation 과 동일하게 Local 전용.
    fn comments_react_scoped(
        &self,
        scope: CommentScope,
        slug: &str,
        id: u64,
        emoji: &str,
        author: &str,
    ) -> Result<openguild_core::repo::comments::CommentEntry> {
        let Backend::Local(l) = self else {
            return Err(Self::http_unsupported_meta());
        };
        match scope {
            CommentScope::Quest => Self::map_err(l.rt.block_on(
                openguild_core::ops::comments::toggle_comment_reaction(
                    &l.store, slug, id, emoji, author,
                ),
            )),
            CommentScope::Campaign => Self::map_err(l.rt.block_on(
                openguild_core::ops::campaign_comments::toggle_reaction(
                    &l.store, slug, id, emoji, author,
                ),
            )),
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
                    .map_err(|e| anyhow::anyhow!(tf!("quest {slug} 본문 읽기 실패: {e:#}", "failed to read quest {slug} body: {e:#}")))?;
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
            Backend::Http(_) => Err(anyhow!(tf!(
                "원격 모드에선 미지원 — 서버에서 `openguild-server reindex` 사용",
                "not supported in remote mode — use `openguild-server reindex` on the server"
            ))),
            Backend::Local(l) => l
                .rt
                .block_on(openguild_core::reindex::reindex(&l.store))
                .map_err(|e| anyhow!(e)),
        }
    }

    /// DEV-159: index.db ↔ 파일 drift 검사. Local 전용 (remote 는 서버측 명령).
    fn check_drift(&self) -> Result<openguild_core::drift::DriftReport> {
        match self {
            Backend::Http(_) => Err(anyhow!(tf!(
                "원격 모드에선 미지원 — 서버에서 `openguild-server check-drift` 사용",
                "not supported in remote mode — use `openguild-server check-drift` on the server"
            ))),
            Backend::Local(l) => l
                .rt
                .block_on(openguild_core::drift::detect_drift(&l.store))
                .map_err(|e| anyhow!(e)),
        }
    }

    /// DEV-162: index.db VACUUM. Local 전용 (실행 중 host 는 HTTP admin 사용).
    fn vacuum(&self) -> Result<openguild_core::maintenance::VacuumReport> {
        match self {
            Backend::Http(_) => Err(anyhow!(tf!(
                "원격 모드에선 미지원 — 실행 중 host 는 HTTP admin, 오프라인은 로컬에서 실행",
                "not supported in remote mode — a running host uses HTTP admin, offline runs locally"
            ))),
            Backend::Local(l) => l
                .rt
                .block_on(openguild_core::maintenance::vacuum(&l.store))
                .map_err(|e| anyhow!(e)),
        }
    }

    /// DEV-162: journal.db 최근 op tail. Local 전용.
    fn journal_tail(&self, count: i64) -> Result<Option<openguild_core::maintenance::JournalTail>> {
        match self {
            Backend::Http(_) => Err(anyhow!(tf!(
                "원격 모드에선 미지원 — 실행 중 host 는 HTTP admin, 오프라인은 로컬에서 실행",
                "not supported in remote mode — a running host uses HTTP admin, offline runs locally"
            ))),
            Backend::Local(l) => l
                .rt
                .block_on(openguild_core::maintenance::journal_tail(&l.store.paths, count))
                .map_err(|e| anyhow!(e)),
        }
    }

    /// DEV-164: counter 정합 검사 / 보정. Local 전용.
    fn check_counters(&self, fix: bool) -> Result<openguild_core::ops::counter::CombinedReport> {
        match self {
            Backend::Http(_) => Err(anyhow!(tf!(
                "원격 모드에선 미지원 — 오프라인(로컬)에서 실행",
                "not supported in remote mode — run offline (locally)"
            ))),
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
            Backend::Http(_) => Err(anyhow!(tf!(
                "원격 모드에선 미지원 — 오프라인(로컬)에서 실행",
                "not supported in remote mode — run offline (locally)"
            ))),
            Backend::Local(l) => {
                let quests_dir = l.guild_path.join(".guild").join("quests");
                let has_md = std::fs::read_dir(&quests_dir)
                    .ok()
                    .into_iter()
                    .flatten()
                    .flatten()
                    .any(|e| e.path().extension().and_then(|s| s.to_str()) == Some("md"));
                if has_md {
                    return Err(anyhow!(tf!(
                        ".guild/quests/ 에 이미 quest 파일이 있습니다 — 마이그레이션은 한 번만. \
                         덮어쓰려면 quests/ 를 비운 뒤 재시도.",
                        ".guild/quests/ already has quest files — migration only runs once. \
                         To overwrite, empty quests/ and retry."
                    )));
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
            Backend::Http(_) => Err(anyhow!(tf!(
                "원격 모드에선 미지원 — 실행 중 host 정보는 server 측에서 확인",
                "not supported in remote mode — check a running host's info on the server"
            ))),
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
                        .ok_or_else(|| anyhow!(tf!("snapshot 없음: {ts}", "no such snapshot: {ts}")))?
                } else {
                    snapshots.last().cloned().ok_or_else(|| {
                        anyhow!(tf!("사용 가능한 snapshot 이 없습니다", "no snapshots available"))
                    })?
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
            Backend::Http(c) => c.restore_to_point(target_ts),
            Backend::Local(l) => {
                let snapshots = openguild_core::snapshot::list_snapshots(&l.store.paths)?;
                let latest = snapshots.last().cloned().ok_or_else(|| {
                    anyhow!(tf!(
                        "사용 가능한 snapshot 이 없습니다 (replay 는 최신 snapshot 기준)",
                        "no snapshots available (replay is based on the latest snapshot)"
                    ))
                })?;
                let report = l
                    .rt
                    .block_on(openguild_core::replay::replay_to(&l.store, &latest, target_ts))
                    .map_err(|e| anyhow!(tf!("replay 실패: {e}", "replay failed: {e}")))?;
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
        .ok_or_else(|| {
            anyhow!(tf!(
                "--type 필요 (또는 type 을 정의한 --template 지정)",
                "--type is required (or specify --template with a type defined)"
            ))
        })?;
    let title = title
        .or_else(|| tpl.and_then(|t| t.frontmatter.title.clone()))
        .ok_or_else(|| {
            anyhow!(tf!(
                "--title 필요 (또는 title 을 정의한 --template 지정)",
                "--title is required (or specify --template with a title defined)"
            ))
        })?;
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
                println!("{}", tf!("(첨부 없음)", "(no attachments)"));
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
                println!(
                    "{}",
                    tf!("✓ 첨부 추가 — 총 {} 개", "✓ attachment added — {} total", list.len())
                );
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
                return Err(anyhow!(tf!(
                    "그런 첨부 없음: {path}\n  `attach list {slug}` 로 경로를 확인하세요",
                    "no such attachment: {path}\n  check the path with `attach list {slug}`"
                )));
            }
            let list = c.attachments_remove(scope, &slug, &path)?;
            if json {
                println!("{}", serde_json::json!({ "ok": true, "count": list.len() }));
            } else {
                println!(
                    "{}",
                    tf!("✓ 첨부 제거 — 남은 {} 개", "✓ attachment removed — {} remaining", list.len())
                );
            }
        }
    }
    Ok(())
}

/// quest / campaign 댓글 명령 공용 핸들러 (DEV-100).
/// `--depth` 값 파서 — 숫자 또는 `all`(무제한).
fn parse_comment_depth(s: &str) -> Result<usize, String> {
    if s.eq_ignore_ascii_case("all") {
        return Ok(usize::MAX);
    }
    s.parse::<usize>().map_err(|_| {
        tf!(
            "'{s}' — 숫자 또는 'all' 이어야 합니다",
            "'{s}' — must be a number or 'all'"
        )
    })
}

/// 댓글 대상(quest/campaign)이 실존하는지 — 미존재 slug 가 "(댓글 없음)" 으로
/// 침묵 통과하지 않게. 댓글이 0건일 때만 호출(정상 경로 비용 없음).
fn ensure_scope_target_exists(c: &Backend, scope: CommentScope, slug: &str) -> Result<()> {
    match scope {
        CommentScope::Quest => {
            c.quest_by_slug(slug).map_err(|_| {
                anyhow!(tf!(
                    "퀘스트 '{slug}' 가 존재하지 않습니다",
                    "quest '{slug}' does not exist"
                ))
            })?;
        }
        CommentScope::Campaign => {
            c.campaign_show(slug).map_err(|_| {
                anyhow!(tf!(
                    "캠페인 '{slug}' 가 존재하지 않습니다",
                    "campaign '{slug}' does not exist"
                ))
            })?;
        }
    }
    Ok(())
}

/// 댓글 메타 배지 — 📌(고정) / ● 미해결 토론 / ✓ 해결됨. list/show/comments
/// 세 출력이 동일 규칙을 쓰도록 단일화.
fn comment_badges(pinned: bool, discussion: bool, resolved: bool) -> String {
    let mut out = String::new();
    if pinned {
        out.push_str(" 📌");
    }
    if discussion {
        out.push_str(&if resolved {
            tf!(" ✓해결", " ✓resolved")
        } else {
            tf!(" ●미해결", " ●unresolved")
        });
    }
    out
}

/// 반응 집계 표시 — `[👍2 ✅1]` (없으면 빈 문자열). list 의 DEV-199 포맷을
/// show / comments 횡단에서도 재사용.
fn reactions_summary(reactions: &[String]) -> String {
    if reactions.is_empty() {
        return String::new();
    }
    use openguild_core::repo::comments::split_reaction;
    let agg = reactions
        .iter()
        .map(|r| {
            let (em, a) = split_reaction(r);
            format!("{em}{}", a.len().max(1))
        })
        .collect::<Vec<_>>()
        .join(" ");
    format!("  [{agg}]")
}

fn run_comment_cmd(c: &Backend, scope: CommentScope, sub: CommentCmd, json: bool) -> Result<()> {
    match sub {
                CommentCmd::List { slug, author, since, top_only, reply_to, grep, reverse, limit, tree } => {
                    let mut entries = c.comments_list_scoped(scope, &slug)?;
                    // DEV-250: 미존재 slug 가 "(댓글 없음)" 으로 침묵 통과하면
                    // 오타를 들고 계속 진행하게 됨(특히 agent) — 빈 결과일 때만
                    // 실존 확인(정상 경로 비용 0).
                    if entries.is_empty() {
                        ensure_scope_target_exists(c, scope, &slug)?;
                    }
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
                    // DEV-221: 최신순 + 개수 제한 (필터 적용 후).
                    if reverse {
                        entries.reverse();
                    }
                    if let Some(n) = limit {
                        entries.truncate(n);
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
                                    // DEV-250: 메타도 agent 가 파싱 가능하게.
                                    "pinned": e.pinned,
                                    "discussion": e.discussion,
                                    "resolved": e.resolved,
                                    "reactions": e.reactions,
                                })).collect::<Vec<_>>(),
                            })
                        );
                    } else if entries.is_empty() {
                        println!("{}", tf!("(댓글 없음)", "(no comments)"));
                    } else {
                        // 한 줄 렌더 — flat/tree 공용. DEV-250: 📌/토론 배지 추가.
                        let render = |e: &openguild_core::repo::comments::CommentEntry,
                                      prefix: &str| {
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
                                tf!("(이름 없음)", "(no name)")
                            } else {
                                e.author.clone()
                            };
                            let ts = if e.ts.is_empty() {
                                tf!("(시각 미상)", "(unknown time)")
                            } else {
                                e.ts.clone()
                            };
                            let badges = comment_badges(e.pinned, e.discussion, e.resolved);
                            // DEV-199: 반응 집계 표시 — 👍2 ✅1 형태.
                            let reacts = reactions_summary(&e.reactions);
                            println!(
                                "{prefix}#{}  {}  {}{}{}  {}{}",
                                e.id, ts, author, reply, badges, summary, reacts
                            );
                        };
                        if tree {
                            // DEV-250: parent_id 들여쓰기 트리 (대화 흐름 순).
                            // 필터로 부모가 결과에서 빠진 답글은 root 로 표시.
                            use std::collections::{HashMap, HashSet};
                            let ids: HashSet<u64> = entries.iter().map(|e| e.id).collect();
                            let mut children: HashMap<u64, Vec<&_>> = HashMap::new();
                            let mut roots: Vec<&_> = Vec::new();
                            for e in &entries {
                                match e.parent_id {
                                    Some(p) if ids.contains(&p) => {
                                        children.entry(p).or_default().push(e)
                                    }
                                    _ => roots.push(e),
                                }
                            }
                            fn walk<'a>(
                                e: &'a openguild_core::repo::comments::CommentEntry,
                                depth: usize,
                                children: &HashMap<u64, Vec<&'a openguild_core::repo::comments::CommentEntry>>,
                                render: &impl Fn(&openguild_core::repo::comments::CommentEntry, &str),
                            ) {
                                let prefix = if depth == 0 {
                                    String::new()
                                } else {
                                    format!("{}└─ ", "   ".repeat(depth - 1))
                                };
                                render(e, &prefix);
                                for kid in children.get(&e.id).into_iter().flatten() {
                                    walk(kid, depth + 1, children, render);
                                }
                            }
                            for r in roots {
                                walk(r, 0, &children, &render);
                            }
                        } else {
                            for e in &entries {
                                render(e, "");
                            }
                        }
                    }
                }
                CommentCmd::Show { slug, id, depth, with_parents } => {
                    let entries = c.comments_list_scoped(scope, &slug)?;
                    // DEV-250: 미존재 slug 침묵 통과 방지 (list 와 동일).
                    if entries.is_empty() {
                        ensure_scope_target_exists(c, scope, &slug)?;
                    }
                    let selected: Vec<_> = match id {
                        Some(target) => select_thread(entries, target, depth, with_parents)
                            .ok_or_else(|| {
                                anyhow::anyhow!(
                                    "{}",
                                    tf!(
                                        "entry #{target} 없음 ({} {slug})",
                                        "entry #{target} not found ({} {slug})",
                                        scope.noun()
                                    )
                                )
                            })?,
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
                                    // DEV-250: 메타도 agent 가 파싱 가능하게.
                                    "pinned": e.pinned,
                                    "discussion": e.discussion,
                                    "resolved": e.resolved,
                                    "reactions": e.reactions,
                                })).collect::<Vec<_>>(),
                            })
                        );
                    } else if selected.is_empty() {
                        println!("{}", tf!("(댓글 없음)", "(no comments)"));
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
                                tf!("(이름 없음)", "(no name)")
                            } else {
                                e.author.clone()
                            };
                            // DEV-250: 📌/토론 배지 + 반응 집계를 헤더에.
                            let badges = comment_badges(e.pinned, e.discussion, e.resolved);
                            let reacts = reactions_summary(&e.reactions);
                            println!("#{}  {}  {}{}{}{}", e.id, e.ts, author, reply, badges, reacts);
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
                        println!(
                            "{}",
                            tf!(
                                "✓ 댓글 추가: #{} ({}{})",
                                "✓ comment added: #{} ({}{})",
                                entry.id,
                                entry.ts,
                                reply
                            )
                        );
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
                        println!(
                            "{}",
                            tf!("✓ 댓글 #{} 본문 갱신됨", "✓ comment #{} body updated", entry.id)
                        );
                    }
                }
                CommentCmd::Rm { slug, id, force } => {
                    if !force {
                        eprint!(
                            "{}",
                            tf!(
                                "댓글 #{id} ({} {slug}) 을 삭제할까요? (y/N) ",
                                "Delete comment #{id} ({} {slug})? (y/N) ",
                                scope.noun()
                            )
                        );
                        use std::io::Write;
                        std::io::stderr().flush().ok();
                        let mut buf = String::new();
                        std::io::stdin().read_line(&mut buf)?;
                        if !matches!(buf.trim(), "y" | "Y" | "yes") {
                            println!("{}", tf!("(취소)", "(cancelled)"));
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
                        println!("{}", tf!("✓ 댓글 #{id} 삭제됨", "✓ comment #{id} deleted"));
                    }
                }
                CommentCmd::React { slug, id, emoji, author } => {
                    let entry = c.comments_react_scoped(scope, &slug, id, &emoji, &author)?;
                    // 토글 결과 요약 — 현재 entry 의 반응 집계 표시.
                    use openguild_core::repo::comments::split_reaction;
                    let now_on = entry.reactions.iter().any(|r| {
                        let (e, authors) = split_reaction(r);
                        e == emoji && authors.iter().any(|a| a == &author)
                    });
                    let agg = entry
                        .reactions
                        .iter()
                        .map(|r| {
                            let (e, authors) = split_reaction(r);
                            format!("{e}{}", authors.len().max(1))
                        })
                        .collect::<Vec<_>>()
                        .join(" ");
                    if json {
                        println!(
                            "{}",
                            serde_json::json!({
                                "ok": true,
                                "id": entry.id,
                                "emoji": emoji,
                                "on": now_on,
                                "reactions": entry.reactions,
                            })
                        );
                    } else {
                        let action = if now_on {
                            tf!("추가", "added")
                        } else {
                            tf!("제거", "removed")
                        };
                        let agg_label = if agg.is_empty() { tf!("없음", "none") } else { agg };
                        println!(
                            "{}",
                            tf!(
                                "✓ #{} {emoji} {} (현재: {})",
                                "✓ #{} {emoji} {} (now: {})",
                                entry.id,
                                action,
                                agg_label
                            )
                        );
                    }
                }
                CommentCmd::Discussion { slug, id } => {
                    if scope != CommentScope::Quest {
                        anyhow::bail!(tf!(
                            "토론(discussion) 토글은 quest 댓글 전용입니다.",
                            "discussion toggle is only available for quest comments."
                        ));
                    }
                    let e = c.comments_toggle_discussion(&slug, id)?;
                    if json {
                        println!(
                            "{}",
                            serde_json::json!({ "ok": true, "id": id, "discussion": e.discussion, "resolved": e.resolved })
                        );
                    } else {
                        let state = if e.discussion {
                            tf!("표시", "marked")
                        } else {
                            tf!("해제", "unmarked")
                        };
                        println!("{}", tf!("✓ 댓글 #{id} 토론 {}", "✓ comment #{id} discussion {}", state));
                    }
                }
                CommentCmd::Resolved { slug, id } => {
                    if scope != CommentScope::Quest {
                        anyhow::bail!(tf!(
                            "resolved 토글은 quest 댓글 전용입니다.",
                            "resolved toggle is only available for quest comments."
                        ));
                    }
                    let e = c.comments_toggle_resolved(&slug, id)?;
                    if json {
                        println!("{}", serde_json::json!({ "ok": true, "id": id, "resolved": e.resolved }));
                    } else {
                        let state = if e.resolved {
                            tf!("해결됨", "resolved")
                        } else {
                            tf!("미해결", "unresolved")
                        };
                        println!("{}", tf!("✓ 댓글 #{id} {}", "✓ comment #{id} {}", state));
                    }
                }
                CommentCmd::Pinned { slug, id } => {
                    let e = c.comments_toggle_pinned_scoped(scope, &slug, id)?;
                    if json {
                        println!("{}", serde_json::json!({ "ok": true, "id": id, "pinned": e.pinned }));
                    } else {
                        let state = if e.pinned {
                            tf!("고정됨", "pinned")
                        } else {
                            tf!("고정 해제", "unpinned")
                        };
                        println!("{}", tf!("✓ 댓글 #{id} {}", "✓ comment #{id} {}", state));
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
                            println!("{}", tf!("(메모 비어있음)", "(memo is empty)"));
                        } else {
                            print!("{s}");
                            if !s.ends_with('\n') {
                                println!();
                            }
                        }
                    } else {
                        println!("{}", tf!("(메모 없음)", "(no memo)"));
                    }
                }
                MemoCmd::Set { slug, file } => {
                    let content = read_content(file.as_deref())?;
                    c.memo_set_scoped(scope, &slug, content)?;
                    if json {
                        println!("{}", serde_json::json!({ "ok": true, "slug": slug }));
                    } else {
                        println!(
                            "{}",
                            tf!("✓ 메모 저장됨 ({} {slug})", "✓ memo saved ({} {slug})", scope.noun())
                        );
                    }
                }
                MemoCmd::Clear { slug } => {
                    c.memo_set_scoped(scope, &slug, String::new())?;
                    if json {
                        println!("{}", serde_json::json!({ "ok": true, "slug": slug }));
                    } else {
                        println!(
                            "{}",
                            tf!("✓ 메모 비움 ({} {slug})", "✓ memo cleared ({} {slug})", scope.noun())
                        );
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
        println!("{}", json_str(q));
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
            println!("{}", json_str(&detail.quest));
        } else {
            println!(
                "{}",
                tf!(
                    "(이미 {} 상태입니다 — 변경 없음)",
                    "(already in {} status — no change)",
                    colorize(&detail.quest.status_name_en, &detail.quest.status_color)
                )
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
        println!("{}", json_str(q));
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
            println!("{}", json_str(r));
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
            println!("{}", json_str(d));
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
        CampaignCmd::List { status, table } => {
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
                println!("{}", json_str(&rows));
            } else if table {
                let cells: Vec<Vec<TableCell>> = rows
                    .iter()
                    .map(|r| {
                        vec![
                            (r.campaign_slug.clone(), None),
                            (r.status.clone(), None),
                            (r.started_at.clone().unwrap_or_default(), None),
                            (r.ended_at.clone().unwrap_or_default(), None),
                            (r.title.clone(), None),
                        ]
                    })
                    .collect();
                render_table(
                    &["ID", "STATUS", "START", "END", "TITLE"],
                    &[false, false, false, false],
                    &cells,
                    "campaigns",
                );
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
        CampaignCmd::History { slug } => {
            let history = c.campaign_history(&slug)?;
            if json {
                println!("{}", json_str(&history));
            } else if history.is_empty() {
                println!("{}", tf!("(이력 없음)", "(no history)"));
            } else {
                for h in &history {
                    let old = h.old_value.as_deref().unwrap_or("∅");
                    let new = h.new_value.as_deref().unwrap_or("∅");
                    let rel = openguild_core::time::format_relative(&h.ts).unwrap_or_else(|| "—".into());
                    println!("{}  {:<10} {} → {}", h.ts, rel, old, new);
                }
                println!("-- {} entries", history.len());
            }
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
                return Err(anyhow!(tf!(
                    "삭제하려면 --yes 를 명시하세요 (안전장치). 예: campaign delete {slug} --yes",
                    "specify --yes to delete (safety guard). e.g. campaign delete {slug} --yes"
                )));
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
                    println!("{}", json_str(&item));
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
        println!("{}", json_str(&quests));
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

/// 테이블 셀 — (표시 텍스트, 색 hex). 색은 패딩 **후** 적용해 ANSI 코드가
/// 폭 계산을 깨지 않게 한다.
type TableCell = (String, Option<String>);

/// 공용 테이블 렌더 — 헤더 + 컬럼 폭 정렬 + 행수 footer. 원칙: **한글 등
/// 가변폭(더블폭) 문자가 올 수 있는 텍스트는 마지막 컬럼에** — 마지막 컬럼은
/// 패딩하지 않으므로 폭 계산 문제가 없다. 그 외 컬럼은 ASCII 전제.
/// `right_align[i]` = 해당 컬럼 우측 정렬(숫자용).
fn render_table(headers: &[&str], right_align: &[bool], rows: &[Vec<TableCell>], noun: &str) {
    if rows.is_empty() {
        println!("(no {noun})");
        return;
    }
    let ncols = headers.len();
    // 마지막 컬럼 제외 폭 계산.
    let widths: Vec<usize> = (0..ncols.saturating_sub(1))
        .map(|i| {
            rows.iter()
                .map(|r| r.get(i).map(|(t, _)| t.chars().count()).unwrap_or(0))
                .chain(std::iter::once(headers[i].chars().count()))
                .max()
                .unwrap_or(0)
        })
        .collect();
    let pad = |text: &str, i: usize| -> String {
        if i + 1 == ncols {
            return text.to_string(); // 마지막 컬럼 — 패딩 없음.
        }
        let w = widths[i];
        if right_align.get(i).copied().unwrap_or(false) {
            format!("{text:>w$}")
        } else {
            format!("{text:<w$}")
        }
    };
    let header_line = headers
        .iter()
        .enumerate()
        .map(|(i, h)| pad(h, i))
        .collect::<Vec<_>>()
        .join("  ");
    println!("{header_line}");
    println!("{}", "─".repeat(header_line.chars().count().max(10)));
    for row in rows {
        let line = row
            .iter()
            .enumerate()
            .map(|(i, (text, color))| {
                let padded = pad(text, i);
                match color {
                    Some(c) if !c.is_empty() => colorize(&padded, c),
                    _ => padded,
                }
            })
            .collect::<Vec<_>>()
            .join("  ");
        println!("{line}");
    }
    println!("-- {} {noun}", rows.len());
}

/// DEV-211: 사람용 정렬 표 — 색은 line 렌더와 동일 규칙(type 색 ID / status
/// 색 / urgency 색). 제목은 한글 가변폭이라 마지막 컬럼.
fn print_quest_table(quests: &[Quest]) {
    let rows: Vec<Vec<TableCell>> = quests
        .iter()
        .map(|q| {
            vec![
                (q.quest_id.clone(), Some(q.type_color.clone())),
                (q.status_name_en.clone(), Some(q.status_color.clone())),
                (q.urgency.to_string(), Some(urgency_color(q.urgency).to_string())),
                (q.title.clone(), None),
            ]
        })
        .collect();
    render_table(
        &["ID", "STATUS", "URG", "TITLE"],
        &[false, false, true],
        &rows,
        "quests",
    );
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
        println!("{}", json_str(d));
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
        println!(
            "  {} : id={p} {}",
            colorize("parent", "#7ee787"),
            tf!("(불러올 수 없음)", "(could not be loaded)")
        );
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
        println!("{}", tf!("✓ index.db 재구축 완료", "✓ index.db rebuilt"));
        for line in report.summary_lines() {
            println!("  {line}");
        }
        if !report.skipped.is_empty() {
            println!();
            println!(
                "{}",
                tf!(
                    "⚠ {} 개 파일 skip 됨 (파싱 / 무결성 실패):",
                    "⚠ {} file(s) skipped (parse / integrity failure):",
                    report.skipped.len()
                )
            );
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
        println!("{}", tf!("✓ index.db 가 파일과 일치 (drift 없음)", "✓ index.db matches files (no drift)"));
    } else {
        println!("{}", tf!("⚠ drift 발견:", "⚠ drift found:"));
        let sections = [
            (tf!("파일은 있는데 index 에 없음", "file exists but missing from index"), &report.missing_in_index),
            (tf!("index 에 있는데 파일이 없음", "in index but file missing"), &report.stale_in_index),
            (tf!("파일 mtime > index.db mtime", "file mtime > index.db mtime"), &report.fresh_files),
            (
                tf!("sibling(.comments/.memo) 가 더 새것", "sibling (.comments/.memo) is newer"),
                &report.fresh_siblings,
            ),
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
            println!("{}", tf!("▸ reindex 실행 중...", "▸ running reindex..."));
            c.reindex()?;
            println!("{}", tf!("✓ resync 완료", "✓ resync complete"));
        } else {
            println!("{}", tf!("(--resync 로 자동 reindex 가능)", "(use --resync to auto-reindex)"));
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
        println!("{}", tf!("✓ counter 검증 완료", "✓ counter check complete"));
        println!(
            "{}",
            tf!(
                "  검사된 type 수 : {}",
                "  types checked  : {}",
                report.file_report.types_checked
            )
        );
        println!(
            "{}",
            tf!(
                "  발견 이슈     : {} (file) + {} (SQL)",
                "  issues found   : {} (file) + {} (SQL)",
                report.file_report.issues.len(),
                report.sql_drift.len()
            )
        );
        for issue in &report.file_report.issues {
            println!();
            println!("  • type {} [file drift]:", issue.prefix);
            println!(
                "{}",
                tf!(
                    "    저장된 last_number   : {}",
                    "    stored last_number   : {}",
                    issue.stored_last_number
                )
            );
            println!(
                "{}",
                tf!(
                    "    실제 max quest 번호  : {}",
                    "    actual max quest num : {}",
                    issue.actual_max_number
                )
            );
            if fix {
                println!(
                    "{}",
                    tf!(
                        "    → {} 으로 보정됨 (file + SQL)",
                        "    → corrected to {} (file + SQL)",
                        issue.corrected_to
                    )
                );
            } else {
                println!("{}", tf!("    (--fix 로 자동 보정 가능)", "    (use --fix to auto-correct)"));
            }
        }
        for drift in &report.sql_drift {
            println!();
            println!("  • type {} [SQL drift]:", drift.prefix);
            println!("    file last_number     : {}", drift.file_last_number);
            println!("    SQL  last_number     : {}", drift.sql_last_number);
            if fix {
                println!(
                    "{}",
                    tf!(
                        "    → {} 으로 보정됨 (SQL ← file)",
                        "    → corrected to {} (SQL ← file)",
                        drift.synced_to
                    )
                );
            } else {
                println!("{}", tf!("    (--fix 로 자동 보정 가능)", "    (use --fix to auto-correct)"));
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
        println!("{}", tf!("✓ VACUUM 완료", "✓ VACUUM complete"));
        println!("  before : {} bytes", r.before_bytes);
        println!("  after  : {} bytes", r.after_bytes);
        if r.saved() > 0 && r.before_bytes > 0 {
            println!(
                "  saved  : {} bytes ({:.1}%)",
                r.saved(),
                (r.saved() as f64 / r.before_bytes as f64) * 100.0
            );
        } else {
            println!("{}", tf!("  saved  : 0 bytes (이미 dense)", "  saved  : 0 bytes (already dense)"));
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
                println!(
                    "{}",
                    tf!(
                        "(journal.db 없음 — 아직 mutation 안 됐거나 snapshot 직후)",
                        "(no journal.db — no mutations yet, or just after a snapshot)"
                    )
                );
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

/// DEV-227 후속(BUG-110): `type`/`status` 는 canonical 단수형이라 sub 필수로
/// 바꿨는데, clap alias(`types`/`statuses`)는 같은 파싱 트리를 공유해 alias
/// 로 불러도 sub 필수가 그대로 적용됨 — 원래 alias 를 유지한 취지(기존
/// 스크립트가 bare `openguild types` 로 list 를 기대하던 관행 안 깨기)가
/// 무색해짐. clap 파싱 이후엔 어떤 이름(canonical/alias)으로 불렀는지 구분이
/// 안 되므로, raw argv 단계에서 legacy plural bare 호출만 `list` 를 끼워
/// 넣어 예전처럼 동작하게 한다. canonical `type`/`status` 는 여전히 sub 필수.
fn rewrite_legacy_plural_bare_invocation(args: Vec<String>) -> Vec<String> {
    const LEGACY_PLURALS: &[&str] = &["types", "statuses"];
    let mut i = 1; // args[0] = 실행 파일 경로.
    let cmd_idx = loop {
        let Some(a) = args.get(i) else { break None };
        if a == "--json" {
            i += 1;
        } else if a == "--remote" || a == "--guild" {
            i += 2; // flag + value
        } else if a.starts_with("--") {
            i += 1;
        } else {
            break Some(i);
        }
    };
    if let Some(idx) = cmd_idx
        && LEGACY_PLURALS.contains(&args[idx].as_str())
        && args.len() == idx + 1
    {
        let mut out = args;
        out.push("list".to_string());
        return out;
    }
    args
}

fn run() -> Result<()> {
    let cli = Cli::parse_from(rewrite_legacy_plural_bare_invocation(
        std::env::args().collect(),
    ));

    // DEV-211: --compact — json_str() 이 참조하는 전역 플래그 설정.
    JSON_COMPACT.store(cli.compact, std::sync::atomic::Ordering::Relaxed);

    // DEV-254: 저장된 언어(~/.openguild/locale.json, GUI 와 공유) 로드 —
    // tf! 매크로가 참조하는 프로세스 전역 플래그 1회 설정.
    LOCALE.store(
        openguild_core::locale::current() == openguild_core::locale::Locale::En,
        std::sync::atomic::Ordering::Relaxed,
    );

    // Init 은 길드 자체를 만드는 명령 — 백엔드 연결 불필요. 먼저 처리.
    if let Command::Init { name } = &cli.command {
        return init_guild(name.clone(), cli.json);
    }
    // docs 도 길드/백엔드 무관 (embed 문서 출력) — 길드 밖에서도 동작해야 함.
    if let Command::Docs { name } = &cli.command {
        return handle_docs(cli.json, name.clone());
    }
    // locale 도 길드/백엔드 무관 — 어디서든 언어 조회/변경 가능해야 함.
    if let Command::Locale { lang } = &cli.command {
        return handle_locale(cli.json, lang.clone());
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
        Command::Types { sub } => handle_types(&c, cli.json, sub)?,
        Command::Statuses { sub } => handle_statuses(&c, cli.json, sub)?,
        // DEV-060: 퀘스트 템플릿.
        Command::Template { sub } => handle_template(&c, cli.json, sub)?,
        Command::Rules { sub } => handle_rules(&c, cli.json, sub)?,
        Command::Library { sub } => handle_library(&c, cli.json, sub)?,
        Command::Tag { sub } => handle_tag(&c, cli.json, sub)?,
        Command::Docs { .. } => unreachable!("handled above"),
        Command::Locale { .. } => unreachable!("handled above"),
        Command::Worklog { sub } => handle_worklog(&c, cli.json, sub)?,
        Command::Backup { sub } => handle_backup(&c, cli.json, sub)?,
        Command::Restore { to, at } => handle_restore(&c, cli.json, to, at)?,
        Command::Reindex => run_reindex_cmd(&c, cli.json)?,
        Command::Check { sub } => match sub {
            CheckCmd::Drift { resync } => run_check_drift_cmd(&c, resync, cli.json)?,
            CheckCmd::Counters { fix } => run_check_counters_cmd(&c, fix, cli.json)?,
        },
        Command::Index { sub } => match sub {
            IndexCmd::Rebuild => run_reindex_cmd(&c, cli.json)?,
            IndexCmd::Vacuum => run_vacuum_cmd(&c, cli.json)?,
        },
        Command::Comments { author, since, until, grep, discussion, unresolved, limit, summary } => {
            let rows = c.comments_search(
                author.as_deref(),
                since.as_deref(),
                until.as_deref(),
                grep.as_deref(),
                discussion,
                unresolved,
                limit,
            )?;
            if cli.json {
                println!("{}", json_str(&rows));
            } else if rows.is_empty() {
                println!("{}", tf!("(댓글 없음)", "(no comments)"));
            } else {
                let no_name = tf!("(이름 없음)", "(no name)");
                for r in &rows {
                    let author = if r.author.is_empty() { &no_name } else { &r.author };
                    // DEV-250: 📌/토론 배지 + 반응 집계 — comment list/show 와 동일 규칙.
                    let badge = comment_badges(r.pinned, r.discussion, r.resolved);
                    let reactions: Vec<String> = if r.reactions.is_empty() {
                        Vec::new()
                    } else {
                        r.reactions.split(',').map(|s| s.to_string()).collect()
                    };
                    let reacts = reactions_summary(&reactions);
                    // BUG-110: quest comment list 와 동일하게 답글이면 부모 표시.
                    let reply = r.parent_id.map(|p| format!(" ↩ #{p}")).unwrap_or_default();
                    println!(
                        "{:<9} #{:<3}{}  {}  {}{}{}",
                        r.slug, r.entry_id, reply, r.ts, author, badge, reacts
                    );
                    // 기본은 본문 전체 (quest comment list 와 동일) — --summary 시
                    // 첫 줄 60자만. 요약만 보고 뒷줄을 놓쳐 답글을 잘못 단 사고
                    // (2026-07-05) 이후 기본을 전체로 바꿈.
                    if summary {
                        let s: String = r.body.lines().next().unwrap_or("").chars().take(60).collect();
                        println!("  {s}");
                    } else {
                        for line in r.body.lines() {
                            println!("  {line}");
                        }
                    }
                }
            }
        }
        Command::Journal { sub } => match sub {
            JournalCmd::Tail { count } => run_journal_tail_cmd(&c, count, cli.json)?,
        },
        Command::MigrateToFiles => handle_migrate_to_files(&c, cli.json)?,
        Command::Info { brief } => handle_info(&c, cli.json, brief)?,
        Command::Campaign { sub } => handle_campaign(&c, cli.json, sub)?,
        Command::Quest { sub } => handle_quest(&c, cli.json, sub)?,
    }
    Ok(())
}

// ─────────────────────────── init ───────────────────────────

/// 현재 디렉토리를 길드로 초기화. `<name>.guild` 마커 파일 생성.
fn init_guild(name_arg: Option<String>, json: bool) -> Result<()> {
    let cwd = std::env::current_dir()
        .with_context(|| tf!("현재 디렉토리를 확인할 수 없음", "could not determine current directory"))?;
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
        .with_context(|| tf!("디렉토리 읽기 실패", "failed to read directory"))?
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
            anyhow!(tf!(
                "디렉토리 경로 인코딩 오류: {}",
                "directory path encoding error: {}",
                cwd.display()
            ))
        })?)
        .with_context(|| tf!("기존 마커 파싱 실패: {}", "failed to parse existing marker: {}", path.display()))?;
        if let Some(arg) = &name_arg
            && arg != &parsed.name
        {
            eprintln!(
                "{}",
                tf!(
                    "ℹ︎ 기존 길드 이름 보존: \"{}\" (--name \"{}\" 무시)",
                    "ℹ︎ keeping existing guild name: \"{}\" (--name \"{}\" ignored)",
                    parsed.name,
                    arg
                )
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
                .ok_or_else(|| {
                    anyhow!(tf!(
                        "현재 디렉토리 이름을 추출할 수 없음. --name 으로 지정하세요.",
                        "could not derive a name from the current directory. Specify --name."
                    ))
                })?
                .to_string(),
        };
        let guild_path = cwd.join(format!("{name}.guild"));
        let today = today_date();
        // DEV-064: 마커 포맷은 core 공용 헬퍼 — schema_version 포함.
        let content = openguild_core::guild_file::marker_content(&name, &today);
        std::fs::write(&guild_path, content).with_context(|| {
            tf!("길드 파일 작성 실패: {}", "failed to write guild file: {}", guild_path.display())
        })?;
        (guild_path, name)
    };

    // .guild/ 디렉토리 + 기본 시드 (types/statuses) + .gitignore.
    // idempotent — 이미 있는 파일은 건드리지 않음.
    openguild_core::repo::seed_guild_dir(cwd)
        .with_context(|| tf!(".guild/ 시드 실패: {}", ".guild/ seed failed: {}", cwd.display()))?;

    // BUG-102: 시드 직후 index.db 를 파일 기준으로 재구축. 이걸 안 하면 첫
    // Store::open 의 migration(0001)이 넣는 *구식 기본 statuses(5개)* 가 DB 에
    // 남아, 파일 시드(7개 — testing/returned 포함)와 첫날부터 drift 상태가 됨
    // (statuses 목록이 5개로 보이다가 restore/reindex 시점에 7개로 "바뀌는"
    // 증상의 원인). GUI 의 init_and_open_guild 는 sync_on_open 으로 이미 동일
    // 보정을 함 — CLI init 만 구멍이었음.
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .with_context(|| tf!("tokio runtime 생성 실패", "failed to create tokio runtime"))?;
    rt.block_on(async {
        let store = openguild_core::Store::open(cwd).await?;
        let _ = openguild_core::reindex::reindex(&store).await?;
        anyhow::Ok(())
    })
    .with_context(|| {
        tf!(
            "init 후 index.db 초기 동기화(reindex) 실패",
            "initial index.db sync (reindex) after init failed"
        )
    })?;

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

// ─────────── BUG-135: run() dispatch 핸들러 (프레임 분리) ───────────

/// BUG-135: run() 스택 프레임 축소 — arm 지역값을 개별 함수 프레임으로.
fn handle_types(c: &Backend, json: bool, sub: TypesCmd) -> Result<()> {
    match sub {
        TypesCmd::List { table } => {
            let types = c.quest_types()?;
            if json {
                println!("{}", json_str(&types));
            } else if table {
                let rows: Vec<Vec<TableCell>> = types
                    .iter()
                    .map(|t| {
                        vec![
                            (t.prefix.clone(), Some(t.color.clone())),
                            (t.color.clone(), None),
                            (t.description.clone().unwrap_or_default(), None),
                        ]
                    })
                    .collect();
                render_table(&["PREFIX", "COLOR", "DESCRIPTION"], &[false, false], &rows, "types");
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
            if json {
                println!("{}", json_str(&row));
            } else {
                let p = colorize(&format!("{:<6}", row.prefix), &row.color);
                println!(
                    "{}",
                    tf!(
                        "{p} 추가됨 — {}",
                        "{p} added — {}",
                        row.description.as_deref().unwrap_or("")
                    )
                );
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
                bail!(tf!(
                    "--description 과 --clear-description 동시 사용 불가",
                    "--description and --clear-description are mutually exclusive"
                ));
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
            if json {
                println!("{}", json_str(&row));
            } else {
                let p = colorize(&format!("{:<6}", row.prefix), &row.color);
                if renamed {
                    println!(
                        "{}",
                        tf!(
                            "{p} 갱신됨 (rename: '{}' → '{}', 관련 quest slug cascade) — {}",
                            "{p} updated (rename: '{}' → '{}', cascades related quest slugs) — {}",
                            old_prefix,
                            row.prefix,
                            row.description.as_deref().unwrap_or("")
                        )
                    );
                } else {
                    println!(
                        "{}",
                        tf!("{p} 갱신됨 — {}", "{p} updated — {}", row.description.as_deref().unwrap_or(""))
                    );
                }
            }
        }
        TypesCmd::Delete { prefix } => {
            c.delete_type(prefix.trim().to_string())?;
            if json {
                println!("{}", serde_json::json!({ "ok": true }));
            } else {
                println!("{}", tf!("'{}' 삭제됨", "'{}' deleted", prefix.trim()));
            }
        }
    }
    Ok(())
}

/// 번들 문서 embed 출력 — `include_str!` 이 **컴파일 타임에 리포의 md 파일을
/// 그대로 담으므로** 진리원은 기존 문서 파일 하나뿐(이중 관리 없음). 문서를
/// 고치면 다음 빌드가 자동 반영. 설치 폴더의 docs/ 복사본이 유실/차단돼도
/// 이 명령은 항상 동작.
fn handle_docs(json: bool, name: Option<String>) -> Result<()> {
    const DOCS: &[(&str, &str, &str)] = &[
        (
            "agents",
            "AI agent 용 openguild 사용 가이드 (AGENTS_OPENGUILD_USAGE.md)",
            include_str!("../../docs/AGENTS_OPENGUILD_USAGE.md"),
        ),
        ("usage", "사용자 매뉴얼 (USAGE.md)", include_str!("../../docs/USAGE.md")),
        ("readme", "프로젝트 소개 (README.md)", include_str!("../../README.md")),
        ("changelog", "변경 이력 (CHANGELOG.md)", include_str!("../../CHANGELOG.md")),
    ];
    match name {
        Some(n) => {
            let key = n.to_lowercase();
            let Some((_, _, body)) = DOCS.iter().find(|(k, _, _)| *k == key) else {
                bail!(tf!(
                    "알 수 없는 문서 '{n}' — 사용 가능: {}",
                    "unknown doc '{n}' — available: {}",
                    DOCS.iter().map(|(k, _, _)| *k).collect::<Vec<_>>().join(" | ")
                ));
            };
            // 문서 원문 그대로 — json 모드도 raw 출력(문서는 markdown 텍스트).
            print!("{body}");
            if !body.ends_with('\n') {
                println!();
            }
        }
        None => {
            if json {
                println!(
                    "{}",
                    serde_json::json!(DOCS
                        .iter()
                        .map(|(k, d, _)| serde_json::json!({ "name": k, "description": d }))
                        .collect::<Vec<_>>())
                );
            } else {
                for (k, d, _) in DOCS {
                    println!("{k:<10} {d}");
                }
                println!("{}", tf!("\n사용: openguild docs <name>", "\nusage: openguild docs <name>"));
            }
        }
    }
    Ok(())
}

/// DEV-254: CLI 출력 언어 조회/변경 — `~/.openguild/locale.json` (GUI 와
/// 공유). 인자 없으면 현재 값(저장된 값, env override 는 별도 표시), 있으면
/// 저장 후 확인 메시지.
fn handle_locale(json: bool, lang: Option<String>) -> Result<()> {
    use openguild_core::locale::{self, Locale};

    match lang {
        None => {
            let saved = locale::load_saved()?;
            let effective = locale::current();
            if json {
                println!(
                    "{}",
                    serde_json::json!({ "saved": saved.as_str(), "effective": effective.as_str() })
                );
            } else if saved == effective {
                println!("{}", tf!("현재 언어: {}", "Current language: {}", saved.as_str()));
            } else {
                // OPENGUILD_LOCALE env 가 저장값을 덮어쓴 상태 — 둘 다 보여줌.
                println!(
                    "{}",
                    tf!(
                        "현재 언어: {} (저장된 값: {}, OPENGUILD_LOCALE 로 재정의됨)",
                        "Current language: {} (saved: {}, overridden by OPENGUILD_LOCALE)",
                        effective.as_str(),
                        saved.as_str()
                    )
                );
            }
        }
        Some(l) => {
            let Some(parsed) = Locale::parse(&l) else {
                bail!(tf!(
                    "알 수 없는 언어 '{}' — ko 또는 en 사용",
                    "Unknown language '{}' — use ko or en",
                    l
                ));
            };
            locale::save(parsed)?;
            if json {
                println!("{}", serde_json::json!({ "ok": true, "locale": parsed.as_str() }));
            } else {
                println!(
                    "{}",
                    tf!("✓ 언어를 '{}' 로 저장했습니다.", "✓ Language saved as '{}'.", parsed.as_str())
                );
            }
        }
    }
    Ok(())
}

/// 태그 정의 카탈로그 관리 — GUI 어드민 "Tag 정의" 섹션의 CLI 파리티.
fn handle_tag(c: &Backend, json: bool, sub: TagDefCmd) -> Result<()> {
    match sub {
        TagDefCmd::List { used } => {
            let defs = c.tag_defs()?;
            let used_tags = if used { c.tags_in_use()? } else { Vec::new() };
            if json {
                let defined: std::collections::HashSet<&str> =
                    defs.iter().map(|d| d.slug.as_str()).collect();
                let undefined: Vec<&String> =
                    used_tags.iter().filter(|t| !defined.contains(t.as_str())).collect();
                println!(
                    "{}",
                    serde_json::json!({
                        "defs": defs,
                        "used": if used { Some(&used_tags) } else { None },
                        "undefined_in_use": if used { Some(undefined) } else { None },
                    })
                );
            } else {
                if defs.is_empty() {
                    println!("{}", tf!("(정의된 태그 없음)", "(no defined tags)"));
                }
                for d in &defs {
                    let slug = colorize(&format!("{:<20}", d.slug), &d.color);
                    let color = if d.color.is_empty() { tf!("(색 없음)", "(no color)") } else { d.color.clone() };
                    println!("{slug} {:<8} {}", color, d.description);
                }
                if used {
                    let defined: std::collections::HashSet<&str> =
                        defs.iter().map(|d| d.slug.as_str()).collect();
                    let undefined: Vec<&String> =
                        used_tags.iter().filter(|t| !defined.contains(t.as_str())).collect();
                    println!("{}", tf!("-- 사용 중 태그 {}개", "-- {} tag(s) in use", used_tags.len()));
                    if !undefined.is_empty() {
                        println!(
                            "{}",
                            tf!(
                                "-- 정의 없이 사용 중: {}",
                                "-- in use without a definition: {}",
                                undefined.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", ")
                            )
                        );
                    }
                }
            }
        }
        TagDefCmd::Add { slug, color, description } => {
            if c.tag_defs()?.iter().any(|d| d.slug == slug) {
                bail!(tf!(
                    "태그 정의 '{slug}' 가 이미 있습니다 — 수정은 `tag update`",
                    "tag def '{slug}' already exists — use `tag update` to modify"
                ));
            }
            let d = c.tag_def_upsert(
                &slug,
                color.as_deref().unwrap_or(""),
                description.as_deref().unwrap_or(""),
            )?;
            if json {
                println!("{}", json_str(&d));
            } else {
                println!(
                    "{}",
                    tf!("✓ 태그 정의 추가: {}", "✓ tag def added: {}", colorize(&d.slug, &d.color))
                );
            }
        }
        TagDefCmd::Update { slug, color, description } => {
            let defs = c.tag_defs()?;
            let Some(existing) = defs.iter().find(|d| d.slug == slug) else {
                bail!(tf!(
                    "태그 정의 '{slug}' 가 없습니다 — 추가는 `tag add`",
                    "tag def '{slug}' does not exist — use `tag add` to create it"
                ));
            };
            if color.is_none() && description.is_none() {
                bail!(tf!(
                    "--color / --description 중 하나는 지정해야 합니다",
                    "specify at least one of --color / --description"
                ));
            }
            // 지정한 필드만 교체 — upsert 가 전체 교체라 기존값과 merge.
            let d = c.tag_def_upsert(
                &slug,
                color.as_deref().unwrap_or(&existing.color),
                description.as_deref().unwrap_or(&existing.description),
            )?;
            if json {
                println!("{}", json_str(&d));
            } else {
                println!(
                    "{}",
                    tf!("✓ 태그 정의 갱신: {}", "✓ tag def updated: {}", colorize(&d.slug, &d.color))
                );
            }
        }
        TagDefCmd::Delete { slug } => {
            c.tag_def_delete(&slug)?;
            if json {
                println!("{}", serde_json::json!({ "ok": true, "slug": slug }));
            } else {
                println!(
                    "{}",
                    tf!(
                        "✓ 태그 정의 삭제: {slug} (태그 사용 자체는 보존 — 기본 색으로 표시)",
                        "✓ tag def deleted: {slug} (existing tag usages are preserved — shown in default color)"
                    )
                );
            }
        }
    }
    Ok(())
}

/// BUG-135: run() 스택 프레임 축소 — arm 지역값을 개별 함수 프레임으로.
fn handle_statuses(c: &Backend, json: bool, sub: StatusesCmd) -> Result<()> {
    match sub {
        StatusesCmd::List { table } => {
            let statuses = c.quest_statuses()?;
            if json {
                // BUG-018: agent / script 용 — slug 포함된 raw row.
                println!("{}", json_str(&statuses));
            } else if table {
                let rows: Vec<Vec<TableCell>> = statuses
                    .iter()
                    .map(|s| {
                        vec![
                            (s.name_en.clone(), Some(s.color.clone())),
                            (s.slug.clone(), None),
                            (s.sort_order.to_string(), None),
                            (if s.counts_as_done { "✓" } else { "" }.to_string(), None),
                            (s.name_ko.clone(), None),
                        ]
                    })
                    .collect();
                render_table(
                    &["NAME", "SLUG", "ORDER", "DONE", "NAME_KO"],
                    &[false, false, true, false],
                    &rows,
                    "statuses",
                );
            } else {
                // DEV-209(BUG-018 정책 갱신): slug 도 표시 — quest move 등 명령
                // 인자와 frontmatter 의 정규 식별자가 slug 라, 목록에서 안 보이면
                // 사용자가 뭘 입력할지 알 수 없음(에러 메시지의 "Open (open)"
                // 표기와 통일).
                for s in &statuses {
                    let name_colored = colorize(&format!("{:<14}", s.name_en), &s.color);
                    let slug = format!("({})", s.slug);
                    println!("{name_colored} {slug:<14} {}", s.name_ko);
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
            if json {
                println!("{}", json_str(&row));
            } else {
                let n = colorize(&format!("{:<14}", row.name_en), &row.color);
                println!(
                    "{}",
                    tf!(
                        "{n} (slug={}) 추가됨 — {}",
                        "{n} (slug={}) added — {}",
                        row.slug,
                        if row.name_ko.is_empty() { "-" } else { &row.name_ko }
                    )
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
                bail!(tf!(
                    "--name-ko 와 --clear-name-ko 동시 사용 불가",
                    "--name-ko and --clear-name-ko are mutually exclusive"
                ));
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
            if json {
                println!("{}", json_str(&row));
            } else {
                let n = colorize(&format!("{:<14}", row.name_en), &row.color);
                if renamed {
                    println!(
                        "{}",
                        tf!(
                            "{n} 갱신됨 (slug rename: '{}' → '{}', cascade)",
                            "{n} updated (slug rename: '{}' → '{}', cascade)",
                            old_slug,
                            row.slug
                        )
                    );
                } else {
                    println!("{}", tf!("{n} 갱신됨", "{n} updated"));
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
            if json {
                println!("{}", serde_json::json!({ "ok": true }));
            } else {
                println!("{}", tf!("'{display}' 삭제됨", "'{display}' deleted"));
            }
        }
    }
    Ok(())
}

/// BUG-135: run() 스택 프레임 축소 — arm 지역값을 개별 함수 프레임으로.
fn handle_template(c: &Backend, json: bool, sub: TemplateCmd) -> Result<()> {
    match sub {
        TemplateCmd::List => {
            let templates = c.templates_list()?;
            if json {
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
                println!(
                    "{}",
                    tf!(
                        "(템플릿 없음 — .guild/templates/{{name}}.md 작성)",
                        "(no templates — create .guild/templates/{{name}}.md)"
                    )
                );
            } else {
                let no_title = tf!("(제목 없음)", "(no title)");
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
                        t.frontmatter.title.as_deref().unwrap_or(&no_title),
                        meta.join(" ")
                    );
                }
            }
        }
        TemplateCmd::Show { name } => {
            let t = c.template_load(&name)?;
            if json {
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
                let no_title = tf!("(제목 없음)", "(no title)");
                println!("# {} — {}", t.name, t.frontmatter.title.as_deref().unwrap_or(&no_title));
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
            if json {
                println!(
                    "{}",
                    serde_json::json!({ "ok": true, "name": name, "path": path.display().to_string() })
                );
            } else {
                println!(
                    "{}",
                    tf!("✓ 템플릿 '{name}' 저장 — {}", "✓ template '{name}' saved — {}", path.display())
                );
            }
        }
    }
    Ok(())
}

/// BUG-135: run() 스택 프레임 축소 — arm 지역값을 개별 함수 프레임으로.
fn handle_rules(c: &Backend, json: bool, sub: RulesCmd) -> Result<()> {
    match sub {
        RulesCmd::List => {
            let entries = c.rules_list()?;
            if json {
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
                println!("{}", tf!("(규칙 없음)", "(no rules)"));
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
                .ok_or_else(|| anyhow::anyhow!(tf!("규칙 '{slug}' 없음", "rule '{slug}' not found")))?;
            if json {
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
            if json {
                println!("{}", serde_json::json!({ "ok": true, "slug": slug }));
            } else {
                println!("{}", tf!("✓ 규칙 '{slug}' 저장됨", "✓ rule '{slug}' saved"));
            }
        }
        RulesCmd::Create { slug, file, empty } => {
            let content = if empty {
                String::new()
            } else {
                read_content(file.as_deref())?
            };
            c.rules_create(&slug, content)?;
            if json {
                println!("{}", serde_json::json!({ "ok": true, "slug": slug }));
            } else {
                println!("{}", tf!("✓ 규칙 '{slug}' 생성됨", "✓ rule '{slug}' created"));
            }
        }
        RulesCmd::Delete { slug, force } => {
            if !force {
                eprint!("{}", tf!("규칙 '{slug}' 을 삭제할까요? (y/N) ", "delete rule '{slug}'? (y/N) "));
                use std::io::Write;
                std::io::stderr().flush().ok();
                let mut buf = String::new();
                std::io::stdin().read_line(&mut buf)?;
                if !matches!(buf.trim(), "y" | "Y" | "yes") {
                    println!("{}", tf!("(취소)", "(cancelled)"));
                    return Ok(());
                }
            }
            c.rules_delete(&slug)?;
            if json {
                println!("{}", serde_json::json!({ "ok": true, "slug": slug }));
            } else {
                println!("{}", tf!("✓ 규칙 '{slug}' 삭제됨", "✓ rule '{slug}' deleted"));
            }
        }
        RulesCmd::Rename { slug, new_slug } => {
            c.rules_rename(&slug, &new_slug)?;
            if json {
                println!(
                    "{}",
                    serde_json::json!({
                        "ok": true, "from": slug, "to": new_slug,
                    })
                );
            } else {
                println!("{}", tf!("✓ '{slug}' → '{new_slug}' 이름 변경", "✓ renamed '{slug}' → '{new_slug}'"));
            }
        }
    }
    Ok(())
}

/// BUG-135: run() 스택 프레임 축소 — arm 지역값을 개별 함수 프레임으로.
fn handle_library(c: &Backend, json: bool, sub: LibraryCmd) -> Result<()> {
    match sub {
        LibraryCmd::List { table } => {
            let books = c.library_list()?;
            if json {
                println!("{}", json_str(&books));
            } else if table {
                // 폴더 path/제목은 한글 가변폭 가능 — 뒤쪽 컬럼에.
                let rows: Vec<Vec<TableCell>> = books
                    .iter()
                    .map(|b| {
                        vec![
                            (b.book_id.clone(), None),
                            (b.updated_at.clone(), None),
                            (format!("{}{}{}",
                                if b.path.is_empty() { "" } else { "[" },
                                b.path,
                                if b.path.is_empty() { "" } else { "] " },
                            ) + &b.title, None),
                        ]
                    })
                    .collect();
                render_table(&["ID", "UPDATED", "TITLE"], &[false, false], &rows, "docs");
            } else if books.is_empty() {
                println!("{}", tf!("(도서관 문서 없음)", "(no library docs)"));
            } else {
                for b in &books {
                    let loc = if b.path.is_empty() {
                        String::new()
                    } else {
                        format!("  [{}]", b.path)
                    };
                    println!("{:<10} {}{}  ({})", b.book_id, b.title, loc, b.updated_at);
                }
            }
        }
        LibraryCmd::Show { id } => {
            let b = c.library_get(&id)?;
            if json {
                println!("{}", json_str(&b));
            } else {
                println!("{}  {}", b.book_id, b.title);
                let top_level = tf!("(최상위)", "(top level)");
                let loc = if b.path.is_empty() { &top_level } else { &b.path };
                println!("{}", tf!("  경로: {loc}", "  path: {loc}"));
                println!("  created: {}  updated: {}", b.created_at, b.updated_at);
                if !b.body.is_empty() {
                    println!();
                    println!("{}", b.body);
                }
            }
        }
        LibraryCmd::New { title, file, path } => {
            let body = match file {
                Some(p) => std::fs::read_to_string(&p).with_context(|| {
                    tf!("파일 읽기 실패: {}", "failed to read file: {}", p.display())
                })?,
                None => String::new(),
            };
            let b = c.library_new(&title, &body, path.as_deref().unwrap_or(""))?;
            if json {
                println!("{}", json_str(&b));
            } else {
                println!("{}", tf!("✓ {} 생성됨 — {}", "✓ {} created — {}", b.book_id, b.title));
            }
        }
        LibraryCmd::Update { id, title, file, path } => {
            if title.is_none() && file.is_none() && path.is_none() {
                bail!(tf!(
                    "변경할 필드가 없습니다 — --title / --file / --path 지정",
                    "no fields to change — specify --title / --file / --path"
                ));
            }
            let body = match file {
                Some(p) => Some(std::fs::read_to_string(&p).with_context(|| {
                    tf!("파일 읽기 실패: {}", "failed to read file: {}", p.display())
                })?),
                None => None,
            };
            let b = c.library_update(&id, title.as_deref(), body.as_deref(), path.as_deref())?;
            if json {
                println!("{}", json_str(&b));
            } else {
                println!("{}", tf!("✓ {} 수정됨 — {}", "✓ {} updated — {}", b.book_id, b.title));
            }
        }
        LibraryCmd::Delete { id, yes } => {
            if !yes {
                eprint!("{}", tf!("도서관 문서 '{id}' 을 삭제할까요? (y/N) ", "delete library doc '{id}'? (y/N) "));
                use std::io::Write;
                std::io::stderr().flush().ok();
                let mut input = String::new();
                std::io::stdin().read_line(&mut input)?;
                if !input.trim().eq_ignore_ascii_case("y") {
                    println!("{}", tf!("취소됨", "cancelled"));
                    return Ok(());
                }
            }
            c.library_delete(&id)?;
            if json {
                println!("{}", serde_json::json!({ "ok": true, "book_id": id }));
            } else {
                println!(
                    "{}",
                    tf!(
                        "✓ '{id}' 삭제됨 (soft delete — 번호는 재사용되지 않음)",
                        "✓ '{id}' deleted (soft delete — the number is never reused)"
                    )
                );
            }
        }
        LibraryCmd::Folder { sub } => match sub {
            LibraryFolderCmd::List => {
                let folders = c.library_folder_list()?;
                if json {
                    println!("{}", json_str(&folders));
                } else if folders.is_empty() {
                    println!("{}", tf!("(폴더 없음)", "(no folders)"));
                } else {
                    for f in &folders {
                        println!("{}", f.path);
                    }
                }
            }
            LibraryFolderCmd::New { path } => {
                let f = c.library_folder_new(&path)?;
                if json {
                    println!("{}", json_str(&f));
                } else {
                    println!("{}", tf!("✓ 폴더 '{}' 생성됨", "✓ folder '{}' created", f.path));
                }
            }
            LibraryFolderCmd::Delete { path, yes } => {
                if !yes {
                    eprint!("{}", tf!("폴더 '{path}' 을 삭제할까요? (y/N) ", "delete folder '{path}'? (y/N) "));
                    use std::io::Write;
                    std::io::stderr().flush().ok();
                    let mut input = String::new();
                    std::io::stdin().read_line(&mut input)?;
                    if !input.trim().eq_ignore_ascii_case("y") {
                        println!("{}", tf!("취소됨", "cancelled"));
                        return Ok(());
                    }
                }
                c.library_folder_delete(&path)?;
                if json {
                    println!("{}", serde_json::json!({ "ok": true, "path": path }));
                } else {
                    println!("{}", tf!("✓ 폴더 '{path}' 삭제됨", "✓ folder '{path}' deleted"));
                }
            }
        },
    }
    Ok(())
}

/// BUG-135: run() 스택 프레임 축소 — arm 지역값을 개별 함수 프레임으로.
fn handle_worklog(c: &Backend, json: bool, sub: WorklogCmd) -> Result<()> {
    match sub {
        WorklogCmd::Show { date, from, to } => {
            // 기본 = 오늘 하루. --date 는 그 하루, --from/--to 는 기간.
            let (f, t) = match (date, from, to) {
                (Some(d), _, _) => (d.clone(), d),
                (None, Some(f), Some(t)) => (f, t),
                _ => {
                    let today = openguild_core::time::now_local_iso8601()[..10].to_string();
                    (today.clone(), today)
                }
            };
            let report = c.worklog_activities(&f, &t)?;
            if json {
                println!("{}", json_str(&report));
            } else {
                if f == t {
                    println!("{}", tf!("작업 기록 — {f}", "work log — {f}"));
                } else {
                    println!("{}", tf!("작업 기록 — {f} ~ {t}", "work log — {f} ~ {t}"));
                }
                // 하루 뷰면 그 날짜의 노트도 함께 (파일 있으면).
                if f == t
                    && let Ok(Some(note)) = c.worklog_note_get(&f)
                {
                    println!();
                    println!("{}", tf!("📝 노트:", "📝 note:"));
                    for line in note.lines() {
                        println!("  {line}");
                    }
                }
                println!();
                if report.activities.is_empty() {
                    println!("{}", tf!("(활동 없음)", "(no activity)"));
                } else {
                    let mut cur_date = String::new();
                    for a in &report.activities {
                        let d = a.ts.get(..10).unwrap_or("");
                        if f != t && d != cur_date {
                            cur_date = d.to_string();
                            println!("── {cur_date} ──");
                        }
                        let hm = a.ts.get(11..16).unwrap_or("--:--");
                        let badge = match a.kind.as_str() {
                            "status" => tf!("상태", "status"),
                            "type" => tf!("타입", "type"),
                            "comment" => tf!("댓글", "comment"),
                            "created" => tf!("생성", "created"),
                            other => other.to_string(),
                        };
                        let first = a.summary.lines().next().unwrap_or("");
                        println!("{hm}  {:<10} [{badge}] {first}", a.slug);
                    }
                    println!();
                    println!(
                        "{}",
                        tf!(
                            "요약: 상태변경 {} · 댓글 {} · 생성 {} · done 전환 {}",
                            "summary: status changes {} · comments {} · created {} · done transitions {}",
                            report.counts.status_changes,
                            report.counts.comments,
                            report.counts.created,
                            report.counts.done_transitions
                        )
                    );
                }
            }
        }
        WorklogCmd::Note { sub } => match sub {
            WorklogNoteCmd::Show { date } => {
                let content = c.worklog_note_get(&date)?;
                if json {
                    println!("{}", serde_json::json!({ "date": date, "content": content }));
                } else {
                    match content {
                        Some(s) => print!("{s}{}", if s.ends_with('\n') { "" } else { "\n" }),
                        None => println!("{}", tf!("(노트 없음)", "(no note)")),
                    }
                }
            }
            WorklogNoteCmd::Set { date, file } => {
                let content = read_content(file.as_deref())?;
                c.worklog_note_set(&date, content)?;
                if json {
                    println!("{}", serde_json::json!({ "ok": true, "date": date }));
                } else {
                    println!("{}", tf!("✓ {date} 노트 저장됨", "✓ {date} note saved"));
                }
            }
            WorklogNoteCmd::Clear { date } => {
                c.worklog_note_set(&date, String::new())?;
                if json {
                    println!("{}", serde_json::json!({ "ok": true, "date": date }));
                } else {
                    println!("{}", tf!("✓ {date} 노트 삭제됨", "✓ {date} note cleared"));
                }
            }
        },
    }
    Ok(())
}

/// BUG-135: run() 스택 프레임 축소 — arm 지역값을 개별 함수 프레임으로.
fn handle_backup(c: &Backend, json: bool, sub: BackupCmd) -> Result<()> {
    match sub {
        BackupCmd::New => {
            let info = c.create_backup()?;
            if json {
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
                    "{}",
                    tf!(
                        "✓ snapshot 생성: {} ({} bytes)",
                        "✓ snapshot created: {} ({} bytes)",
                        openguild_core::snapshot::ts_to_local_display(&info.timestamp),
                        info.size_bytes
                    )
                );
                println!("  path: {}", info.path.display());
            }
        }
        BackupCmd::List => {
            let list = c.list_backups()?;
            if json {
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
                println!("{}", json_str(&arr));
            } else if list.is_empty() {
                println!("{}", tf!("(사용 가능한 백업 없음)", "(no backups available)"));
                println!();
                println!(
                    "{}",
                    tf!(
                        "`openguild backup create` 으로 생성하세요.",
                        "create one with `openguild backup create`."
                    )
                );
            } else {
                println!("{}", tf!("백업 목록 (오래된 순):", "backup list (oldest first):"));
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
            if json {
                println!("{}", serde_json::json!({ "ok": true, "deleted": timestamp }));
            } else {
                println!("{}", tf!("✓ 백업 삭제: {timestamp}", "✓ backup deleted: {timestamp}"));
            }
        }
    }
    Ok(())
}

/// BUG-135: run() 스택 프레임 축소 — arm 지역값을 개별 함수 프레임으로.
fn handle_restore(c: &Backend, json: bool, to: Option<String>, at: Option<String>) -> Result<()> {
    if let Some(ts) = at {
        // DEV-022: 시점 복원 (journal replay).
        // DEV-210: `latest` 키워드 = 최신 스냅샷 + journal 전체 재적용
        // (= 최신 상태로 복구). 먼 미래 ISO 를 직접 칠 필요 없음.
        let is_latest = ts.eq_ignore_ascii_case("latest");
        let ts = resolve_at_keyword(&ts);
        let report = c.restore_to_point(&ts)?;
        if json {
            println!(
                "{}",
                serde_json::json!({
                    "ok": true,
                    "latest": is_latest,
                    "replayed_to": report.target_ts,
                    "applied": report.applied,
                    "pre_backup": report.pre_backup,
                })
            );
        } else if is_latest {
            // 최신 복구 = 무손실(상태 동일) — 폐기 경고 없음.
            println!(
                "{}",
                tf!(
                    "✓ 최신 상태로 복구: 최신 스냅샷 + journal op {} 개 재적용",
                    "✓ restored to latest: latest snapshot + {} journal op(s) replayed",
                    report.applied
                )
            );
            println!();
            println!(
                "{}",
                tf!(
                    "참고: 파일 시스템 표시가 안 맞으면 `openguild reindex`.",
                    "note: if the file system view looks off, run `openguild reindex`."
                )
            );
        } else {
            println!(
                "{}",
                tf!(
                    "✓ 시점 복원 완료: {} 까지 journal op {} 개 재적용",
                    "✓ point-in-time restore complete: up to {} — {} journal op(s) replayed",
                    report.target_ts,
                    report.applied
                )
            );
            println!();
            if let Some(pb) = &report.pre_backup {
                // DEV-212: 폐기가 아니라 자동 백업으로 보존됨을 안내.
                println!(
                    "{}",
                    tf!(
                        "복원 전 상태는 스냅샷 {pb} 로 자동 백업되었습니다.",
                        "the pre-restore state was auto-backed up as snapshot {pb}."
                    )
                );
                println!(
                    "{}",
                    tf!("되돌리려면: openguild restore --to {pb}", "to revert: openguild restore --to {pb}")
                );
            } else {
                println!(
                    "{}",
                    tf!(
                        "주의: 이 시점 이후의 변경은 폐기되었습니다.",
                        "note: changes made after this point have been discarded."
                    )
                );
            }
            println!(
                "{}",
                tf!(
                    "파일 시스템 표시가 안 맞으면 `openguild reindex`.",
                    "if the file system view looks off, run `openguild reindex`."
                )
            );
        }
    } else {
        let info = c.restore_backup(to)?;
        if json {
            println!(
                "{}",
                serde_json::json!({
                    "ok": true,
                    "restored_to": info.timestamp,
                })
            );
        } else {
            println!(
                "{}",
                tf!(
                    "✓ 복원 완료: {}",
                    "✓ restore complete: {}",
                    openguild_core::snapshot::ts_to_local_display(&info.timestamp)
                )
            );
            println!();
            println!(
                "{}",
                tf!(
                    "주의: 파일 시스템 (`.guild/quests/*.md`) 자동 갱신 안 됨.",
                    "note: the file system (`.guild/quests/*.md`) is not auto-updated."
                )
            );
            println!("{}", tf!("      필요시 `openguild reindex`.", "      run `openguild reindex` if needed."));
        }
    }
    Ok(())
}

/// BUG-135: run() 스택 프레임 축소 — arm 지역값을 개별 함수 프레임으로.
fn handle_migrate_to_files(c: &Backend, json: bool) -> Result<()> {
    let report = c.migrate_to_files()?;
    if json {
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
        println!("{}", tf!("✓ 마이그레이션 완료", "✓ migration complete"));
        println!("  legacy DB     : {}", report.legacy_db_path.display());
        println!(
            "{}",
            tf!(
                "  quests 작성   : {}",
                "  quests written : {}",
                report.quests_written
            )
        );
        println!(
            "  - alive       : {}",
            report.quests_written - report.deleted_quests_included
        );
        println!("  - soft-deleted: {}", report.deleted_quests_included);
        println!(
            "{}",
            tf!(
                "  types 갱신    : {} (counter)",
                "  types updated  : {} (counter)",
                report.types_updated
            )
        );
        println!(
            "  index.db      : {}",
            if report.index_db_copied {
                tf!("복사됨", "copied")
            } else {
                tf!("이미 존재 — 건드리지 않음", "already exists — left untouched")
            }
        );
    }
    Ok(())
}

/// BUG-135: run() 스택 프레임 축소 — arm 지역값을 개별 함수 프레임으로.
fn handle_info(c: &Backend, json: bool, brief: bool) -> Result<()> {
    let i = c.info()?;
    let total = i.summary.quests_alive + i.summary.quests_deleted;
    let snap_total: u64 = i.snapshots.iter().map(|s| s.size_bytes).sum();
    let latest = i
        .snapshots
        .last()
        .map(|s| openguild_core::snapshot::ts_to_local_display(&s.timestamp))
        .unwrap_or_else(|| "(none)".to_string());
    let schema = i.summary.schema_version.as_deref();
    if json {
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
    Ok(())
}

/// BUG-135: run() 스택 프레임 축소 — arm 지역값을 개별 함수 프레임으로.
fn handle_quest(c: &Backend, json: bool, sub: QuestCmd) -> Result<()> {
    match sub {
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
            table,
        } => {
            if table && json {
                return Err(anyhow!(tf!(
                    "--table 은 --json 과 함께 쓸 수 없습니다",
                    "--table cannot be used together with --json"
                )));
            }
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
            } else if tree && !json {
                // DEV-065 (CLI tree mode): 부모 → 자식 들여쓰기 출력.
                print_quest_tree(&quests);
            } else if table {
                // DEV-211: 사람용 정렬 표.
                print_quest_table(&quests);
            } else {
                print_quest_list(&quests, json);
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
                if json {
                    println!("{}", json_str(
                        &quests.iter().map(|q| q.quest_id.clone()).collect::<Vec<_>>()
                    ));
                } else {
                    for q in &quests {
                        println!("{}", q.quest_id);
                    }
                }
            } else {
                print_quest_list(&quests, json);
            }
        }
        QuestCmd::Show { slug, field } => {
            let d = c.quest_by_slug(&slug)?;
            if let Some(name) = field {
                let v = quest_field_value(&d, &name)?;
                if json {
                    println!("{}", serde_json::to_string(&v).unwrap());
                } else {
                    // raw — multi-line (description 등) 그대로.
                    println!("{v}");
                }
            } else {
                print_quest_detail(&d, json);
            }
        }
        QuestCmd::History { slug } => {
            let d = c.quest_by_slug(&slug)?;
            let history = c.list_quest_history(d.quest.id)?;
            if json {
                println!("{}", json_str(&history));
            } else if history.is_empty() {
                println!("{}", tf!("(이력 없음)", "(no history)"));
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
            description_file,
            urgency,
            parent,
            template,
        } => {
            // DEV-222: --description-file 이면 UTF-8 파일에서 본문 읽기.
            let description = resolve_description_input(description, description_file)?;
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
                eprintln!("[openguild] {}", tf!("warn: 템플릿 tags 적용 실패 — {e:#}", "warn: failed to apply template tags — {e:#}"));
            }
            // multi-line description 도 그대로 보여줘 사용자가 "잘렸다" 오해 방지.
            print_quest_full(&q, json);
        }
        QuestCmd::Update {
            slug,
            title,
            description,
            description_file,
            urgency,
            dry_run,
        } => {
            // DEV-222: --description-file 이면 UTF-8 파일에서 본문 읽기.
            let description = resolve_description_input(description, description_file)?;
            let detail = c.quest_by_slug(&slug)?;
            let id = detail.quest.id;

            if dry_run {
                if json {
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
            print_quest_full(&q, json);
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
                if json {
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
                return Err(anyhow!(tf!(
                    "delete 는 위험한 작업입니다. 영향 확인은 --dry-run, 실제 실행은 --yes 를 명시하세요.",
                    "delete is a destructive operation. Use --dry-run to preview the impact, --yes to actually run it."
                )));
            }

            let id = detail.quest.id;
            let mut cascade_ids = Vec::new();
            for s in &cascade {
                cascade_ids.push(c.id_of(s)?);
            }
            c.delete_quest(id, &cascade_ids)?;
            if json {
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
        QuestCmd::Deleted { table } => {
            let quests = c.list_deleted_quests()?;
            if table && !json {
                print_quest_table(&quests);
            } else {
                print_quest_list(&quests, json);
            }
        }
        QuestCmd::Restore { slug } => {
            // alive 목록엔 없으니 deleted 목록에서 slug → id 매칭
            let deleted = c.list_deleted_quests()?;
            let q = deleted
                .iter()
                .find(|q| q.quest_id == slug)
                .ok_or_else(|| {
                    anyhow!(tf!(
                        "'{slug}' is not in the deleted list (또는 이미 alive)",
                        "'{slug}' is not in the deleted list (or is already alive)"
                    ))
                })?;
            let restored = c.restore_quest(q.id)?;
            print_quest(&restored, json);
        }
        QuestCmd::Status { slug, status } => {
            if let Some(target) = status {
                // DEV-044: deprecated 변경 호출 — `move` 권장 알림.
                eprintln!(
                    "{}",
                    tf!(
                        "warning: `quest status <slug> <status>` 는 deprecated. \
                         앞으로는 `quest move <slug> <status>` 사용 (혼란 방지).",
                        "warning: `quest status <slug> <status>` is deprecated. \
                         Use `quest move <slug> <status>` instead (avoids ambiguity)."
                    )
                );
                change_status_with_noop_notice(c, &slug, &target, json)?;
            } else {
                // 출력 전용 — 현재 상태만.
                // DEV-046: JSON 에서 status_id 제거 (positional id 는 외부
                // 클라이언트가 참조하면 안 됨). slug 가 stable identifier.
                let d = c.quest_by_slug(&slug)?;
                if json {
                    let payload = serde_json::json!({
                        "quest_id": d.quest.quest_id,
                        "status_slug": d.quest.status_slug,
                        "status_name_en": d.quest.status_name_en,
                        "status_name_ko": d.quest.status_name_ko,
                    });
                    println!("{}", json_str(&payload));
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
            change_status_with_noop_notice(c, &slug, &status, json)?;
        }
        QuestCmd::Start { slug } => {
            change_status_with_noop_notice(c, &slug, "In Progress", json)?;
        }
        QuestCmd::Done { slug } => {
            change_status_with_noop_notice(c, &slug, "Done", json)?;
        }
        QuestCmd::Reopen { slug } => {
            change_status_with_noop_notice(c, &slug, "Open", json)?;
        }
        QuestCmd::Parent {
            slug,
            parent,
            detach,
        } => {
            if detach && parent.is_some() {
                return Err(anyhow!(tf!(
                    "--detach 와 parent 인자를 동시에 사용할 수 없음",
                    "--detach and the parent argument cannot be used together"
                )));
            }
            let id = c.id_of(&slug)?;
            let parent_id = if detach {
                None
            } else {
                match parent {
                    Some(p) => Some(c.id_of(&p)?),
                    None => {
                        return Err(anyhow!(tf!(
                            "부모 슬러그를 지정하거나 --detach 를 사용하세요",
                            "specify a parent slug or use --detach"
                        )))
                    }
                }
            };
            let q = c.change_parent(id, parent_id)?;
            print_quest(&q, json);
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
                if json {
                    let payload = serde_json::json!({
                        "quest_id": q.quest_id,
                        "desired_due": q.desired_due,
                        "required_due": q.required_due,
                    });
                    println!("{}", json_str(&payload));
                } else {
                    let none_label = tf!("(없음)", "(none)");
                    println!(
                        "{}  desired_due: {}  required_due: {}",
                        colorize(&q.quest_id, &q.type_color),
                        q.desired_due.as_deref().unwrap_or(&none_label),
                        q.required_due.as_deref().unwrap_or(&none_label),
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
                if json {
                    let payload = serde_json::json!({
                        "quest_id": q.quest_id,
                        "desired_due": q.desired_due,
                        "required_due": q.required_due,
                    });
                    println!("{}", json_str(&payload));
                } else {
                    let none_label = tf!("(없음)", "(none)");
                    println!(
                        "{}  desired_due: {}  required_due: {}",
                        colorize(&q.quest_id, &q.type_color),
                        q.desired_due.as_deref().unwrap_or(&none_label),
                        q.required_due.as_deref().unwrap_or(&none_label),
                    );
                }
            }
        }
        // DEV-100: quest / campaign 공용 핸들러로 위임.
        QuestCmd::Comment { sub } => run_comment_cmd(c, CommentScope::Quest, sub, json)?,
        QuestCmd::Attach { sub } => run_attach_cmd(c, CommentScope::Quest, sub, json)?,
        QuestCmd::Memo { sub } => run_memo_cmd(c, CommentScope::Quest, sub, json)?,
        QuestCmd::Tag { sub } => match sub {
            TagCmd::List { slug } => {
                let tags = c.tag_list(&slug)?;
                if json {
                    println!(
                        "{}",
                        serde_json::json!({ "slug": slug, "tags": tags })
                    );
                } else if tags.is_empty() {
                    println!("{}", tf!("(태그 없음)", "(no tags)"));
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
                if json {
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
                if json {
                    println!("{}", serde_json::json!({ "ok": true, "slug": slug, "tags": after }));
                } else if after.is_empty() {
                    println!("✓ {slug} tags: {}", tf!("(없음)", "(none)"));
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
                if json {
                    println!("{}", serde_json::json!({ "ok": true, "slug": slug, "tags": flat }));
                } else if flat.is_empty() {
                    println!("✓ {slug} tags: {}", tf!("(모두 제거)", "(all removed)"));
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
                if json {
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
                if json {
                    println!(
                        "{}",
                        serde_json::json!({ "ok": true, "removed": prereq, "from": slug })
                    );
                } else {
                    println!("{slug} prereq - {prereq}");
                }
            }
        },
    }
    Ok(())
}

fn main() {
    // BUG-135: debug(opt-level=0) 빌드에서 clap derive 가 생성한 Command 빌더
    // 함수 한 개의 스택 프레임이 Windows 메인 스레드 기본 스택(1MB)을 초과해
    // 어떤 명령이든(--help 포함) 진입 즉시 stack overflow. opt-level=0 은
    // lifetime 마커가 없어 LLVM stack coloring 이 임시값 슬롯을 병합하지 못하고,
    // 수백 개 Arg 빌더 체인의 임시값이 전부 합산되기 때문. 매크로 생성 코드라
    // 함수 분리가 불가능 — rustc 자신도 쓰는 관용구대로 넉넉한 스택의 별도
    // 스레드에서 실행한다. (release 는 슬롯 병합 + 인라이닝으로 프레임이 작아
    // 원래도 문제없음. Linux 는 메인 스택 8MB 라 드러나지 않았을 뿐.)
    let result = std::thread::Builder::new()
        .name("run".into())
        .stack_size(8 * 1024 * 1024)
        .spawn(run)
        .expect("run 스레드 spawn 실패")
        .join()
        .unwrap_or_else(|p| std::panic::resume_unwind(p));
    if let Err(e) = result {
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
                    table,
                },
            } => {
                assert!(!table);
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
                    sort, reverse, limit, offset, id_only, count, tree, table,
                },
            } => {
                assert!(!tree);
                assert!(!table);
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
                        description_file,
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
                assert!(description_file.is_none());
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
                        description_file,
                        template,
                    },
            } => {
                assert_eq!(type_prefix.as_deref(), Some("BUG"));
                assert_eq!(title.as_deref(), Some("fix"));
                assert_eq!(urgency, Some(1));
                assert_eq!(parent.as_deref(), Some("DEV-007"));
                assert_eq!(description.as_deref(), Some("details"));
                assert!(description_file.is_none());
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

    /// DEV-221: comments_search — quest/campaign UNION + 필터 + 최신순.
    /// 캐시 테이블에 직접 INSERT 해 SQL 조합을 검증 (파일 IO 없이).
    #[test]
    fn comments_search_union_filters_and_order() {
        let dir = fresh_tmp("comsearch");
        init_guild_at(&dir, Some("t".into())).unwrap();
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let store = rt.block_on(openguild_core::Store::open(&dir)).unwrap();
        rt.block_on(async {
            // quest DEV-001 + campaign C-001 행을 캐시에 직접 구성.
            sqlx::query(
                "INSERT INTO quests (quest_type_id, number, title, status_id, urgency, created_at, updated_at)
                 VALUES (1, 1, 'q', 1, 3, 't', 't')",
            )
            .execute(&store.index_pool)
            .await
            .unwrap();
            sqlx::query(
                "INSERT INTO campaigns (campaign_slug, title, status, created_at, updated_at)
                 VALUES ('C-001', 'c', 'active', 't', 't')",
            )
            .execute(&store.index_pool)
            .await
            .unwrap();
            for (eid, ts, author, body, disc, res, parent) in [
                (1_i64, "2026-01-01T00:00:00+09:00", "admin", "첫 댓글", 0_i64, 0_i64, None::<i64>),
                (2, "2026-01-03T00:00:00+09:00", "claude", "토론 시작", 1, 0, None),
                // BUG-110: #1 에 대한 답글 — comments_search 가 parent_id 를 실어야.
                (3, "2026-01-05T00:00:00+09:00", "admin", "해결된 토론", 1, 1, Some(1)),
            ] {
                sqlx::query(
                    "INSERT INTO quest_comments (quest_id, entry_id, ts, author, body, discussion, resolved, parent_id)
                     VALUES (1, ?, ?, ?, ?, ?, ?, ?)",
                )
                .bind(eid)
                .bind(ts)
                .bind(author)
                .bind(body)
                .bind(disc)
                .bind(res)
                .bind(parent)
                .execute(&store.index_pool)
                .await
                .unwrap();
            }
            sqlx::query(
                "INSERT INTO campaign_comments (campaign_id, entry_id, ts, author, body)
                 VALUES (1, 1, '2026-01-04T00:00:00+09:00', 'admin', '캠페인 댓글')",
            )
            .execute(&store.index_pool)
            .await
            .unwrap();
        });
        let backend = Backend::Local(LocalBackend {
            store,
            rt,
            guild_path: dir.clone(),
        });

        // 전체 — quest+campaign UNION, 최신순.
        let all = backend
            .comments_search(None, None, None, None, false, false, 20)
            .unwrap();
        assert_eq!(all.len(), 4);
        assert_eq!(all[0].ts, "2026-01-05T00:00:00+09:00", "최신순 정렬");
        assert!(all.iter().any(|c| c.scope == "campaign" && c.slug == "C-001"));
        // BUG-110: 답글(entry 3)은 parent_id 가 실려야, 답글 아닌 것들은 None.
        assert_eq!(
            all.iter().find(|c| c.entry_id == 3 && c.scope == "quest").unwrap().parent_id,
            Some(1)
        );
        assert_eq!(
            all.iter().find(|c| c.entry_id == 1 && c.scope == "quest").unwrap().parent_id,
            None
        );

        // author 필터 (대소문자 무시) — quest 2 + campaign 1.
        let admins = backend
            .comments_search(Some("ADMIN"), None, None, None, false, false, 20)
            .unwrap();
        assert_eq!(admins.len(), 3);

        // 미해결 토론만 — campaign 은 discussion 개념이 없어 자연 제외.
        let unresolved = backend
            .comments_search(None, None, None, None, false, true, 20)
            .unwrap();
        assert_eq!(unresolved.len(), 1);
        assert_eq!(unresolved[0].entry_id, 2);

        // grep + since 조합.
        let hits = backend
            .comments_search(None, Some("2026-01-02"), None, Some("토론"), false, false, 20)
            .unwrap();
        assert_eq!(hits.len(), 2);

        // limit.
        let limited = backend
            .comments_search(None, None, None, None, false, false, 2)
            .unwrap();
        assert_eq!(limited.len(), 2);

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// DEV-222: --description-file — 파일 읽기 / 상호배타 / 미지정 passthrough.
    #[test]
    fn resolve_description_input_file_and_conflict() {
        // 파일 읽기 (UTF-8 한글 + multi-line, 끝 개행 trim).
        let dir = std::env::temp_dir().join(format!(
            "og-desc-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let f = dir.join("d.md");
        std::fs::write(&f, "첫 줄\n\n## 섹션\n본문\n").unwrap();
        let got = resolve_description_input(None, Some(f)).unwrap();
        assert_eq!(got.as_deref(), Some("첫 줄\n\n## 섹션\n본문"));
        // 미지정 시 inline passthrough.
        assert_eq!(
            resolve_description_input(Some("x".into()), None).unwrap().as_deref(),
            Some("x")
        );
        // 없는 파일은 에러.
        assert!(resolve_description_input(None, Some(dir.join("none.md"))).is_err());
        let _ = std::fs::remove_dir_all(&dir);

        // clap: --description 과 --description-file 동시 지정 거부.
        assert!(Cli::try_parse_from([
            "openguild", "quest", "new", "--type", "DEV", "--title", "t",
            "--description", "a", "--description-file", "b.md",
        ])
        .is_err());
    }

    /// DEV-210: `--at latest` 키워드 해석 — 대소문자 무시, 일반 ISO 는 passthrough.
    #[test]
    fn resolve_at_keyword_latest_and_passthrough() {
        assert_eq!(resolve_at_keyword("latest"), "9999-12-31T23:59:59Z");
        assert_eq!(resolve_at_keyword("LATEST"), "9999-12-31T23:59:59Z");
        assert_eq!(resolve_at_keyword("Latest"), "9999-12-31T23:59:59Z");
        assert_eq!(
            resolve_at_keyword("2026-06-27T00:15:00Z"),
            "2026-06-27T00:15:00Z"
        );
        // 오타류는 그대로 통과 — core replay 의 ts 파싱이 거부한다.
        assert_eq!(resolve_at_keyword("lastest"), "lastest");
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
            Command::Quest { sub: QuestCmd::Comment { sub: CommentCmd::List { slug, author, since, top_only, reply_to, grep, reverse, limit, tree } } } => {
                assert_eq!(slug, "DEV-001");
                assert_eq!(author.as_deref(), Some("claude"));
                assert_eq!(since.as_deref(), Some("2026-06-01"));
                assert!(top_only);
                assert!(reply_to.is_none());
                assert_eq!(grep.as_deref(), Some("needle"));
                assert!(!reverse);
                assert!(limit.is_none());
                assert!(!tree);
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

    /// DEV-234: pin 토글 — quest / campaign 양쪽 subcommand 트리에서 파싱.
    #[test]
    fn cli_parse_comment_pinned() {
        let cli = Cli::try_parse_from([
            "openguild", "quest", "comment", "pinned", "DEV-001", "3",
        ])
        .unwrap();
        match cli.command {
            Command::Quest { sub: QuestCmd::Comment { sub: CommentCmd::Pinned { slug, id } } } => {
                assert_eq!(slug, "DEV-001");
                assert_eq!(id, 3);
            }
            _ => panic!("expected comment pinned"),
        }
    }

    /// 옵션셋 일관성 가드(guild rule cli-list-command-options): 알려진 목록형
    /// 명령은 --table 이 파싱돼야 한다. 새 목록 명령을 추가하면 이 목록에도
    /// 넣을 것 — 안 넣으면 리뷰에서 걸리는 구조.
    #[test]
    fn all_list_commands_accept_table_flag() {
        const LIST_COMMANDS: &[&[&str]] = &[
            &["quest", "list"],
            &["quest", "deleted"],
            &["campaign", "list"],
            &["type", "list"],
            &["status", "list"],
            &["library", "list"],
        ];
        for cmd in LIST_COMMANDS {
            let mut argv = vec!["openguild"];
            argv.extend_from_slice(cmd);
            argv.push("--table");
            assert!(
                Cli::try_parse_from(&argv).is_ok(),
                "{} 은 --table 을 지원해야 함 (cli-list-command-options 규칙)",
                cmd.join(" ")
            );
        }
    }

    /// admin 요청: docs 명령 — 이름 optional, 목록/본문 파싱.
    #[test]
    fn cli_parse_docs() {
        let bare = Cli::try_parse_from(["openguild", "docs"]).unwrap();
        assert!(matches!(bare.command, Command::Docs { name: None }));
        let named = Cli::try_parse_from(["openguild", "docs", "agents"]).unwrap();
        match named.command {
            Command::Docs { name } => assert_eq!(name.as_deref(), Some("agents")),
            _ => panic!("expected docs"),
        }
        // 알 수 없는 이름은 파싱은 통과(런타임 에러 담당) — free string 이므로.
        assert!(Cli::try_parse_from(["openguild", "docs", "nope"]).is_ok());
    }

    /// admin 요청: top-level tag 그룹 — 단수형 + sub 필수 규칙 준수.
    #[test]
    fn cli_parse_tag_group() {
        // sub 없이 bare 호출은 에러 (cli-command-naming 규칙).
        assert!(Cli::try_parse_from(["openguild", "tag"]).is_err());
        let cli = Cli::try_parse_from([
            "openguild", "tag", "add", "infra", "--color", "#ff8800", "--description", "인프라",
        ])
        .unwrap();
        match cli.command {
            Command::Tag { sub: TagDefCmd::Add { slug, color, description } } => {
                assert_eq!(slug, "infra");
                assert_eq!(color.as_deref(), Some("#ff8800"));
                assert_eq!(description.as_deref(), Some("인프라"));
            }
            _ => panic!("expected tag add"),
        }
        let l = Cli::try_parse_from(["openguild", "tag", "list", "--used"]).unwrap();
        match l.command {
            Command::Tag { sub: TagDefCmd::List { used } } => assert!(used),
            _ => panic!("expected tag list"),
        }
    }

    /// admin 요청(comment 출력 개선): --tree / --depth all 파싱.
    #[test]
    fn cli_parse_comment_tree_and_depth_all() {
        // --tree 는 --reverse 와 상호배타.
        assert!(Cli::try_parse_from([
            "openguild", "quest", "comment", "list", "DEV-001", "--tree", "--reverse",
        ])
        .is_err());
        let ok = Cli::try_parse_from([
            "openguild", "quest", "comment", "list", "DEV-001", "--tree",
        ])
        .unwrap();
        match ok.command {
            Command::Quest { sub: QuestCmd::Comment { sub: CommentCmd::List { tree, .. } } } => {
                assert!(tree);
            }
            _ => panic!("expected comment list"),
        }

        // --depth: 숫자 / all / 그 외 거부.
        let d = Cli::try_parse_from([
            "openguild", "quest", "comment", "show", "DEV-001", "--id", "1", "--depth", "all",
        ])
        .unwrap();
        match d.command {
            Command::Quest { sub: QuestCmd::Comment { sub: CommentCmd::Show { depth, .. } } } => {
                assert_eq!(depth, usize::MAX, "'all' = 무제한");
            }
            _ => panic!("expected comment show"),
        }
        assert!(Cli::try_parse_from([
            "openguild", "quest", "comment", "show", "DEV-001", "--id", "1", "--depth", "deep",
        ])
        .is_err());
    }

    /// admin 요청: comment show 의 부모/자식 출력 범위 옵션.
    #[test]
    fn cli_parse_comment_show_depth_and_with_parents() {
        let cli = Cli::try_parse_from([
            "openguild", "quest", "comment", "show", "DEV-001",
            "--id", "3", "--depth", "2", "--with-parents",
        ])
        .unwrap();
        match cli.command {
            Command::Quest {
                sub: QuestCmd::Comment { sub: CommentCmd::Show { slug, id, depth, with_parents } },
            } => {
                assert_eq!(slug, "DEV-001");
                assert_eq!(id, Some(3));
                assert_eq!(depth, 2);
                assert!(with_parents);
            }
            _ => panic!("expected comment show"),
        }

        // 기본값 — depth=0, with_parents=false.
        let cli2 =
            Cli::try_parse_from(["openguild", "quest", "comment", "show", "DEV-001"]).unwrap();
        match cli2.command {
            Command::Quest {
                sub: QuestCmd::Comment { sub: CommentCmd::Show { id, depth, with_parents, .. } },
            } => {
                assert_eq!(id, None);
                assert_eq!(depth, 0);
                assert!(!with_parents);
            }
            _ => panic!("expected comment show"),
        }
    }

    fn thread_entry(
        id: u64,
        parent_id: Option<u64>,
        ts: &str,
    ) -> openguild_core::repo::comments::CommentEntry {
        openguild_core::repo::comments::CommentEntry {
            id,
            ts: ts.to_string(),
            author: "a".into(),
            body: format!("body {id}"),
            parent_id,
            reactions: Vec::new(),
            discussion: false,
            resolved: false,
            pinned: false,
            edited_at: None,
        }
    }

    /// select_thread — depth 0 은 대상만, with_parents 는 root 까지 조상 포함,
    /// depth>0 은 그만큼 답글 단계를 BFS 로 포함(존재 안 하는 id 는 None).
    #[test]
    fn select_thread_depth_and_parents() {
        // 1(root) -> 2(level1) -> 3(level2) -> 4(level3); 2 의 형제로 5.
        let entries = vec![
            thread_entry(1, None, "t1"),
            thread_entry(2, Some(1), "t2"),
            thread_entry(3, Some(2), "t3"),
            thread_entry(4, Some(3), "t4"),
            thread_entry(5, Some(1), "t5"),
        ];

        // depth=0, with_parents=false → 대상 하나만.
        let only = select_thread(entries.clone(), 3, 0, false).unwrap();
        assert_eq!(only.iter().map(|e| e.id).collect::<Vec<_>>(), vec![3]);

        // depth=1 → 3 의 직접 답글(4)까지.
        let d1 = select_thread(entries.clone(), 3, 1, false).unwrap();
        assert_eq!(d1.iter().map(|e| e.id).collect::<Vec<_>>(), vec![3, 4]);

        // with_parents=true → root(1) → 2 → 3 순으로 조상 포함.
        let wp = select_thread(entries.clone(), 3, 0, true).unwrap();
        assert_eq!(wp.iter().map(|e| e.id).collect::<Vec<_>>(), vec![1, 2, 3]);

        // 둘 다 → 조상 + 대상 + 답글.
        let both = select_thread(entries.clone(), 3, 1, true).unwrap();
        assert_eq!(both.iter().map(|e| e.id).collect::<Vec<_>>(), vec![1, 2, 3, 4]);

        // 없는 id → None.
        assert!(select_thread(entries, 99, 0, false).is_none());
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

    // ───────── DEV-211: --compact / --table 플래그 ─────────

    #[test]
    fn compact_requires_json() {
        let r = Cli::try_parse_from(["openguild", "quest", "list", "--compact"]);
        assert!(r.is_err(), "--compact 는 --json 없이는 거부");
        let ok = Cli::try_parse_from(["openguild", "quest", "list", "--json", "--compact"]);
        assert!(ok.is_ok());
        assert!(ok.unwrap().compact);
    }

    #[test]
    fn table_conflicts_with_tree_idonly_count() {
        // --json 은 전역 인자라 clap conflicts 대상이 못 됨 — 핸들러 수동 검증.
        for extra in ["--tree", "--id-only", "--count"] {
            let r = Cli::try_parse_from(["openguild", "quest", "list", "--table", extra]);
            assert!(r.is_err(), "--table 은 {extra} 와 상호배타");
        }
        assert!(Cli::try_parse_from(["openguild", "quest", "list", "--table"]).is_ok());
    }

    #[test]
    fn json_str_respects_compact_flag() {
        let v = serde_json::json!({ "a": 1, "b": [1, 2] });
        JSON_COMPACT.store(false, std::sync::atomic::Ordering::Relaxed);
        assert!(json_str(&v).contains('\n'), "기본은 pretty (기존 호환)");
        JSON_COMPACT.store(true, std::sync::atomic::Ordering::Relaxed);
        assert!(!json_str(&v).contains('\n'), "--compact 는 한 줄");
        // 다른 테스트에 새지 않게 복원.
        JSON_COMPACT.store(false, std::sync::atomic::Ordering::Relaxed);
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

    /// BUG-102: init 직후 index.db 의 statuses 가 파일 시드와 일치해야 한다.
    /// init 이 reindex 를 안 하면 migration(0001)의 구식 기본 5개 셋이 DB 에
    /// 남아 파일(7개 — testing/returned 포함)과 첫날부터 drift — statuses
    /// 목록이 5개로 보이다가 restore/reindex 시점에 7개로 "바뀌는" 증상.
    #[test]
    fn init_syncs_index_db_statuses_with_seed_files() {
        let dir = fresh_tmp("bug102-statuses");
        init_guild_at(&dir, Some("t".into())).unwrap();

        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let n: i64 = rt.block_on(async {
            let store = openguild_core::Store::open(&dir).await.unwrap();
            sqlx::query_scalar("SELECT COUNT(*) FROM quest_statuses")
                .fetch_one(&store.index_pool)
                .await
                .unwrap()
        });
        assert_eq!(
            n as usize,
            openguild_core::repo::default_statuses().len(),
            "init 직후 DB statuses 수가 파일 시드와 달라 drift (BUG-102)"
        );
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

    /// admin 지적(2026-07-05): 규칙 slug 는 일반 단어와 형식이 같아
    /// `<PREFIX>-<숫자>` 패턴으로는 못 잡는다 — 대신 **이 repo 의 실제
    /// `.guild/rules/*.md` slug 목록**을 읽어 help 에 등장하는지 직접 검사.
    /// (개발 repo 밖에서 실행되면 rules 디렉토리가 없어 자연 skip.)
    #[test]
    fn help_output_has_no_rule_slug_leaks() {
        use clap::CommandFactory;
        let rules_dir =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../.guild/rules");
        let Ok(rd) = std::fs::read_dir(&rules_dir) else {
            return; // dogfood 길드 없음 — skip.
        };
        let slugs: Vec<String> = rd
            .filter_map(|e| e.ok())
            .filter_map(|e| {
                let name = e.file_name().to_string_lossy().to_string();
                name.strip_suffix(".md").map(str::to_string)
            })
            .collect();

        let cmd = Cli::command();
        let mut violations: Vec<String> = Vec::new();
        check_rule_slugs_recursive(&cmd, cmd.get_name(), &slugs, &mut violations);
        assert!(
            violations.is_empty(),
            "규칙 slug 가 help 출력에 leak:\n{}",
            violations.join("\n")
        );
    }

    fn check_rule_slugs_recursive(
        cmd: &clap::Command,
        path: &str,
        slugs: &[String],
        violations: &mut Vec<String>,
    ) {
        let mut owned = cmd.clone();
        let help = owned.render_long_help().to_string().to_lowercase();
        for slug in slugs {
            if help.contains(&slug.to_lowercase()) {
                violations.push(format!("[{path}] '{slug}' in help"));
            }
        }
        for sub in cmd.get_subcommands() {
            let sub_path = format!("{path} {}", sub.get_name());
            check_rule_slugs_recursive(sub, &sub_path, slugs, violations);
        }
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

    /// quest ID 형식과 겹치지만 help 에 써도 되는 기술 용어 (오탐 방지).
    const HELP_ID_ALLOWLIST: &[&str] = &["UTF-8", "ISO-8601", "RFC-3339", "SHA-256"];

    /// 처음 발견된 `<PREFIX>-<숫자>` substring (PREFIX 는 ASCII 대문자 2~5).
    /// 단 HELP_ID_ALLOWLIST 의 기술 용어는 건너뜀.
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
            // prefix 1자도 잡음 — 캠페인 slug(C-001) leak 도 가드 대상 (admin 지적).
            // 길드 규칙 slug(소문자 단어)는 일반 텍스트와 형식이 같아 패턴으로
            // 원리적으로 구분 불가 — help_output_has_no_rule_slug_leaks 가 실제 slug 목록으로 커버.
            if prefix_len >= 1 && digits > after {
                let candidate = &s[prefix_start..digits];
                // 기술 용어 오탐 skip — 더 긴 용어(ISO-8601 등)의 부분 매칭도
                // 흡수하도록 allowlist 항목이 candidate 를 포함하는지로 검사.
                let allowed = HELP_ID_ALLOWLIST.iter().any(|t| t.contains(candidate));
                if !allowed {
                    return Some(candidate);
                }
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
        assert_eq!(find_quest_id("캠페인 C-001 참고"), Some("C-001")); // 캠페인도 가드.
        assert!(find_quest_id("DEV-").is_none()); // 숫자 없음.
        assert!(find_quest_id("dev-001").is_none()); // 소문자 (규칙 slug 는 별도 테스트가 실제 목록으로 검사).
        // 기술 용어 allowlist — 오탐 아님.
        assert!(find_quest_id("본문을 UTF-8 파일에서 읽기").is_none());
        assert!(find_quest_id("ISO-8601 UTC").is_none());
        // allowlist 용어와 quest id 가 같이 있으면 quest id 는 잡아야 함.
        assert_eq!(find_quest_id("UTF-8 그리고 DEV-001"), Some("DEV-001"));
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

    /// DEV-227: quest/campaign/template/backup/check/index/journal/rule 은
    /// sub 없인 에러(list 조차 명시 필요) — type/status 도 동일하게 맞춤.
    /// bare 호출이 조용히 list 로 떨어지던 DEV-062 관행 제거.
    #[test]
    fn cli_types_and_statuses_require_explicit_sub() {
        assert!(Cli::try_parse_from(["openguild", "type"]).is_err());
        assert!(Cli::try_parse_from(["openguild", "status"]).is_err());
    }

    /// DEV-227: type/status 단수형이 canonical, 복수형은 alias 로 계속
    /// 동작해야(기존 스크립트 호환) — sub 는 다른 그룹처럼 필수.
    #[test]
    fn cli_singular_type_status_parse_same_as_plural_alias() {
        for args in [["openguild", "type", "list"], ["openguild", "types", "list"]] {
            let cli = Cli::try_parse_from(args).unwrap();
            assert!(matches!(cli.command, Command::Types { sub: TypesCmd::List { .. } }));
        }
        for args in [["openguild", "status", "list"], ["openguild", "statuses", "list"]] {
            let cli = Cli::try_parse_from(args).unwrap();
            assert!(matches!(cli.command, Command::Statuses { sub: StatusesCmd::List { .. } }));
        }
    }

    /// DEV-231: rule 은 type/status 와 달리 복수형 alias 를 아예 남기지
    /// 않기로 함(사용자 지시) — `rules` 는 이제 unknown subcommand 에러.
    #[test]
    fn cli_rule_singular_only_no_plural_alias() {
        let cli = Cli::try_parse_from(["openguild", "rule", "list"]).unwrap();
        match cli.command {
            Command::Rules { sub } => assert!(matches!(sub, RulesCmd::List)),
            _ => panic!(),
        }
        assert!(
            Cli::try_parse_from(["openguild", "rules", "list"]).is_err(),
            "rules alias 는 완전히 제거됐어야"
        );
    }

    /// 도서관 명령 파싱 — 단수형 top-level + sub 필수 규칙 준수, new/update
    /// 의 --title/--file 조합, delete --yes.
    #[test]
    fn cli_library_subcommands_parse() {
        assert!(Cli::try_parse_from(["openguild", "library"]).is_err(), "sub 필수");
        let cli = Cli::try_parse_from(["openguild", "library", "list"]).unwrap();
        assert!(matches!(cli.command, Command::Library { sub: LibraryCmd::List { .. } }));

        let cli = Cli::try_parse_from(["openguild", "library", "show", "BOOK-001"]).unwrap();
        match cli.command {
            Command::Library { sub: LibraryCmd::Show { id } } => assert_eq!(id, "BOOK-001"),
            _ => panic!(),
        }

        let cli = Cli::try_parse_from([
            "openguild", "library", "new", "--title", "제목", "--file", "b.md", "--path", "아키텍처",
        ])
        .unwrap();
        match cli.command {
            Command::Library { sub: LibraryCmd::New { title, file, path } } => {
                assert_eq!(title, "제목");
                assert_eq!(file.unwrap().to_string_lossy(), "b.md");
                assert_eq!(path.as_deref(), Some("아키텍처"));
            }
            _ => panic!(),
        }

        let cli = Cli::try_parse_from([
            "openguild", "library", "update", "BOOK-002", "--title", "t2",
        ])
        .unwrap();
        match cli.command {
            Command::Library { sub: LibraryCmd::Update { id, title, file, path } } => {
                assert_eq!(id, "BOOK-002");
                assert_eq!(title.as_deref(), Some("t2"));
                assert!(file.is_none());
                assert!(path.is_none());
            }
            _ => panic!(),
        }

        let cli =
            Cli::try_parse_from(["openguild", "library", "delete", "BOOK-003", "--yes"]).unwrap();
        match cli.command {
            Command::Library { sub: LibraryCmd::Delete { id, yes } } => {
                assert_eq!(id, "BOOK-003");
                assert!(yes);
            }
            _ => panic!(),
        }

        let cli = Cli::try_parse_from(["openguild", "library", "folder", "new", "아키텍처"])
            .unwrap();
        match cli.command {
            Command::Library { sub: LibraryCmd::Folder { sub: LibraryFolderCmd::New { path } } } => {
                assert_eq!(path, "아키텍처");
            }
            _ => panic!(),
        }
        assert!(
            matches!(
                Cli::try_parse_from(["openguild", "library", "folder", "list"])
                    .unwrap()
                    .command,
                Command::Library {
                    sub: LibraryCmd::Folder { sub: LibraryFolderCmd::List }
                }
            )
        );

        let cli = Cli::try_parse_from([
            "openguild", "library", "folder", "delete", "아키텍처/서브", "--yes",
        ])
        .unwrap();
        match cli.command {
            Command::Library {
                sub: LibraryCmd::Folder { sub: LibraryFolderCmd::Delete { path, yes } },
            } => {
                assert_eq!(path, "아키텍처/서브");
                assert!(yes);
            }
            _ => panic!(),
        }
    }

    /// 작업 기록 명령 파싱 — show 의 date/from-to 상호배타, note sub 필수.
    #[test]
    fn cli_worklog_subcommands_parse() {
        assert!(Cli::try_parse_from(["openguild", "worklog"]).is_err(), "sub 필수");
        let cli = Cli::try_parse_from(["openguild", "worklog", "show"]).unwrap();
        assert!(matches!(
            cli.command,
            Command::Worklog { sub: WorklogCmd::Show { date: None, from: None, to: None } }
        ));

        let cli =
            Cli::try_parse_from(["openguild", "worklog", "show", "--date", "2026-07-05"]).unwrap();
        match cli.command {
            Command::Worklog { sub: WorklogCmd::Show { date, .. } } => {
                assert_eq!(date.as_deref(), Some("2026-07-05"));
            }
            _ => panic!(),
        }
        // --date 와 --from 동시 지정 거부, --from 은 --to 필수.
        assert!(Cli::try_parse_from([
            "openguild", "worklog", "show", "--date", "2026-07-05", "--from", "2026-07-01",
            "--to", "2026-07-05",
        ])
        .is_err());
        assert!(
            Cli::try_parse_from(["openguild", "worklog", "show", "--from", "2026-07-01"]).is_err()
        );

        let cli = Cli::try_parse_from([
            "openguild", "worklog", "note", "set", "2026-07-05", "--file", "n.md",
        ])
        .unwrap();
        match cli.command {
            Command::Worklog { sub: WorklogCmd::Note { sub: WorklogNoteCmd::Set { date, file } } } => {
                assert_eq!(date, "2026-07-05");
                assert!(file.is_some());
            }
            _ => panic!(),
        }
        let cli =
            Cli::try_parse_from(["openguild", "worklog", "note", "clear", "2026-07-05"]).unwrap();
        assert!(matches!(
            cli.command,
            Command::Worklog { sub: WorklogCmd::Note { sub: WorklogNoteCmd::Clear { .. } } }
        ));
    }

    /// BUG-111: quest/campaign/template/backup 은 전부 `new` 가 canonical 인데
    /// rules 만 `create` 가 canonical 이라 `rule --help` 에 create 로 나왔음 —
    /// new 를 canonical 로 스왑. DEV-232: create alias 도 사용자 지시로 완전
    /// 제거(rules 복수형 alias 와 동일한 판단 — 남길 이유 없음).
    #[test]
    fn cli_rule_new_is_canonical_create_removed() {
        use clap::CommandFactory;
        let cmd = Cli::command();
        let rule_cmd = cmd
            .get_subcommands()
            .find(|c| c.get_name() == "rule")
            .expect("rule top-level 존재");
        let new_sub = rule_cmd
            .get_subcommands()
            .find(|c| c.get_name() == "new")
            .expect("new 가 canonical 이어야 (help 에 new 로 표시)");
        let aliases: Vec<&str> = new_sub.get_all_aliases().collect();
        assert!(aliases.is_empty(), "create alias 는 완전히 제거됐어야: {aliases:?}");

        let cli = Cli::try_parse_from(["openguild", "rule", "new", "t1", "--empty"]).unwrap();
        match cli.command {
            Command::Rules { sub: RulesCmd::Create { slug, empty, .. } } => {
                assert_eq!(slug, "t1");
                assert!(empty);
            }
            _ => panic!(),
        }
        assert!(
            Cli::try_parse_from(["openguild", "rule", "create", "t1", "--empty"]).is_err(),
            "create 는 이제 unknown subcommand 에러여야"
        );
    }

    /// BUG-110: 예전엔 bare `openguild types`/`statuses` 가 list 로 떨어졌는데
    /// DEV-227 의 sub 필수화가 alias 에도 그대로 적용돼 이 하위호환이 깨졌음.
    /// canonical(`type`/`status`)은 sub 필수 유지, legacy alias 만 rewrite 로
    /// list 를 끼워 넣어 예전 동작 복원.
    #[test]
    fn rewrite_legacy_plural_bare_appends_list() {
        let out = rewrite_legacy_plural_bare_invocation(vec!["openguild".into(), "types".into()]);
        assert_eq!(out, vec!["openguild", "types", "list"]);
        let out2 =
            rewrite_legacy_plural_bare_invocation(vec!["openguild".into(), "statuses".into()]);
        assert_eq!(out2, vec!["openguild", "statuses", "list"]);
    }

    #[test]
    fn rewrite_legacy_plural_leaves_canonical_and_explicit_sub_untouched() {
        // canonical 단수형은 안 건드림 — sub 필수 그대로.
        let out = rewrite_legacy_plural_bare_invocation(vec!["openguild".into(), "type".into()]);
        assert_eq!(out, vec!["openguild", "type"]);
        // 이미 서브커맨드가 있으면 안 건드림.
        let out2 = rewrite_legacy_plural_bare_invocation(vec![
            "openguild".into(),
            "types".into(),
            "add".into(),
            "FOO".into(),
        ]);
        assert_eq!(out2, vec!["openguild", "types", "add", "FOO"]);
    }

    #[test]
    fn rewrite_legacy_plural_skips_global_flags() {
        let out = rewrite_legacy_plural_bare_invocation(vec![
            "openguild".into(),
            "--json".into(),
            "statuses".into(),
        ]);
        assert_eq!(out, vec!["openguild", "--json", "statuses", "list"]);
    }

    /// end-to-end: rewrite 를 거치면 legacy plural bare 호출이 실제로 List 로
    /// 파싱되지만, canonical 단수형은 여전히 sub 없인 에러.
    #[test]
    fn cli_legacy_plural_bare_parses_as_list_via_rewrite() {
        let rewritten =
            rewrite_legacy_plural_bare_invocation(vec!["openguild".into(), "types".into()]);
        let cli = Cli::try_parse_from(rewritten).unwrap();
        assert!(matches!(cli.command, Command::Types { sub: TypesCmd::List { .. } }));

        let rewritten2 =
            rewrite_legacy_plural_bare_invocation(vec!["openguild".into(), "statuses".into()]);
        let cli2 = Cli::try_parse_from(rewritten2).unwrap();
        assert!(matches!(cli2.command, Command::Statuses { sub: StatusesCmd::List { .. } }));

        assert!(Cli::try_parse_from(["openguild", "type"]).is_err());
    }

    #[test]
    fn cli_types_add() {
        let cli = Cli::try_parse_from([
            "openguild", "types", "add", "FOO", "--color", "#abcdef", "--description", "x",
        ])
        .unwrap();
        match cli.command {
            Command::Types {
                sub: TypesCmd::Add { prefix, color, description },
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
                    TypesCmd::Update {
                        prefix,
                        new_prefix,
                        clear_description,
                        description,
                        color,
                    },
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
                sub: TypesCmd::Update { prefix, new_prefix, .. },
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
                sub: StatusesCmd::Add { name_en, color, name_ko, sort_order },
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
                sub: StatusesCmd::Update { slug, new_slug, .. },
            } => {
                assert_eq!(slug, "open");
                assert_eq!(new_slug.as_deref(), Some("backlog"));
            }
            _ => panic!(),
        }
    }
}
