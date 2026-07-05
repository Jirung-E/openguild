# openguild 사용 가이드

> 처음 설치한 사용자를 위한 빠른 시작 + 핵심 기능 안내.

---

## 1. 첫 실행

설치 후 시작 메뉴 또는 바탕화면에서 **openguild** 를 실행하면 Welcome 화면이
열립니다. 길드를 새로 만들거나, 기존 길드 폴더를 열 수 있습니다.

### 길드 만들기

길드 = 한 프로젝트 단위. (예: "내 개인 프로젝트", "팀 X 의 사이드 프로젝트")

1. Welcome 화면에서 **"새 길드 만들기"** 클릭.
2. 빈 폴더 선택 (예: `C:\Users\me\Projects\my-app`).
3. 길드 이름 입력 (예: `my-app`).
4. 자동으로 `my-app.guild` 마커 파일 + `.guild/` 디렉토리 (캐시 / 백업) 생성.

### 기존 길드 열기

- Welcome 의 **"폴더 열기"** → 길드 폴더 선택.
- 또는 탐색기에서 `*.guild` 파일을 더블클릭 (파일 연결 등록됨).
- 최근에 열었던 길드는 자동 목록에 표시.

---

## 2. 핵심 개념

| 용어 | 의미 |
|------|------|
| **Guild** | 한 프로젝트 단위. 폴더에 `{name}.guild` 마커 파일로 표시. |
| **Quest** | 개별 이슈 (작업 / 버그 / 요청). 타입 + 번호로 식별 (`DEV-001`, `BUG-045` 등). |
| **Sub-quest** | quest 의 하위 작업. parent quest 가 자동으로 sub 목록 갖춤. |
| **Prerequisite** | 선행 quest. A 의 prereq B → A 시작 전 B 가 필요. |
| **Campaign** | 마일스톤 / 다음 업데이트 묶음. quest 들을 선택적으로 링크. |
| **Status** | quest 의 진행 상태 (open / in_progress / testing / done / on_hold / cancelled / returned). 사용자 정의 가능. |
| **Urgency** | 1=Critical / 2=High / 3=Medium / 4=Low. |

---

## 3. 데이터는 어디에?

`.guild/` 폴더 안:

```
my-app/
├── my-app.guild              ← 길드 마커 (TOML)
└── .guild/
    ├── quests/
    │   ├── DEV-001.md        ← quest 본문 (frontmatter + markdown)
    │   ├── DEV-001.comments.md  ← 공개 댓글 (git 공유)
    │   └── DEV-001.memo.md      ← 비공개 메모 (gitignored)
    ├── campaigns/            ← 캠페인
    ├── types/                ← quest 타입 정의 (DEV / BUG / REQ ...)
    ├── statuses/             ← 상태 정의
    ├── rules/                ← 길드 규칙 문서
    ├── index.db              ← 쿼리 캐시 (자동 재구축 가능)
    └── backups/              ← 자동 백업 (journal + snapshot)
        ├── journal.db
        └── snapshots/
```

### 텍스트 에디터로 직접 열어보세요

각 quest 는 평범한 Markdown 파일. VS Code, Obsidian, GitHub web 등 어디서든
편집·검색 가능합니다. 깊이 들어간 도구 없이도 데이터를 직접 확인하세요.

### git 친화

`.guild/quests/*.md`, `.guild/campaigns/*.md`, `.guild/rules/*.md`, `.guild/types/`,
`.guild/statuses/` 모두 git tracked 권장. diff / blame / branch / PR 자연스럽게
활용 가능.

`.guild/index.db` / `.guild/backups/` / `.guild/positions.json` 은 자동으로
`.gitignore` 처리. 손실되어도 파일에서 재구축 가능.

### git 안 써도 안전

`backups/journal.db` (모든 변경 의도 기록) + `backups/snapshots/*.db` (시점별
사본) 으로 시점 복원 가능. 설정 화면의 "백업 / 복구" 메뉴 사용.

