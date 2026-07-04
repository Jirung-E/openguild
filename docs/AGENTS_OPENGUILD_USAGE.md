# AGENTS — openguild 사용 가이드

AI agent / 자동화 스크립트가 openguild 를 **자기 작업 관리 도구** 로 사용할 때 참조하는 문서.
(openguild **를 개발** 할 때 참조는 `AGENTS.md` 의 "개발" 인덱스 따라갈 것.)

---

## 1. 셋업

### 1.1 길드 초기화 (최초 1회)

현재 디렉토리를 길드로 초기화. `.guild` 마커 파일 생성:

```bash
cd /path/to/your-project
openguild init                            # 디렉토리 이름이 길드명
openguild init --name "내 프로젝트"       # 길드명 지정
```

### 1.2 동작 모드

**로컬 모드 (기본, 권장)** — 서버 불필요. cwd 부터 `.guild` 자동 탐색해 core 직접 호출.

```bash
cd /path/to/your-project    # .guild 있는 디렉토리
openguild quest list        # 바로 사용 가능
openguild --guild ./other-project quest list   # 다른 길드 명시
```

**원격 모드** — 호스팅된 서버에 HTTP 로 접속.

```bash
openguild --remote https://openguild.io/alice/monitor quest list
# 또는
export OPENGUILD_REMOTE=https://openguild.io/alice/monitor
openguild quest list
```

자체 서버를 띄우고 싶을 때:
```bash
GUILD_PATH=/path/to/your-project cargo run --bin openguild-server -- host
# 다른 터미널에서:
openguild --remote http://localhost:3000 ping
```

정비/진단 명령 (서버 불필요 — `openguild` CLI):
```bash
openguild backup new   # 즉시 1회 백업 (snapshot)
openguild info         # 길드 메타 / DB 크기 / 백업 현황
```

### 1.3 환경변수 / 옵션

| 항목 | 기본값 | 설명 |
|---|---|---|
| env `OPENGUILD_REMOTE` | (미설정) | 원격 서버 URL. 설정 시 원격 모드 |
| 전역 `--remote <URL>` | env 보다 우선 | 원격 모드 강제 |
| 전역 `--guild <PATH>` | (미설정) | 로컬 모드의 길드 경로. 미설정 시 cwd 부터 자동 탐색 |
| 전역 `--json` | 끔 | agent stdout 파싱용 |

---

## 2. 명령어 카탈로그

### 2.1 길드 / 메타

```bash
openguild init [--name <NAME>]   # cwd 를 길드로 초기화
openguild ping                   # 서버 health 확인
openguild types                  # quest 타입 목록 (DEV / BUG / REQ)
openguild statuses               # 상태 목록 (Open / In Progress / Done / ...)
```

### 2.2 Quest CRUD

```bash
openguild quest list [--json]
openguild quest list --type DEV,BUG --status open,in_progress --urgency 1-2
                    --has-prereq --no-sub --child-of <slug> --no-parent
                    --created-after 2026-05-01 --updated-before 2026-06-01
                    --search "키워드" --title-only
                    --sort urgency,id --reverse --limit 20 --offset 0
                    --id-only | --count                # script 친화 출력

openguild quest search "<keyword>" [--title-only] [--limit N]
                       [--id-only | --count]          # `list --search` 의 단축

openguild quest show <slug> [--field <NAME>]          # NAME: id/title/status/description/urgency/type/parent/created_at/updated_at

openguild quest new --type <PREFIX> --title <T>      # 상태는 자동으로 Open
                  [--description <DESC>]
                  [--urgency 1-4]                    # 1=Critical 4=Low (기본 3)
                  [--parent <slug>]                  # 서브퀘스트로 생성

openguild quest update <slug> [--title] [--description] [--urgency]
                              [--dry-run]            # 영향 미리보기

openguild quest delete <slug> [--cascade <slug>,...] --yes        # --yes 필수
openguild quest delete <slug> [--cascade <slug>,...] --dry-run    # 영향 미리보기

openguild quest restore <slug>   # soft delete 취소 (alive 복원)
openguild quest deleted          # soft deleted 목록
```

### 2.3 상태 변경

```bash
openguild quest move   <slug> <STATUS>   # ⭐ 정식 — 임의 상태 (이름 또는 slug, ID)
openguild quest start  <slug>            # → In Progress (shortcut)
openguild quest done   <slug>            # → Done       (shortcut)
openguild quest reopen <slug>            # → Open       (shortcut)

openguild quest status <slug>            # 현재 상태만 출력 (조회 전용)
openguild quest status <slug> <STATUS>   # ⚠ deprecated — `move` 사용 권장
```

