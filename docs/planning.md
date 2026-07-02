# openguild 기획 논의 요약

> ⚠️ 이 문서는 초기 기획 논의(2026-05)의 **역사적 스냅샷**이다. 일부 "추후/예정/
> 미구현" 항목은 이미 구현됨(예: CI/CD, 다국어, 데스크톱 GUI). **현재 로드맵·
> 진행 상태의 진리원은 openguild 자체의 퀘스트/캠페인**(`openguild quest/campaign
> list`)이며, 이 문서는 당시 결정 기록으로만 참조할 것.

## 구조 (확정)

```
openguild (앱)
  └─ Guild (최상위 단위, 예: "모니터 길드")
       ├─ Campaign (기획+목표, 다음 업데이트 목표)
       │    └─ Quest 연결 (선택적)
       └─ Quest (DEV / BUG / REQ 등 커스텀 타입)
            └─ Sub-Quest
```

- Guild = 하나의 프로젝트 단위. "모니터 길드에 퀘스트를 생성한다" 처럼 사용
- Campaign = 다음 업데이트를 위한 기획서 + TODO 목록. Quest와 선택적으로 연결. 계층 강제 없음
- Quest = 개별 이슈. Campaign과 무관하게 생성 가능
- Sub-Quest = Quest 하위 작업. parent_quest_id로 처리
- 별도 "상위 Quest(Epic)" 개념 없음 — Sub-Quest로 충분

---

## 길드 열기 방식 (확정)

- 각 길드는 사용자가 원하는 경로에 독립 저장 (VS Solution 단위와 동일)
- 마커 파일: `{길드명}.guild` (TOML 형식)
- 더블클릭으로 openguild 실행 (파일 연결, 데스크톱 단계 예정)
- CLI: cwd 자동 탐색 또는 `openguild --guild ./monitor ...` (`.guild` 없으면 `openguild init` 안내)
- GUI: 디렉토리 선택 또는 최근 길드 목록 (데스크톱 단계 예정)

### {name}.guild 파일 내용 (TOML)
```toml
name = "모니터"
version = "1.0"
created_at = "2026-05-01"
```
- 단순 마커가 아닌 길드 메타 포함 → 앱 시작 화면에서 DB 없이도 길드명 표시 가능

---

## 용어 테이블 (확정)

| 일반 용어 | EN | KO |
|---|---|---|
| 앱 | openguild | 오픈길드 |
| 프로젝트 단위 | Guild | 길드 |
| 기획+목표 | Campaign | 캠페인 |
| 이슈 | Quest | 퀘스트 |
| 서브이슈 | Sub-Quest | 서브퀘스트 |
| 선행 이슈 | Prerequisite Quest | 선행 퀘스트 |
| 담당자 | Quest Holder | 담당자 |
| 이슈 등록자 | Requester | 의뢰인 |
| 우선순위 | Urgency | 긴급도 |
| 이슈 보드 (노드) | Quest Board | 의뢰게시판 |
| 이슈 목록 (리스트) | Quest List | 퀘스트 목록 |
| 상태 변경 이력 | Quest History | 퀘스트 히스토리 |
| 코멘트 (공개) | Comment | 댓글 |
| 메모 (비공개) | Memo | 메모 |
| 알림 | Notice | 공지 |
| 프로젝트 관리자 | Guild Master | 길드마스터 |
| 팀원 | Guild Member | 길드원 |
| 팀/부서 | Party | 파티 |
| 멤버 등급 | Rank | 등급 |

---

## 기타 확정 사항

| 항목 | 확정 내용 |
|---|---|
| 길드 마커 파일 | `{길드명}.guild` (TOML 형식) |
| 브랜치명 형식 | `DEV-001` (타입 prefix + ID, 제목 미포함) |
| 퀘스트 타입 | 길드마스터 커스텀 가능, `quest_types` 별도 테이블. 기본값: DEV/BUG/REQ |

---

## Jira 대응표

| Jira | openguild |
|---|---|
| Project | Guild |
| Version / Release | Campaign |
| Epic | (상위 Quest로 처리, 별도 개념 없음) |
| Story / Task / Bug | Quest |
| Sub-task | Sub-Quest |
| Board | Quest Board |
| Backlog | Quest List |
| Comment | Comment / 댓글 |
| — | Memo / 메모 (비공개, Jira에 없음) |

