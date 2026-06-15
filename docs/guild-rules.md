# Guild Rules — openguild 개발 규칙

> 이 프로젝트를 진행하면서 지켜야 할 규칙 모음.
> 구현 중 발견된 것들을 지속적으로 추가할 것.

---

## 커밋 & 브랜치

### 권한
- 커밋 / push 는 **사용자가 명시적으로 요청할 때만** 실행. amend / reset / force push 도 명시 요청 필요.

### 브랜치 전략

```
master       ─── 릴리즈 전용 (태그 v0.x.y)
  ↑ release merge only
develop      ─── 통합 / 검증 단계
  ↑ feature merge
DEV-001, DEV-002, BUG-045, REQ-007, ...  ─── feature 브랜치 (develop 기반)
```

- **master**: 릴리즈 전용. 직접 commit / push 금지. develop 에서 release 머지만 받음.
- **develop**: 통합 분기. 모든 feature 가 여기 모임. 일상적 작업의 기준 분기.
- **feature 분기**: `{PREFIX}-{N}` 형식. quest_id 를 그대로 사용 (예: `DEV-001`, `BUG-045`).
  - prefix 없음 (`feature/` 따위 금지).
  - 작업 시작: `git checkout develop && git pull && git checkout -b DEV-001`.
  - 작업 완료: PR → develop. 단일 개발자 단계엔 self-merge 허용 (squash 권장).

### 브랜치 ↔ Quest 연동

- branch 명 = quest_id (예: `DEV-001`).
- `openguild quest show DEV-001` 의 "권장 브랜치" 표시 (DEV-017 quest 의 목표) 도 이 규칙 따름.
- 추후 자동화 (보류): quest start → 자동 branch 생성 / branch push → quest in_progress.

### 커밋 메시지 형식

```
[{QUEST_ID}][{CATEGORY?}] 요약 한 줄

본문 (선택) — 무엇이 아니라 왜.
```

- `[QUEST_ID]` 필수. branch 의 quest_id 와 일치.
- `[CATEGORY]` 선택. 변경의 큰 분류 — `gui/desktop`, `gui/frontend`, `core`, `cli`, `server`, `docs`, `chore` 등.
- 예시:
  - `[DEV-002][gui/frontend] Tauri 환경 감지 어댑터`
  - `[DEV-002][core] invoke 핸들러 wiring`
  - `[BUG-045] --remote env override 무시되던 문제 수정`
  - `[DEV-019][server] check-drift 명령 추가`
- 본문 첫 줄 — 70자 이내. 본문은 빈 줄로 구분.
- 다중 카테고리는 별도 commit 으로 분리 권장 (각 commit 의 영역 명확).
- **한 commit 에 다른 quest 변경 섞지 말 것** (BUG-016 정책). 다른 quest 의
  파일이 stage 됐다면 `git reset HEAD <path>` 또는 별도 branch 로 분리.
- 무엇(what) 보다 **왜(why)** 중심 — diff 가 what 은 보여주므로.
- **Co-Authored-By 표기**: AI agent 가 작성한 commit 은 trailer 에
  `Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>` 추가.

### 머지 (2026-05-18 변경, BUG-016 강조)

- 기본 `git merge {QUEST_ID}` — linear 면 FF, 분기 시에만 자동 merge commit.
- `--no-ff` 강제 금지 (log 가 머지 커밋으로 지저분해짐).
- **머지된 feature 브랜치 삭제 금지** — 사용자가 명시적으로 삭제 요청할 때까지 보존.
- rebase 가 필요하면 (develop 이 앞서 갔을 때) feature → develop 위로 rebase 후
  FF merge — develop 의 linear 히스토리 유지.

### 릴리즈

- develop 가 안정된 시점에 `master` 로 머지 — fast-forward 또는 merge commit.
- master 에 태그: `v0.x.y` (semver, 0.x.x 부터 시작. 메이저 1 은 사용자 승인 필요).
- GitHub Releases 에 changelog 기록.

---

## 개발 순서 원칙

- 단계를 **건너뛰지 않는다** — 이전 단계가 완료되어야 다음 단계로 진행
- 단계 완료 기준: 계획 항목 구현 + 수동 테스트 통과 (자동 테스트 가능하면 추가)
- 계획에 없는 기능을 추가할 경우 **먼저 말하고 승인 받는다**

---

## 백엔드 (Rust)

- API 추가/변경 시 `docs/dev-plan.md` 또는 변경 이력에 **이유를 기록**한다
- 라우터 등록(`routes/mod.rs`)과 핸들러 구현은 항상 함께 커밋
- DB 스키마 변경은 반드시 migration 파일로 관리 (직접 ALTER 금지)
- 응답 타입 변경 시 프론트엔드 타입(`gui/frontend/src/lib/types/index.ts`)도 함께 수정. 추가로 `core::models` 가 진리원이므로 그쪽 우선 갱신

---

## 저장소 — 파일 진리 / DB 캐시 (불변 규칙)