상태명은 대소문자 / 공백 / `_` / `-` 모두 허용:
`In Progress`, `in progress`, `in_progress`, `in-progress` 모두 같은 상태.

#### 상태 흐름 (권장 워크플로)

```
open → in_progress → testing → done
                ↓        ↑
            (반복 가능)
            cancelled
            on_hold (필요 시 분기)
```

- **자동 테스트로 검증 가능한 변경**: agent 가 가능한 테스트
  (`cargo test --workspace`, `npm test`, `npm run check` 등) 를 수행하고
  통과하면 바로 `done` 으로 보내도 OK. 문제가 발견되면 추가 커밋으로 수정.
- **수동 검증이 필요한 변경** (UI / UX / 외부 통합 등): `testing` 으로 보낸 뒤
  사용자가 검증한 후에 `done`. 이 경우 본문에 테스트 방법 첨부 필수 (아래 참고).
- agent 가 무엇이 자동 테스트로 커버되는지 판단하고 둘 중 선택.
  애매하면 `testing` 으로 보내는 쪽이 안전.

#### 테스트 단계로 보낼 때 — 본문에 테스트 방법 첨부 **필수**

`openguild quest move <slug> testing` 호출 직전 또는 직후에, quest 본문
(description) 에 **"## 테스트 방법"** 섹션을 추가한다.

예시:
```bash
openguild quest update DEV-002 --description "$(cat <<'EOF'
window.__TAURI__ 감지 → invoke 또는 fetch. Tauri 작업의 선행.

## 테스트 방법
- `cd gui/frontend && npm test -- --run` → transport.test.ts 10 tests 통과
- `cd gui/frontend && npm run check` → 0 errors
- 브라우저에서 GUI 정상 동작 (fetch 경로) — `npm run dev` 후 quest list 표시
- SSR / Node 환경에서 detectEnvironment() 가 'http' 반환 (Node 의 globalThis 에 window 없음)
EOF
)"
openguild quest move DEV-002 testing
```

테스트 방법 항목 작성 가이드:
- **자동 테스트**: 실행할 명령 + 기대 결과
- **수동 검증**: 어떤 화면 / 어떤 동작을 확인할지
- **회귀**: 본 변경이 깨뜨릴 수 있는 기존 기능 (수동 확인)
- **예상 출력 / 파일**: 무엇이 어디에 생겨야 하는지

이 정보가 있어야 사용자가 무엇을 검증해야 할지 명확하고, 미래의 본인 / 다른
agent 가 같은 quest 재방문 시 맥락을 잃지 않는다.

### 2.4 관계

```bash
openguild quest parent <slug> <parent-slug>       # 부모 변경
openguild quest parent <slug> --detach            # 부모 분리

openguild quest prereq add <slug> <prereq-slug>
openguild quest prereq rm  <slug> <prereq-slug>
```

### 2.5 기한 (DEV-076)

quest 에 희망 / 필수 기한 (`YYYY-MM-DD`) 지정. **필수 기한 (`required_due`)**
은 Home 의 "마감 임박" / "Overdue" 섹션 분류 기준. 희망 기한 (`desired_due`)
은 정보성.

```bash
openguild quest due <slug>                       # 현재 두 기한 출력
openguild quest due <slug> --desired  2026-06-15 # 희망 기한 설정
openguild quest due <slug> --required 2026-06-30 # 필수 기한 설정
openguild quest due <slug> --clear-desired       # 희망 기한 해제 (null)
openguild quest due <slug> --clear-required      # 필수 기한 해제 (null)
```

- 형식은 `YYYY-MM-DD` 만 허용. 잘못된 형식은 `BadRequest`.
- `--desired` / `--required` 와 대응 `--clear-*` 는 상호배타.
- 빈 문자열 / 공백은 자동으로 None 으로 정규화.

### 2.6 변경 이력 (DEV-013)

```bash
openguild quest history <slug>         # 최신 → 과거 순. status / type 변경 등.
openguild quest history <slug> --json
```

각 row 는 op (e.g. `change_status`, `change_type`), old_value, new_value, ts,
quest_slug 포함.

### 2.7 Campaign (DEV-011)

캠페인 = "다음 마일스톤" 계획서. quest 와 다대다 링크 + 자체 체크리스트 +
기간. slug 는 `C-001` ~ `C-NNN`.

