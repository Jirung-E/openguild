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

### 1.2 백엔드 서버 띄우기

CLI 는 HTTP 클라이언트. 사전에 서버가 떠 있어야 한다.

```bash
GUILD_PATH=/path/to/your-project cargo run --bin openguild-server
```

서버가 떠 있는지 확인:

```bash
openguild ping
```

### 1.3 환경변수 / 옵션

| 항목 | 기본값 | 설명 |
|---|---|---|
| env `OPENGUILD_URL` | `http://localhost:3000` | 서버 base URL |
| 전역 `--url <URL>` | env 보다 우선 | |
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

### 2.4 관계

```bash
openguild quest parent <slug> <parent-slug>       # 부모 변경
openguild quest parent <slug> --detach            # 부모 분리

openguild quest prereq add <slug> <prereq-slug>
openguild quest prereq rm  <slug> <prereq-slug>
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
- 서버 다운 시: `openguild ping` 으로 사전 확인

```bash
if ! openguild ping >/dev/null 2>&1; then
    echo "OpenGuild 서버가 떠 있지 않습니다" >&2
    exit 1
fi
```

서버 자동 spawn 등의 동작은 미지원 (추후 추가 예정).

---

## 6. JSON 출력 사용

`--json` 시 모든 출력이 한 줄 또는 pretty-printed JSON. agent 가 `jq` / `serde_json` 등으로 파싱:

```bash
# 새 quest 생성 후 슬러그만 캡처
SLUG=$(openguild quest new --type DEV --title "X" --json | jq -r '.quest_id')
openguild quest start "$SLUG"
```

dry-run 도 JSON 모드 지원 — 영향 분석을 프로그래밍으로 처리 가능.
