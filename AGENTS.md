# OpenGuild 프로젝트 컨텍스트

세션 시작 시 이 파일을 먼저 읽을 것.

---

## 프로젝트 개요

RPG 테마의 프로젝트 이슈 트래커. 자세한 기획은 `docs/planning.md` 참조.

## 절대 규칙

- **git commit은 사용자가 명시적으로 요청할 때만.**
- **메이저 버전 1은 사용자 명시적 승인 전까지 절대 사용 금지.** 현재 버전: `0.x.x`

---

## 프로젝트 구조

```
openguild/
├── backend/
│   ├── Cargo.toml   ← Cargo workspace
│   └── server/      ← Axum API 서버 (메인 바이너리)
├── frontend/        ← Svelte + TypeScript + Vite
├── tools/           ← 유틸리티 툴 (언어 무관, 독립적)
└── docs/            ← 기획/설계 문서
```

## 주요 명령어

**백엔드**
```bash
cd backend
cargo run -p server        # 서버 실행
cargo check                # 빠른 타입 체크
cargo test                 # 테스트 실행
```

**프론트엔드**
```bash
cd frontend
npm run dev                # 개발 서버
npm run build              # 프로덕션 빌드
npm run check              # 타입 체크
```

---

## 브랜치 / 이슈 규칙

- 브랜치명: `DEV-{이슈번호}`, `BUG-{이슈번호}`, `REQ-{이슈번호}`
- 이슈 트래커: GitHub Issues (Labels: DEV / BUG / REQ)
- `main` ← 릴리즈 전용, `develop` ← 개발 통합
- OpenGuild 기본 기능 완성 후 OpenGuild 자체로 이슈 관리 전환 예정 (dogfood)

---

## 핵심 용어

| EN | KO |
|---|---|
| Guild | 길드 (프로젝트 단위) |
| Campaign | 캠페인 (기획+목표) |
| Quest | 퀘스트 (이슈) |
| Sub-Quest | 서브퀘스트 |
| Quest Holder | 담당자 |
| Quest Board | 의뢰게시판 (노드 그래프) |
| Quest List | 퀘스트 목록 (리스트) |
| Urgency | 긴급도 |

전체 용어 목록: `docs/planning.md` 참조

---

## MVP 범위

포함: Quest CRUD, Quest Board (Cytoscape.js + 레인), Quest List (트리뷰), 서브/선행 퀘스트, 긴급도, 단일 사용자

제외 (추후): Campaign, 다국어, 타입/상태 커스텀, Comment/Memo, Quest History, 브랜치명 표시, 멀티유저

---

## 기술 스택

| 영역 | 선택 |
|---|---|
| 백엔드 | Rust + Axum |
| DB | SQLite + sqlx |
| 프론트엔드 | Svelte + TypeScript + Vite |
| 노드 그래프 | Cytoscape.js |
| 마크다운 편집 | CodeMirror 6 |
| 마크다운 렌더 | marked.js |

---

## 주의사항

- 멀티유저 확장을 염두에 두고 설계할 것 (JWT 인증 등)
- `tools/`는 Cargo workspace 밖 — Rust 외 언어도 가능
- 자세한 아키텍처: `docs/architecture.md` 참조