```bash
openguild campaign list [--status active|done|planned] [--json]
openguild campaign show <slug> [--json]      # 체크리스트 + linked quests 포함

openguild campaign new --title <T>
                      [--start <YYYY-MM-DD>] [--end <YYYY-MM-DD>]
openguild campaign delete <slug>             # soft delete

openguild campaign start <slug>              # status → active
openguild campaign end   <slug>              # status → done

# Quest 연결 (다대다)
openguild campaign link   <slug> <quest-slug>
openguild campaign unlink <slug> <quest-slug>

# 체크리스트 (1-based 인덱스, body 의 `- [ ]` / `- [x]` 줄과 양방향)
openguild campaign checklist add     <slug> "<text>"
openguild campaign checklist check   <slug> <N>
openguild campaign checklist uncheck <slug> <N>
openguild campaign checklist rm      <slug> <N>
```

진행률 = `checked / total`. Home 의 active 캠페인 carousel 카드 progress bar
가 이 값을 표시. 100% 달성 시 카드 초록 강조.

### 2.8 백업 / 복원

```bash
openguild backup new                   # 즉시 snapshot 생성
openguild backup list                  # 사용 가능 snapshot 목록
openguild backup remove <TIMESTAMP>    # 특정 snapshot 삭제
openguild restore                      # 최신 snapshot 으로 복원 (journal 보존)
openguild restore --to <TIMESTAMP>     # 지정 snapshot 으로 복원 (journal 보존)
openguild restore --at <ISO8601-UTC>   # 시점 복원(DEV-022) — 최신 snapshot 복원 후
                                       # journal(AOF) 을 그 시각까지 재적용, 이후 journal truncate.
                                       # `--to` 와 상호배타. 내용 op(댓글/메모 본문)·type 변경·
                                       # 첨부가 낀 구간은 거부(fail-loud).
openguild restore --at latest          # 최신 상태로 복구(DEV-210) — journal 전체 재적용.
                                       # 손상 복구의 정식 진입점.
```

자동 백업: 매 mutation 이후 정책 검토 (ops 50회 OR 24시간 경과 시 자동 snapshot).
env 로 임계치 조정 가능:
- `OPENGUILD_AUTO_BACKUP_OPS=N` (기본 50)
- `OPENGUILD_AUTO_BACKUP_HOURS=N` (기본 24)

자동 백업 시 stderr 에 알림: `[auto-backup] snapshot 생성됨: 20260516-103341 (...)`.

### 2.9 댓글 / 메모 (DEV-094 / DEV-099)

```bash
openguild comments [--author A] [--since TS] [--until TS] [--grep T]
                   [--discussion | --unresolved] [--limit N]
    # DEV-221: 길드 전체(quest+campaign) 댓글 횡단 검색 — 기본 최신순 20개.
    # agent 세션 시작 시 `openguild comments --author admin` 으로 피드백 수집.

openguild quest comment list <SLUG>                       # entry 목록
openguild quest comment list <SLUG> --author claude --since 2026-06-01 \
    --top-only --grep TEXT                                # 필터 (AND, DEV-110)
openguild quest comment list <SLUG> --reply-to N          # 특정 entry 의 답글만
openguild quest comment list <SLUG> --reverse --limit 5   # 최근 5개 (DEV-221)
openguild quest comment show <SLUG> [--id N]              # 본문
openguild quest comment add <SLUG> --author <NAME> --file <PATH>   # 추가 (stdin 도 가능)
openguild quest comment add <SLUG> --author <NAME> --parent-id N --file <PATH>  # 답글
openguild quest comment edit <SLUG> N --file <PATH>       # body 교체 (id 는 positional)
openguild quest comment remove <SLUG> N [--force]         # 삭제
openguild quest comment discussion <SLUG> N               # 토론(discussion) 토글 — quest 전용 (DEV-185)
openguild quest comment resolved <SLUG> N                 # 토론 해결 토글 (DEV-185)
openguild quest memo set <SLUG> --file <PATH>             # 비공개 메모 (사용자당 1개)
```

**🚨 `--author` 필수 규칙**: agent 가 댓글을 쓸 때는 반드시 `--author` 에 자기
식별자를 명시할 것 (예: `--author claude`). 작성자 없는 댓글은 GUI 에서
"(이름 없음)" 으로 표시되어 사용자 댓글과 구분이 안 됨 — 누가 쓴 건지 추적
불가. 사용자 / 여러 agent 가 한 quest 에서 대화하는 구조이므로 작성자는
대화의 전제 조건.

캠페인에도 동일 구조의 댓글 / 메모 가 있음 (DEV-100) — 명령 형식 / 필터 /
`--author` 규칙 모두 quest 와 동일:

