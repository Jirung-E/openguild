+++
created_at = "2026-08-27T03:19:14+09:00"
updated_at = "2026-08-27T03:19:14+09:00"
+++
# UI 배율 (rem) 규칙 — BUG-246 / 252 / 253 / 254

설정의 UI 크기 조절(DEV-101)은 `<html>` 의 `font-size` 를 바꾼다(기본 16px,
0.5 ~ 2.0 배). 따라서 **rem 으로 쓴 값만 배율을 따라간다.** px 로 쓴 값은
그 자리에 고정된다 — 상자만 커지고 안의 글자·아이콘은 그대로여서 잘리거나
경계를 넘는다.

같은 함정으로 버그가 **네 번** 났다. 아래는 그때마다 새로 드러난 새는 곳들이다.

## 1. 길이는 rem — px 는 예외일 때만

```css
/* ❌ 배율을 안 따라간다 */
.sidebar { width: 260px; }

/* ✅ */
.sidebar { width: 16.25rem; }
```

**px 를 그대로 두는 것이 맞는 곳**도 있다 — OS 크롬이라 OS 가 정한 치수를
따라야 하는 경우다. Windows / Linux 타이틀바의 창 버튼(`.tb-btn`,
`.tb-winicon`)이 그렇다([[BUG-246]]). 이건 실수가 아니므로 주석으로 이유를
남긴다.

## 2. 조사할 때 빠뜨리기 쉬운 속성

[[BUG-253]] 이 `width` / `height` / `min-*` / `max-*` / `font-size` 만 훑어
아래를 놓쳤고, 그래서 [[BUG-254]] 가 다시 났다.

- `grid-template-columns`
- `flex-basis`, `flex` 의 basis 성분
- `contain-intrinsic-size` (값이 실제 높이와 정확히 맞아야 한다 — [[BUG-244]])
- `gap`, `padding`, `inset` 계열

## 3. SVG 의 `width` / `height` **속성**은 px 다

가장 많이 걸린 곳. 프레젠테이션 속성은 단위가 px 라 root font-size 와 무관하다.

```svelte
<!-- ❌ size 는 호출측이 주는 px 숫자다. 배율을 안 따라간다 -->
<svg width={size} height={size}>

<!-- ✅ 컴포넌트에서 rem 으로 환산 — 호출부를 안 고쳐도 된다 -->
const dim = $derived(`${size / 16}rem`);
<svg width={dim} height={dim}>
```

`16` 으로 나누면 **기본 배율에서 크기가 그대로**라 회귀가 없다.

인라인 SVG 를 직접 쓸 때는 속성을 폴백으로 두고 CSS 로 덮는다 — 속성은 CSS
보다 우선순위가 낮다.

> **새 아이콘 컴포넌트를 만들 때 특히 조심.** [[BUG-254]] 는 `Icon.svelte` 만
> 고치고 끝냈는데, `PlayPauseIcon.svelte` 라는 **형제 컴포넌트**가 같은 결함을
> 갖고 있어 다시 보고됐다. `size` prop 을 받아 SVG 속성에 넣는 컴포넌트를
> 새로 만들면 이 규칙이 그대로 적용된다.

## 4. 아이콘 + 글자 버튼은 부모를 flex 로

인라인 SVG 는 **글자의 기준선(baseline) 위에 얹힌다.** 글자에는 기준선 아래로
descender 가 더 있으므로 아이콘만 위로 떠 보이고, 배율을 올리면 어긋남도 같이
커진다.

```css
/* ✅ 아이콘과 글자를 같이 넣는 버튼/라벨 */
.btn-with-icon {
    display: inline-flex;
    align-items: center;
    gap: 0.3em;
}
```

`Icon.svelte` 의 SVG 에는 `vertical-align: middle` 이 기본으로 걸려 있지만
그건 x-height 기준이라 완전히는 못 맞춘다(실측 200% 에서 1.63px 남음).
**부모 flex 가 정답**이고(0.23px), `vertical-align` 은 안전망이다.

정사각형 버튼 안에 아이콘 하나만 넣을 때도 마찬가지다 — `justify-content:
center` 까지 줘야 두 축 모두 중앙에 온다.

## 확인 방법

배율 200% 로 올리고 본다. 숫자로 보려면 브라우저 콘솔에서:

```js
// 배율을 직접 적용 (앱이 하는 것과 동일한 쓰기)
document.documentElement.style.fontSize = '32.00px';
// 어떤 요소가 2배가 됐는지 확인
document.querySelector('.foo').getBoundingClientRect();
```

아이콘 정렬 감사는 이렇게 훑을 수 있다:

```js
[...document.querySelectorAll('svg')].filter((s) => {
    const p = s.parentElement;
    const hasText = [...p.childNodes].some((n) => n.nodeType === 3 && n.textContent.trim());
    return hasText && !/flex|grid/.test(getComputedStyle(p).display);
});
```

빈 배열이어야 한다.

## 아직 자동 검사가 없다

색은 `npm run check:no-hex` 가 CI 에서 막는데, 길이 단위에는 그런 검사가 없다.
[[DEV-369]] 에 곡률·테두리 리터럴 정리와 함께 재발 방지 검사를 적어 뒀다 —
그게 생기기 전까지는 이 규칙이 사람 손에 달려 있다.

## 관련

- [[frontend-theme-tokens]] — 색 토큰 규칙(같은 성격의 재발 방지 규칙)
- `docs/guild-rules.md` § 프론트엔드 — agent 용 동일 규칙