---

## 퀘스트 속성 (확정)

### 타입 (Type)
- 속성: `prefix` + `color` + `description`(선택), 이름 없음
- 기본값: DEV / BUG / REQ
- 길드마스터 추가/수정/삭제 가능
- ID 형식: `{PREFIX}-{번호}` (예: DEV-001, BUG-003)

### 긴급도 (Urgency)
- 4단계 고정, 커스텀 불가
- 영어 고정, 다국어 없음

| 레벨 | EN | 색상 |
|---|---|---|
| 1 | Critical | 빨강 |
| 2 | High | 주황 |
| 3 | Medium | 노랑 |
| 4 | Low | 회색 |

### 상태 (Status)
- 기본값: 게시됨 / 진행 중 / 완료 / 취소됨 / 보류
- 기본값 포함 모든 상태 삭제/수정 가능
- "기본 상태 불러오기" 버튼으로 기본값 복원
- 속성: `name_en` + `name_ko` + `color`
- 커스텀 상태 추가 가능

---

## Campaign 상세 (확정)

| 속성 | 내용 |
|---|---|
| 제목 | 필수 |
| 본문 | 마크다운 (기획 내용) |
| 체크리스트 | 간단한 TODO 항목, Quest와 무관 |
| 연결 Quest | Quest에서 Campaign 태깅, 선택사항 |
| 기간 | 시작일 / 종료일, 선택사항 |
| 상태 | 활성 / 완료 |

- Campaign 상태와 연결 Quest 상태는 완전히 독립
- 체크리스트 ↔ Quest 전환 기능 없음

---

## 데이터 모델 변경사항 (확정)

**신규 테이블**
```sql
campaigns           -- 제목, 본문(md), 기간, 상태
campaign_checklists -- 체크리스트 항목
campaign_quests     -- Campaign ↔ Quest 연결 (다대다)
quest_types         -- prefix, color, description(선택)
quest_statuses      -- name_en, name_ko, color, is_default
```

**변경**
```sql
quests  -- type → quest_type_id
        -- status → quest_status_id
```

**제거**: repository 관련 없음

**유지**: guilds / ranks / users / guild_members / parties / party_members / quest_counters / quest_dependencies / quest_assignees / comments / history / quest_positions

---

## 뷰 구성

### Quest Board (확정)

**노드**
- ID + 제목 표시
- 긴급도 색상으로 구분
- 담당자 지정 여부 표시 (아이콘 등, 이름 미표시)

**레인**
- `quest_statuses` 기반 동적 생성
- 노드를 레인 안에서 자유 위치 이동 + 저장
- 노드를 다른 레인으로 드래그 → "이 상태로 변경할까요?" 확인 후 상태 변경

**화살표**
- 선행 퀘스트: 실선 화살표
- 서브퀘스트: 다른 모양 화살표 (점선 등)
- 레인 간 화살표 가로지르기 가능

**필터/검색**
- Campaign 필터링 포함, 상세 스펙은 추후

**미포함**
- 그룹핑 없음 (Campaign 상세 화면에서 확인)

### Quest List (확정)

**구조**
- 섹션 구분 없는 단일 리스트
- 서브퀘스트는 부모 하위 트리 형태 (접기/펼치기)

**각 항목 표시 정보**
- ID, 제목, 타입, 긴급도, 담당자, 연결된 캠페인

**전환**
- 탭 UI: `[Quest Board] [Quest List]`

**상태 변경**
- 리스트에서 지원 안 함
- Quest Detail 또는 Quest Board에서만 변경 가능

### Quest Detail (확정)

**메타 정보 헤더**
- ID, 타입, 긴급도, 상태, 담당자, 의뢰인, 연결된 Campaign

**권장 브랜치명**
- `권장 브랜치 이름: [DEV-001] [복사]` 형태로 표시

**본문**
- `.md` 파일 렌더링 + 편집 모드 토글 (CodeMirror 6 + marked.js)