```bash
openguild campaign comment list C-001 [--author ... --since ... --grep ...]
openguild campaign comment add C-001 --author <NAME> --file <PATH>
openguild campaign comment rm C-001 <ID> --force
openguild campaign memo set C-001 --file <PATH>   # show / clear 도 동일
```

### 2.10 퀘스트 템플릿 (DEV-060)

`.guild/templates/{name}.md` — quest 파일과 같은 `+++` TOML frontmatter
(`title` / `type` / `urgency` / `tags` 전부 선택) + 기본 본문. frontmatter
없으면 파일 전체가 본문.

```bash
openguild template list                  # 템플릿 목록 (이름 / 기본값 요약)
openguild template show <NAME>           # 본문 출력
openguild quest new --template <NAME>    # 템플릿으로 생성
openguild quest new --template bug-report --title "특정 제목"   # 명시 옵션이 우선
```

merge 우선순위: **명시 옵션 > 템플릿 값 > 기본** (urgency 기본 3).
type / title 은 둘 중 한 쪽엔 있어야 함. local 모드 전용 (HTTP 미지원).

### 2.11 정비 / 진단 / 규칙

서버 불필요 — `openguild` CLI 로 직접.

```bash
openguild reindex                    # 파일 → index.db 캐시 재구축 (= index rebuild).
                                     # 외부 편집 / git pull / restore 후 정합
openguild check drift                # 파일 ↔ 캐시 drift 점검
openguild check counters             # 타입 카운터 정합 점검
openguild index rebuild | vacuum     # 캐시 재구축 / VACUUM
openguild journal tail               # journal(AOF) 최근 op — audit / 디버그
openguild info                       # 길드 메타 / index.db·snapshot·journal 요약 (진단)

# 길드 규칙 (.guild/rules/{slug}.md — 프로젝트 컨벤션 문서, git tracked)
openguild rules list                          # 규칙 slug 목록
openguild rules show   <slug>                 # 본문 출력
openguild rules create <slug> --file <PATH>   # 신규 (중복 slug 는 에러, `new` 별칭 OK)
openguild rules set    <slug> --file <PATH>   # 본문 교체 (멱등 — 없으면 생성)
openguild rules delete <slug> [--force]
openguild rules rename <old-slug> <new-slug>
```

---

## 3. Agent 워크플로 패턴

### 3.1 새 작업 시작

```bash
$ openguild quest new --type DEV --title "OAuth 구현" --json
{"id":47,"quest_id":"DEV-047","title":"OAuth 구현", ...}

# 이후 명령에서 슬러그 사용
$ openguild quest start DEV-047
```

### 3.2 큰 작업을 sub-quest 트리로 분할

```bash
$ openguild quest new --type DEV --title "토큰 발급 API" --parent DEV-047 --json
$ openguild quest new --type DEV --title "토큰 검증 미들웨어" --parent DEV-047 --json
```

### 3.3 작업 완료

```bash
$ openguild quest done DEV-047
```

### 3.4 버그 발견 시

```bash
$ openguild quest new --type BUG --title "로그인 토큰 만료 안됨" --urgency 2 --json
# → BUG-XXX 받음. 수정 후 done.
```

### 3.5 진행 중 작업 조회

```bash
$ openguild quest list --json | jq '.[] | select(.status_name_en == "In Progress")'
```

### 3.6 선행 관계 표현

```bash
# "OAuth 토큰 검증 미들웨어 (DEV-049) 은 토큰 발급 API (DEV-048) 가 먼저 완료되어야"
$ openguild quest prereq add DEV-049 DEV-048
```

---

## 4. 안전장치

| 장치 | 설명 |
|---|---|
| **Soft delete** | `delete` 는 실제로 row 를 안 지우고 `deleted_at` 만 set. 복원 `restore`, 목록 `deleted` |
| **`--yes` 강제** | 삭제는 `--yes` 명시 필수. 미명시 시 거부 |
| **`--dry-run`** | `delete` / `update` 의 영향을 실제 호출 없이 미리 확인 |
| **자동 백업** | 매 mutation 후 정책 검토(ops 50회 OR 24h 경과)로 `.guild/` 소스 파일 스냅샷 자동 생성. env `OPENGUILD_AUTO_BACKUP_OPS`/`_HOURS` 로 조정 (§2.8) |
| **저널(AOF)** | 모든 mutation 이 `.guild/backups/journal.db` 에 op 로 기록 — `openguild journal tail` 로 확인, 시점 복원(`restore --at`)의 소스 |

