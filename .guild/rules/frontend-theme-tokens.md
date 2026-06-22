# Frontend Theme Tokens (DEV-074)

GUI 의 다크 / 라이트 / 시스템 테마 전환이 깨지지 않게 하기 위한 규칙.

## 1. CSS 안의 색은 토큰만

컴포넌트 `.svelte` 파일의 `<style>` 안에서 hex / rgb 직접 작성 금지.

```css
/* ❌ 잘못 — 라이트모드에서 흰 배경에 묻혀 안 보임 */
.label { color: #79c0ff; }

/* ✅ 올바름 — 테마 토큰 */
.label { color: var(--accent-secondary); }
```

토큰이 없는 색은 `gui/frontend/src/lib/styles/global.css` 의 `:root` (다크 기본) +
`[data-theme='light']` 양쪽에 신설 후 사용.

## 2. JS 의 색은 단일 palette helper

Cytoscape canvas 나 SVG data URL 안 (CSS `var()` 컴퓨팅 안 됨) 에서 색이
필요할 때 `gui/frontend/src/lib/stores/theme.ts` 의 `themePalette(eff)` 단일
source 사용.

```ts
// ❌ 잘못 — 같은 분기가 컴포넌트마다 따로 정의 → drift
const bg = eff === 'light' ? '#ffffff' : '#0d1117';

// ✅ 올바름 — 단일 source
import { themePalette } from '$lib/stores/theme';
const palette = themePalette(eff);
const bg = palette.bg;
```

## 3. 새 색은 dark / light 양쪽 정의

한쪽만 정의하면 다른 테마에서 깨짐. global.css 의 `:root` 와
`[data-theme='light']` 둘 다에 토큰 추가.

```css
:root {
    --my-new-color: #79c0ff;  /* 다크 */
}
[data-theme='light'] {
    --my-new-color: #0969da;  /* 라이트 */
}
```

## 4. 토큰은 용도에 맞게 — `--nav-*` 는 Nav 전용 (BUG-069)

토큰을 쓰더라도 **의미에 맞는** 토큰을 써야 한다. `--nav-bg` / `--nav-border`
는 상단 네비게이션(`Nav.svelte`) 전용 색이다. 일반 섹션 / 카드 / 버튼 / 표의
배경·경계에 쓰지 말 것.

```css
/* ❌ 잘못 — Nav 전용 토큰을 일반 surface 에 사용 */
section { background: var(--nav-bg); border: 1px solid var(--nav-border); }

/* ✅ 올바름 — 의미 토큰 */
section { background: var(--bg-elevated); border: 1px solid var(--border); }
```

라이트 테마에선 `--nav-bg` 값이 `--bg-elevated` 와, `--nav-border` 가 `--border`
와 우연히 같아서 **안 들킨다.** 하지만 다크 테마에선 `--nav-*` 가 보라빛이라
그 영역만 다른 섹션과 색이 달라진다. "토큰을 썼으니 OK" 가 아니라 — 잘못된
토큰은 hex 직접 작성과 똑같이 테마 깨짐의 원인.

| 용도 | 토큰 |
|------|------|
| 표면(카드/섹션/elevated) | `--bg-elevated` |
| 살짝 들어간 표면(버튼/입력) | `--bg-subtle` |
| 경계선 | `--border` |
| 네비게이션 바 (그 외 금지) | `--nav-bg` / `--nav-border` |

## 5. native picker — `color-scheme`

`<input type="date">` 같은 native control 의 아이콘 색은 `color-scheme`
property 가 결정. `global.css` 의 `:root` (dark) 와 `[data-theme='light']`
(light) 에 이미 정의됨 — **컴포넌트 단위로 override 금지**.

## 왜

DEV-074 fix1 ~ fix19 거치며 컴포넌트마다 색 처리 방식이 제각각 누적.
라이트모드에서 색 안 바뀌는 영역이 다수 발견됨 (2026-06-09 사용자 보고).
원인:
- 컴포넌트 CSS 에 hex 직접 사용 → 테마 무관 고정.
- JS 안 `eff === 'light' ? ...` 분기를 컴포넌트마다 따로 정의 → 중복 / drift.

위 규칙은 재발 방지용. 새 컴포넌트 / fix 추가 시 준수.

## 관련 문서

- `docs/guild-rules.md` § 프론트엔드 (Svelte) — agent 용 동일 규칙.
- DEV-074 quest 본문 — 후속 fix 계획.