**하위 섹션**
- 서브퀘스트 목록
- 선행 퀘스트 목록
- Comment (공개 댓글)
- Memo (비공개 메모)
- Quest History (상태 변경 이력)

---

## MVP 범위 (확정)

**포함 ✅**
| 항목 | 비고 |
|---|---|
| Quest CRUD | 생성/조회/수정/삭제 |
| Quest List | 트리 형태 서브퀘스트 |
| Quest Board | Cytoscape.js + 레인 + 위치 저장 |
| 서브퀘스트 / 선행 퀘스트 | 관계 표시 |
| 긴급도 | 4단계 고정 |
| 퀘스트 타입 | DEV/BUG/REQ 하드코딩 |
| 퀘스트 상태 | 기본 5개 하드코딩 |
| guild.guild 마커 파일 | |
| 단일 사용자 | |

**이후 (대부분 구현됨)**
| 항목 | 상태 |
|---|---|
| 다국어 | 추후 |
| Campaign | ✅ 구현 (DEV-011) |
| 타입/상태 커스텀 | ✅ 구현 (DEV-014 / DEV-046) |
| Comment (`{slug}.comments.md`) | ✅ DEV-094 (file) + DEV-102 (DB 캐시 + snapshot 백업). |
| Memo (`{slug}.memo.md`) | ✅ DEV-099 (file) + DEV-102 (DB 캐시 + snapshot 백업, user_id=0 sentinel). |
| Quest History | ✅ 구현 (DEV-013) |
| 브랜치명 표시 | ✅ Quest Detail 헤더에 표시 |
| 길드 규칙 | ✅ `.guild/rules/*.md` + CLI |
| 등급/파티/멀티유저 (JWT) | v1.1 이후 — DEV-021 |

---

## 기술 스택 (확정)

| 영역 | 선택 | 비고 |
|---|---|---|
| 백엔드 | Rust (Axum) | 단독 바이너리 배포, 런타임 불필요 |
| DB | SQLite + sqlx | |
| 프론트엔드 | Svelte + Vite | |
| 노드 그래프 | Cytoscape.js | |
| 마크다운 편집 | CodeMirror 6 | |
| 마크다운 렌더 | marked.js | |
| 패키징 | Rust 기본 바이너리 | |

> ⚠️ MVP는 단일 사용자이지만, 팀/조직 단위 멀티유저로 확장을 목표로 함.
> 처음부터 인증(JWT), 권한 관리를 염두에 두고 아키텍처를 설계할 것.

---

## 프로젝트 구조 (2026-05-14 갱신)

```
openguild/
├── Cargo.toml            ← workspace, members = ["core", "cli", "server", "gui"]
├── core/                 ← lib: 도메인 로직 + sqlx + migrations
├── cli/                  ← bin `openguild` (로컬/원격)
├── server/               ← bin Axum API 서버
├── gui/                  ← Tauri v2 desktop (DEV-003 ~ DEV-005 완료, DEV-006 남음)
│   ├── src/              ← Rust shell + invoke 핸들러
│   └── frontend/         ← Svelte + Vite (SvelteKit static)
├── docs/                 ← 기획/설계 문서
├── justfile              ← 개발 단축 명령
└── README.md
```

- 모노레포 (단일 저장소, Rust/JS 동시 수정 용이)
- 컴포넌트 배포는 분리 (server, gui/frontend 별도 배포 가능)
- Cargo workspace 로 core/cli/server/gui 관리.
- 상세 설계 근거: `docs/architecture-refactor.md`

---

## 프로젝트 관리 (확정 / 2026-05-17 갱신)

**저장소**: GitHub (모노레포)

**이슈 트래킹**: openguild 자체 dogfood (2026-05-17 전환 완료)
- `.guild/quests/*.md` 가 진리원 (git tracked).
- 새 작업 = `openguild quest new` → `.guild/quests/{ID}.md` 자동 생성.
- GitHub Issues 보조 사용 X. 외부 todo 도구 X.

**브랜치 전략**
```
master    ← 릴리즈 전용 (태그 v0.x.y, 직접 push 금지)
develop   ← 통합 / 검증 (default 작업 분기)
  └─ DEV-123 ← feature 브랜치 (quest_id 직접, `feature/` prefix 없음)
  └─ BUG-45
```