- **파일이 진리원, `index.db` 는 파생 캐시.** `.guild/**` 의 `.md`/`.toml` 이 source of truth. `index.db` 는 언제든 `reindex` 로 파일에서 **무손실 재구축** 가능해야 한다 → **파일에서 파생되지 않는 값을 DB 에만 저장 금지** (DB-only 권위 상태 도입 금지).
- **모든 mutation 은 파일 + DB 동시 기록.** ops 경로(journal → SQL → 파일 write → auto-block)를 거친다. 한쪽만 바꾸지 말 것.
- **백업 ≠ 캐시.** 백업은 `backups/journal.db` + `snapshots/`. `index.db` 는 백업이 아니다 (캐시를 백업처럼 의존 금지).
- **읽기는 eventually-consistent.** 외부 편집 반영은 sync 지점으로만 — 시동 sync(DEV-121) / 상세 lazy(DEV-137) / 수동 ⟲(DEV-095). 신선도가 필요한 **새 read 경로**를 추가하면 어느 sync 지점이 그걸 덮는지 확인할 것 (목록류는 N-file stat 비용 고려).
- mtime 비교는 **Unix nanoseconds(절대 시각)** 로 — naive ISO string 금지(TZ 안전성).
- 상세: `docs/storage-design.md` § "파일 진리 ↔ 캐시 신선도 정책", guild rule `file-truth-db-cache`.

---

## 프론트엔드 (Svelte)

- 순수 함수(유틸, 필터, 트리 빌드 등)는 **vitest 단위 테스트 작성**
- UI 컴포넌트 테스트는 수동으로 진행 (Cytoscape 등 DOM 의존 컴포넌트)
- API 호출은 반드시 `$lib/api/` 레이어를 통해서만 — 컴포넌트에서 직접 fetch 금지
- `$state` / `$derived` Svelte 5 문법 사용, Svelte 4 방식(`writable` store 등) 혼용 금지
- 타입 에러 0개 유지 (`npm run check`)

### 테마 색 — DEV-074 (재발 방지)

- **컴포넌트 CSS 의 색은 토큰 (`var(--xxx)`) 만 사용.** hex (`#79c0ff` 등) 직접 작성 금지.
  토큰이 없는 색은 `lib/styles/global.css` 의 `:root` + `[data-theme='light']`
  양쪽에 신설 후 사용.
- **JS 가 색을 필요로 하는 경우** (Cytoscape canvas / SVG data URL — CSS `var()`
  컴퓨팅 못 함) `lib/stores/theme.ts` 의 `themePalette(eff)` 단일 source 사용.
  컴포넌트 안에서 `eff === 'light' ? '#x' : '#y'` 분기 작성 금지 — 중복 정의 /
  drift 의 원인.
- 새 색 추가 시 dark / light 양쪽 모두 정의. 한쪽만 정의하면 다른 테마에서 깨짐.
- **토큰은 용도(semantic)에 맞게 사용 — BUG-069 (재발 방지).** `--nav-*`
  (`--nav-bg` / `--nav-border`) 는 Nav 전용. `Nav.svelte` 외부의 surface /
  border 에 쓰지 말 것. 일반 표면은 `--bg-elevated` / `--bg-subtle`, 경계선은
  `--border` 사용. light 테마에선 `--nav-*` 값이 `--bg-elevated` / `--border`
  와 우연히 같아 안 들키지만 dark 에선 보라빛이라 섹션마다 색이 달라진다
  (토큰을 쓰긴 했어도 "잘못된 토큰" 이면 hex 직접 사용과 같은 문제).
- 사용자가 보는 in-app rule 은 `.guild/rules/frontend-theme-tokens.md` 참조.
- **enforcement — DEV-131**: 컴포넌트 CSS(`.svelte` 의 `<style>` + `.css`) 안
  hex 직접 사용은 CI 에서 차단(`npm run check:no-hex`,
  `gui/frontend/scripts/check-no-hex.mjs`). 토큰 정의처(`global.css`)만 allowlist.
  새 색은 `global.css` 의 `:root` + `[data-theme=light]` 양쪽에 토큰으로 추가 후
  사용. (mask 채널 등 비-테마 용도는 `black` 같은 키워드로.)

---

## 테스트

- 새 유틸 함수 추가 시 테스트 파일도 함께 만든다
- UI 기능 추가 시 "테스트 항목"을 문서화 (지금처럼) — 사용자가 직접 확인할 수 있도록
- `npm run test` 항상 통과 상태 유지

---

## 문서

- `docs/dev-plan.md`: 전체 개발 단계 계획 (변경 시 업데이트)
- `docs/planning.md`: 기획 결정 내용 (완료된 논의)
- `docs/guild-rules.md`: 이 파일 — 개발 규칙
- 백엔드 API 변경 이유, 계획 외 구현 항목은 해당 단계 완료 후 기록 남기기

---

## 미결 규칙 (추후 논의)

- PR 리뷰 기준 (멀티유저 단계 이후 의미 있음)
- 릴리즈 체크리스트 (8단계 CI/CD 설계 시 확정)
- 에러 처리 공통 패턴 (백엔드 에러 코드 ↔ 프론트 에러 메시지)
