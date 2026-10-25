+++
created_at = "2026-10-11T02:02:56+09:00"
updated_at = "2026-10-25T13:57:57+09:00"
version = 1
+++
# 디자인 언어 — DEV-427

GUI 를 디자인하거나 고칠 때는 도서관 **[[BOOK-009]] 디자인 언어** 에서 정한 대로 한다. 설명 · 단계표 ·
원칙은 거기에만 있다(여기 다시 적지 않는다 — 두 곳에 있으면 어긋난다).

## 지킬 것

- 글자 크기 · 간격 · 그림자 · 움직임 · 굵기 · 줄 간격은 **토큰만** 쓴다(`--fs-*` · `--sp-*` ·
  `--shadow-*` · `--ring` · `--dur-*` · `--fw-*` · `--lh-*`, 정본은 `gui/frontend/src/lib/styles/global.css`).
- 토큰이 맞지 않는 자리(아이콘 · × 글리프 크기 등)는 그 줄에 `/* token-ok: 이유 */`.
- 새 단계가 필요하면 admin 과 정하고 `global.css` 와 [[BOOK-009]] 를 함께 고친다.
- 버튼은 공통 클래스 `.btn` (+ `btn-primary` · `btn-danger` · `btn-warning` · `btn-quiet` · `btn-sm`) 을
  쓴다 — 화면마다 버튼 모양을 새로 정하지 않는다. 배치(margin 등)만 화면이 정한다([[DEV-428]]).
- 입력칸은 `.input`, 잠깐 뜨는 작은 창은 `.popup`, 모달은 `.modal-box` + 막 `var(--backdrop)` ([[DEV-440]]).
- 둘 중 하나 고르기는 `.seg`(+ `.seg.sm`), 검색칸은 `SearchField`([검색 | 쿼리]) ([[DEV-452]] · [[DEV-453]]).
- 아이콘은 `Icon`(SVG, 도형은 `utils/icon-paths.ts`) — 글자 기호(☰ ⚙ × ▼ …)를 아이콘으로 쓰지 않는다. 글자 자리에는
  `size="1em"`. 곱셈 기호 · 안내 글처럼 글자인 것은 그 줄에 `glyph-ok` ([[DEV-454]], `check:no-emoji` 가 막는다).
- 번역 문자열에 아이콘 글자를 붙이지 않는다('× 해제', '← 목록', '전체 설정 →' 가 아니라 말만) — 아이콘은 마크업의
  `Icon` 이 그린다. 세로 자리는 `Icon` 이 맞춘다(도형 가운데 = 글자 상자 가운데, 글꼴과 상관없이). 높이를 정해 둔
  아이콘만 있는 단추는 `display: inline-flex; align-items: center; justify-content: center`.

`npm run check:tokens` 가 리터럴과 화면별 공통 부품 모양을 막는다(`just test` · CI 에 포함).

함께 보는 규칙: 색은 `frontend-theme-tokens`, 배율(rem)은 `frontend-ui-scale`.
