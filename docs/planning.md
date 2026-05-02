# OpenGuild 기획 논의 요약

## 구조 (확정)

```
OpenGuild (앱)
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
- 더블클릭으로 OpenGuild 실행 (파일 연결)
- CLI: `openguild.exe ./monitor` (`.guild` 파일 있으면 바로 오픈, 없으면 초기화 프롬프트)
- GUI에서도 동일하게 경로 선택 또는 최근 길드 목록에서 열기

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
| 앱 | OpenGuild | 오픈길드 |
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

| Jira | OpenGuild |
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

**이후 ❌**
| 항목 | 단계 |
|---|---|
| 다국어 | 추후 |
| Campaign | 추후 |
| 타입/상태 커스텀 | 추후 |
| Comment / Memo | 추후 |
| Quest History | 추후 |
| 브랜치명 표시 | 추후 |
| 등급/파티/멀티유저 | v1.1 이후 |

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

## 프로젝트 구조 (확정)

```
openguild/
├── backend/
│   ├── Cargo.toml        ← Cargo workspace 루트
│   ├── server/           ← 메인 API 서버 (Axum)
│   ├── core/             ← 공통 모델/로직 (server, tools 공유)
│   ├── tools/            ← 각종 툴 (로그 분석 등)
│   └── migrations/       ← DB 마이그레이션
├── frontend/             ← Svelte + Vite
├── docs/                 ← 기획/설계 문서
└── README.md
```

- 모노레포 (단일 저장소, 백엔드/프론트엔드 동시 수정 용이)
- 백엔드/프론트엔드 배포는 분리
- Cargo workspace로 server/core/tools 크레이트 관리

---

## 프로젝트 관리 (확정)

**저장소**: GitHub (모노레포)

> 📌 OpenGuild의 기본 기능이 완성되는 시점부터 GitHub Issues 대신 OpenGuild로 프로젝트를 관리할 예정 (dogfood). 브랜치명/이슈 ID 규칙은 동일하게 유지되므로 전환 시 혼란 없음.

**이슈 트래킹**: GitHub Issues
- Labels: `DEV`, `BUG`, `REQ` 로 타입 구분 (제목 형식 강제 없음)
- 브랜치명: `DEV-123`, `BUG-45` (OpenGuild prefix + GitHub 이슈 번호)

**브랜치 전략**
```
main      ← 릴리즈 전용 (버전 태그)
develop   ← 개발 통합
  └─ DEV-123  ← 기능/작업별 브랜치
  └─ BUG-45
```

**버전 관리**
- `MAJOR.MINOR.PATCH` (예: `0.1.0`)
- `0.x.x`부터 시작, 메이저 1은 명시적 승인 필요
- Git 태그: `v0.1.0`, GitHub Releases에 변경사항 기록

**CI/CD (GitHub Actions)**
- PR 시: `cargo check` (백엔드), `npm run build` (프론트엔드)
- `~/.cargo/registry`만 캐시 (target/ 제외)
- 배포: main 머지 시 AWS EC2 자동 배포

**배포**
- 백엔드: AWS EC2 t3.micro (Linux), MVP 이후 ECS Fargate 고려
- 프론트엔드: 정적 파일 별도 배포 (Vercel/Netlify 등)

**개발 도구**
- MCP: `plugin:engineering:github` (Issues/PR 관리)
- Skills: `engineering:code-review`, `engineering:debug`, `engineering:testing-strategy`, `engineering:deploy-checklist`
- CLAUDE.md: 프로젝트 구조, 주요 명령어, 브랜치 규칙, 아키텍처 메모

---

## 추후 기능 (미논의)

- GitHub Issues ↔ OpenGuild 연동 (Webhook 기반 Quest 자동 생성)

---

## 미논의 항목 (추후 논의 필요)

- 데이터 모델 상세 (quest_types / quest_statuses 테이블 구조 등)
- 등급/파티 시스템 (v1.1 이후)
- 멀티유저 단계 설계 (v1.5 이후)