- `master` 유지 (rename 안 함).
- 모든 새 작업은 develop 에서 분기 (`git checkout -b DEV-N`).
- 머지: feature → develop (squash 권장) → master (릴리즈 시점).

**커밋 메시지 형식**
```
[{QUEST_ID}][{CATEGORY?}] 한 줄 요약

본문 (선택). what 보다 why.
```

- `[QUEST_ID]` 필수 (branch 의 quest_id 와 일치).
- `[CATEGORY]` 선택: `gui/desktop` / `gui/frontend` / `core` / `cli` / `server` / `docs` / `chore` 등.
- 메타 변경 (브랜치 전략 같이 quest 없음) 은 `[chore][docs] ...` 일회성 예외.

**버전 관리**
- `MAJOR.MINOR.PATCH` (예: `0.1.0`)
- `0.x.x`부터 시작, 메이저 1은 명시적 승인 필요
- Git 태그: `v0.1.0`, GitHub Releases에 변경사항 기록

**CI/CD (GitHub Actions)** — 미구현 (DEV-008 quest)
- PR 시: `cargo test --workspace` + clippy + `npm run check` + `npm test`
- `~/.cargo/registry`만 캐시 (target/ 제외)
- 배포: master 머지 시 자동 배포 검토

**배포**
- 백엔드: AWS EC2 t3.micro (Linux), MVP 이후 ECS Fargate 고려
- 프론트엔드: 정적 파일 별도 배포 (Vercel/Netlify 등)

**개발 도구**
- MCP: `plugin:engineering:github` (Issues/PR 관리)
- Skills: `engineering:code-review`, `engineering:debug`, `engineering:testing-strategy`, `engineering:deploy-checklist`
- AGENTS.md: 절대 규칙 (commit / 브랜치 / commit 메시지) + 문서 인덱스

---

## 추후 기능 (미논의)

- GitHub Issues ↔ openguild 연동 (Webhook 기반 Quest 자동 생성)

---

## 미논의 항목 (추후 논의 필요)

- 데이터 모델 상세 (quest_types / quest_statuses 테이블 구조 등)
- 등급/파티 시스템 (v1.1 이후)
- 멀티유저 단계 설계 (v1.5 이후)

---

## 추후 구현 메모

### 길드 규칙 (Guild Rules)
길드(프로젝트) 단위로 팀이 지켜야 할 규칙을 정의하고 공유하는 기능.
통상적인 개념으로는 "팀 규칙", "그라운드 룰", "개발 컨벤션" 등에 해당.

예시: 브랜치 네이밍 규칙, 커밋 메시지 형식, 코드 리뷰 기준, 배포 체크리스트 등을 길드 내에서 문서화하고 공유.

- 구체적인 스펙(UI, 데이터 모델 등)은 미논의
- MVP 이후 단계에서 설계 필요

### 다음 퀘스트(Successor) / 부모 퀘스트 직접 변경
현재 Quest Detail에서 명시적으로 지원하지 않는 관계 조작:

- **다음 퀘스트(Successor)**: 이 퀘스트가 다른 퀘스트의 선행이 되도록 지정. `quest_dependencies`만으로 표현 가능 (별도 테이블 X). 추가 시 사이클 검증 필수.
- **부모 퀘스트 직접 변경**: Quest Detail에서 자기 자신의 `parent_quest_id`를 직접 갱신. 사이클 검증(자손은 부모 될 수 없음) 필수.

7단계에서 의도적으로 제외함. 간접적으로는 다음 방법으로 동일 효과 가능:
- "다음 퀘스트" → 그 퀘스트 상세에서 이 퀘스트를 선행으로 추가
- "부모 변경" → 새 부모 상세에서 이 퀘스트를 서브로 지정

추후 직접 조작 UI가 필요하다고 판단되면 검토.

### 서브퀘스트 부모 점유 정책 (확정)
서브퀘스트 콤보박스에서 **이미 다른 부모를 가진 퀘스트는 후보에서 제외**한다.
한 번에 한 부모만이라는 모델을 단순하게 유지. 이동하려면 기존 부모에서 먼저 분리(× 버튼)한 뒤 재지정.
