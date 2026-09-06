+++
created_at = "2026-09-06T19:41:42+09:00"
updated_at = "2026-09-06T19:41:42+09:00"
+++
# 코드 컨벤션 (백엔드 / 프론트엔드 / 테스트)

`docs/guild-rules.md` 에 있던 것을 옮겼다([[DEV-371]]).

## 개발 순서

- 계획에 없는 기능을 추가할 때는 **먼저 말하고 승인받는다.**
- 단계 완료 기준은 "구현 + 검증 통과" 다. 자동 테스트로 덮을 수 있으면 덮고,
  사람이 봐야 하면 퀘스트 본문에 **테스트 방법**을 적고 Testing 으로 보낸다.

## 백엔드 (Rust)

- 라우터 등록(`server/src/routes/mod.rs`)과 핸들러 구현은 **항상 같이** 커밋한다.
- DB 스키마 변경은 반드시 migration 파일로. 직접 `ALTER` 금지.
- 응답 타입을 바꾸면 프론트 타입(`gui/frontend/src/lib/types/index.ts`)도 같이
  고친다. **`core::models` 가 진리원**이므로 그쪽을 먼저 바꾼다.
- HTTP 와 Tauri invoke 는 같은 `ops::` 함수를 거쳐야 한다 — 한쪽에만 로직을
  두면 두 경로의 동작이 갈린다([[BUG-231]]).

## 프론트엔드 (Svelte)

- 순수 함수(유틸, 필터, 트리 빌드)는 **vitest 단위 테스트를 함께** 만든다.
- API 호출은 `$lib/api/` 레이어를 통해서만. 컴포넌트에서 직접 `fetch` 금지.
- Svelte 5 문법(`$state` / `$derived` / `$effect`)만 쓴다. Svelte 4 방식
  (`writable` store 등) 혼용 금지.
- `npm run check` 타입 에러 0 유지.
- 색·곡률·여백·글꼴·뷰포트 단위는 각각 전용 규칙과 가드가 있다
  (`frontend-theme-tokens`, `frontend-ui-scale`, `check:*`).

## 테스트

- **`just test` 가 정본이다.** 러스트 + 프론트 + `npm run check` + 가드 + 프로덕션
  빌드까지 CI 와 같은 조합을 돈다. `cargo test` 와 `npm test` 만 돌리면 **가드를
  건너뛴다**([[DEV-372]]).
- 가드를 새로 만들면 `package.json` / `justfile` / `.github/workflows/check.yml`
  **세 곳 모두**에 등록한다. `check:guards` 가 이 정합성을 검사한다.
- **테스트가 실제로 무는지 확인한다** — 고친 것을 되돌려서 실패하는지 본다.
  이 저장소에서 통과만 보고 넘어갔다가 아무것도 안 잡는 검사를 만든 적이
  있다(`\bvh\b` 가 `100vh` 를 못 잡던 것 — [[BUG-264]]).
- 재현이 안 되면 **추측으로 고치지 않는다.** 계측기를 붙여 수치를 받거나,
  엔진 차이면 같은 엔진의 브라우저(사파리 ↔ WKWebView)를 대역으로 쓴다
  ([[BUG-268]] / [[BUG-269]]).