### 권장 패턴

- **삭제 전 항상 `--dry-run` → 결과 확인 → `--yes` 로 실행**
- 한 번에 다수 quest 삭제 금지 (loop 안 됨)
- `--json` 으로 출력 캡처 후 후속 호출에 슬러그 사용

### 🚨 `.guild/` 파일을 직접 편집하지 말 것 (drift 방지)

openguild 는 **파일 = truth, `.guild/index.db` = SQL 캐시** 구조. mutation 은
모두 **`openguild` CLI / `openguild-server` HTTP / Tauri invoke** 를 거쳐
저널 + 파일 + SQL 셋을 원자적으로 갱신.

`.guild/quests/*.md` / `.guild/types/*.toml` / `.guild/statuses/*.toml` 을
에디터 / `Write` 도구로 직접 갈아끼우면 **drift** 발생:
- SQL 캐시는 옛 값을 들고 있어 GUI / `list` 가 다른 상태 보임.
- 저널에 의도 기록 안 됨 → snapshot/restore 로 못 되돌림.
- 카운터 어긋날 수 있음 (BUG-003 류 재현).

#### 필드별 대응 명령

| 변경할 것                | 정식 경로 |
|--------------------------|-------------------------------------------------|
| status                   | `openguild quest move <slug> <STATUS>` (`start` / `done` / `reopen` 도 가능) |
| title                    | `openguild quest update <slug> --title <T>` |
| description              | `openguild quest update <slug> --description <D>` (multi-line OK — BUG-001 수정됨) |
| urgency                  | `openguild quest update <slug> --urgency 1-4` |
| parent                   | `openguild quest parent <slug> <parent>` / `--detach` |
| prerequisites            | `openguild quest prereq add/rm <slug> <other>` |
| 삭제 / 복원              | `openguild quest delete/restore <slug>` |
| type 메타                | `openguild types add/update/delete` (`update --prefix` 로 rename+cascade) |
| status 메타              | `openguild statuses add/update/delete` (`update --slug` 로 rename+cascade) |

#### 부득이하게 직접 편집해야 한다면

본문(description)은 `openguild quest update --description` 이 multi-line 을 정상
저장하므로(BUG-001 수정됨) 웬만하면 CLI 로 처리한다. 그래도 파일을 직접
편집했다면:

1. 편집 직후 **반드시** `openguild reindex` (SQL 캐시 재구축; = `openguild index rebuild`).
2. `openguild check drift` 로 drift 0 확인.
3. journal 에 의도 기록은 자동 안 됨 — commit 메시지 / quest 본문에 사유 명시.

**frontmatter 의 status / urgency / parent / prerequisites 는 절대 직접 안 건드림.**
그건 위 표의 명령으로 해야 함. 본문 (description) 만 직접 편집 OK.

### 안전한 삭제 예시

```bash
# 1단계: 영향 확인
$ openguild quest delete DEV-047 --cascade DEV-048,DEV-049 --dry-run --json
{
  "dry_run": true,
  "would_delete": "DEV-047",
  "cascade_delete": ["DEV-048", "DEV-049"],
  "detach_children": [],
  "unaffected_prerequisites": []
}

# 2단계: 확인 후 실행
$ openguild quest delete DEV-047 --cascade DEV-048,DEV-049 --yes
```

---

## 5. 에러 처리

- **exit code 0**: 성공
- **exit code 1**: 실패. stderr 에 `error: ...` 메시지 출력
- 로컬 모드 — `.guild` 없을 시: stderr 안내 + exit 1. `openguild init` 으로 초기화.
- 원격 모드 — 서버 다운 시: HTTP error + exit 1. `openguild ping` 으로 사전 확인.

```bash
# 사용 가능 여부 확인 (모드 자동 감지)
if ! openguild ping >/dev/null 2>&1; then
    echo "openguild 가 동작하지 않습니다 (로컬 모드면 .guild 없음, 원격 모드면 서버 다운)" >&2
    exit 1
fi
```

---

## 6. JSON 출력 사용

`--json` 시 모든 출력이 pretty-printed(2-space 들여쓰기) JSON. agent 가 `jq` / `serde_json` 등으로 파싱:

```bash
# 새 quest 생성 후 슬러그만 캡처
SLUG=$(openguild quest new --type DEV --title "X" --json | jq -r '.quest_id')
openguild quest start "$SLUG"
```

dry-run 도 JSON 모드 지원 — 영향 분석을 프로그래밍으로 처리 가능.