---

## 4. CLI 도구

GUI 와 함께 `openguild` (CLI) + `openguild-server` (HTTP 서버) 가 설치됩니다.
설치 시 "Add to PATH" 옵션을 체크했으면 명령 프롬프트에서 바로 실행 가능.

### 자주 쓰는 명령

```bash
# 현재 폴더의 길드를 자동 탐색해서 실행. 없으면 init 안내.
openguild quest list

# 새 quest
openguild quest new --type DEV --title "API 추가"

# 상태 변경
openguild quest status DEV-001 in_progress

# 댓글 추가 (stdin 으로 본문)
echo "디자인 확정함." | openguild quest comment add DEV-001 --author alice

# 메모 (비공개)
echo "TODO: foo 확인" | openguild quest memo set DEV-001

# 길드 규칙 (top-level 은 `rule` — `rules` alias 도 동작)
openguild rule list
echo "내용" | openguild rule new branch-policy

# 캠페인
openguild campaign new --title "베타 1.0"
openguild campaign link C-001 DEV-001
```

전체 명령은 `openguild --help` / `openguild <명령> --help` 참조.

### 원격 모드

서버 모드로 띄운 다른 곳의 길드를 조작하려면:

```bash
openguild --remote https://my-team.example/api quest list
# 또는 환경변수
$env:OPENGUILD_REMOTE = "https://my-team.example/api"
openguild quest list
```

---

## 5. 데스크탑 앱 주요 화면

| 화면 | 용도 |
|------|------|
| **Home** | 진행 중 캠페인 carousel / 마감 임박 / 최근 quest. |
| **Quest List** | 트리 / 평면 quest 목록 + 필터. |
| **Quest Board** | Cytoscape 노드 그래프 (선행 / 서브 관계 시각화). |
| **Quest Detail** | 본문 편집 / 댓글 / 메모 / 상태 변경 / 권장 브랜치명 / 캠페인 링크. |
| **Campaigns** | 캠페인 목록 + 체크리스트 + 링크된 quest. |
| **Rules** | 길드 규칙 다중 파일 편집. |
| **Settings** | 정보 / 업데이트 확인 / 백업 / drift / reindex / 타입·상태 관리. |

---

## 6. 백업 / 복구

- **자동 snapshot**: 변경 50 회 또는 24 시간 마다 자동 (`.guild/backups/snapshots/`).
- **수동 snapshot**: Settings → Admin → "즉시 백업".
- **복구**: Settings → Admin → snapshot 선택 → 복원. 기존 `index.db` 는
  `.pre-restore.db` 로 안전 보관.

설치된 사본 위치 기본값:
`C:\Program Files\openguild\` (Windows) — 단 사용자 설정으로 변경 가능.

---

## 7. 자동 업데이트

설치된 binary 는 시작 시 GitHub release 의 `latest.json` 을 조회해서 새 버전
감지. Settings → "업데이트 확인" 으로 수동 확인도 가능. 다운로드 → 서명 검증
→ 재시작 흐름은 자동.

---

## 8. 도움말 / 문제 해결

| 상황 | 해결 |
|------|------|
| 파일을 직접 편집했는데 GUI 가 옛 상태 | 시동 시 자동 reindex 합니다. 또는 상단 reindex 버튼. |
| 다른 PC 에서 git pull 후 quest 안 보임 | 위와 동일. 자동 / 수동 reindex. |
| 캐시 손상 의심 | Settings → "Drift 검사" → 필요 시 Reindex. |
| 데이터 손실 의심 | Settings → Admin → 최근 snapshot 복원. |
| CLI 가 인식 안 됨 | 설치 시 "Add to PATH" 체크 안 했을 가능성. 재설치 또는 수동 PATH 등록. |

문의 / 버그 보고: https://github.com/Jirung-E/openguild
