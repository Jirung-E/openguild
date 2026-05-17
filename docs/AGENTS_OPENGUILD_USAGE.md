# AGENTS — OpenGuild 사용 가이드

AI agent / 자동화 스크립트가 OpenGuild 를 **자기 작업 관리 도구** 로 사용할 때 참조하는 문서.
(OpenGuild **를 개발** 할 때 참조는 `AGENTS.md` 의 "개발" 인덱스 따라갈 것.)

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

서버 관리 명령 (서버를 띄우지 않고 실행):
```bash
openguild-server backup     # 즉시 1회 백업
openguild-server info       # 길드 메타 / DB 크기 / 백업 현황
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
openguild quest show <slug>                          # 예: DEV-001 (서브/선행 포함)

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
openguild quest status <slug> <STATUS>   # 임의 상태 (이름 또는 ID)
openguild quest start  <slug>            # → In Progress
openguild quest done   <slug>            # → Done
openguild quest reopen <slug>            # → Open
```

상태명은 대소문자 / 공백 / `_` / `-` 모두 허용:
`In Progress`, `in progress`, `in_progress`, `in-progress` 모두 같은 상태.

#### 상태 흐름 (필수 워크플로)

```
open → in_progress → testing → done
                ↓        ↑
            (반복 가능)
            cancelled
            on_hold (필요 시 분기)
```

- **`done` 으로 직행 금지.** 작업 완료 시 `testing` 으로 보낸 뒤,
  사용자가 검증한 후에만 `done`.
- 단순 메타 변경 (오타 수정, 주석 등) 도 동일 — 사용자 검증을 거쳐야 함.
- `done` 으로 옮기는 명령 (`openguild quest done`) 은 사용자가 직접 호출하는
  것이 원칙. agent 가 마음대로 `done` 처리 금지.

#### 테스트 단계로 보낼 때 — 본문에 테스트 방법 첨부 **필수**

`openguild quest status <slug> testing` 호출 직전 또는 직후에, quest 본문
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
openguild quest status DEV-002 testing
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

### 2.5 백업 / 복원

```bash
openguild backup                       # 즉시 snapshot 생성
openguild backups                      # 사용 가능 snapshot 목록
openguild restore [--to <TIMESTAMP>]   # 최신 (또는 지정) snapshot 으로 복원
```

자동 백업: 매 mutation 이후 정책 검토 (ops 50회 OR 24시간 경과 시 자동 snapshot).
env 로 임계치 조정 가능:
- `OPENGUILD_AUTO_BACKUP_OPS=N` (기본 50)
- `OPENGUILD_AUTO_BACKUP_HOURS=N` (기본 24)

자동 백업 시 stderr 에 알림: `[auto-backup] snapshot 생성됨: 20260516-103341 (...)`.

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
| **자동 백업** | 서버가 1시간마다 `VACUUM INTO` 로 `<guild>/backups/guild.db.<ts>` 생성, 7일 보관 |
| **Audit log** | 모든 mutation HTTP 호출이 `<guild>/audit.log` 에 timestamped tab-separated 기록 |

### 권장 패턴

- **삭제 전 항상 `--dry-run` → 결과 확인 → `--yes` 로 실행**
- 한 번에 다수 quest 삭제 금지 (loop 안 됨)
- `--json` 으로 출력 캡처 후 후속 호출에 슬러그 사용

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
    echo "OpenGuild 가 동작하지 않습니다 (로컬 모드면 .guild 없음, 원격 모드면 서버 다운)" >&2
    exit 1
fi
```

---

## 6. JSON 출력 사용

`--json` 시 모든 출력이 한 줄 또는 pretty-printed JSON. agent 가 `jq` / `serde_json` 등으로 파싱:

```bash
# 새 quest 생성 후 슬러그만 캡처
SLUG=$(openguild quest new --type DEV --title "X" --json | jq -r '.quest_id')
openguild quest start "$SLUG"
```

dry-run 도 JSON 모드 지원 — 영향 분석을 프로그래밍으로 처리 가능.
