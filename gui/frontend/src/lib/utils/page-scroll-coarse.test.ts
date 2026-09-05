// BUG-264: 터치 기기에서는 **문서가 스크롤한다.**
//
// BUG-257 이 문서를 잠그고 `main` 을 스크롤 컨테이너로 만든 것은 macOS
// rubber-band 가 커스텀 타이틀바를 끌어내리는 것을 막기 위해서다. 모바일
// 브라우저에는 그런 타이틀바가 없는데 잠가 두니, 브라우저가 문서 스크롤을
// 볼 때만 하는 동작이 전부 죽었다 — 주소창 접힘, 당겨서 새로고침.
//
// **이 파일이 지키는 것은 CSS 와 JS 가 같은 조건으로 갈리는 것이다.**
// `global.css` / `+layout.svelte` 는 `@media (pointer: coarse)` 로 잠금을
// 풀고, `page-scroll.ts` 는 같은 질의로 `main` 대신 문서를 고른다. 둘이
// 어긋나면 화면은 문서를 스크롤하는데 JS 는 `main.scrollTop`(항상 0)을 읽어
// **스크롤 복원이 조용히 빗나간다.** 조용하다는 게 이 버그의 성질이라
// 테스트로 못박는다.
import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import { pageScrollEl, pageScrollTop, pageScrollHeight, pageViewportHeight } from './page-scroll';

/** `(pointer: coarse)` 만 갈아끼운다 — 다른 질의는 원래대로 둔다. */
function setPointer(coarse: boolean) {
	Object.defineProperty(window, 'matchMedia', {
		writable: true,
		configurable: true,
		value: (q: string) =>
			({
				matches: q.includes('pointer: coarse') ? coarse : false,
				media: q,
				addEventListener() {},
				removeEventListener() {}
			}) as unknown as MediaQueryList
	});
}

describe('pageScrollEl — 스크롤 주체가 환경에 따라 갈린다', () => {
	beforeEach(() => {
		document.body.innerHTML = '<main></main>';
	});
	afterEach(() => {
		// @ts-expect-error 테스트에서 심은 것을 걷는다.
		delete window.matchMedia;
	});

	it('정밀 포인터(데스크톱)에서는 main 이 스크롤 컨테이너다 — BUG-257 유지', () => {
		setPointer(false);
		expect(pageScrollEl()).toBe(document.querySelector('main'));
	});

	it('거친 포인터(터치)에서는 null — 문서가 스크롤한다', () => {
		setPointer(true);
		expect(pageScrollEl()).toBeNull();
	});

	it('main 이 있어도 터치면 null 이다 — 존재 여부로 고르면 안 된다', () => {
		setPointer(true);
		expect(document.querySelector('main')).not.toBeNull();
		expect(pageScrollEl()).toBeNull();
	});

	it('matchMedia 가 없는 환경(SSR/구형)은 잠금 쪽으로 물러선다', () => {
		// @ts-expect-error 의도적으로 지운다.
		delete window.matchMedia;
		expect(pageScrollEl()).toBe(document.querySelector('main'));
	});
});

describe('읽기 함수들이 터치에서 문서를 본다', () => {
	beforeEach(() => {
		document.body.innerHTML = '<main></main>';
		const main = document.querySelector('main') as HTMLElement;
		// main 쪽에만 값을 심어 둔다 — 터치에서 이 값을 읽으면 잘못된 것이다.
		Object.defineProperty(main, 'scrollTop', { value: 777, configurable: true });
		Object.defineProperty(main, 'scrollHeight', { value: 5000, configurable: true });
		Object.defineProperty(main, 'clientHeight', { value: 600, configurable: true });
		Object.defineProperty(document.documentElement, 'scrollHeight', {
			value: 9000,
			configurable: true
		});
		window.scrollY = 123;
		window.innerHeight = 800;
	});
	afterEach(() => {
		// @ts-expect-error 정리.
		delete window.matchMedia;
	});

	it('데스크톱은 main 값을 읽는다', () => {
		setPointer(false);
		expect(pageScrollTop()).toBe(777);
		expect(pageScrollHeight()).toBe(5000);
		expect(pageViewportHeight()).toBe(600);
	});

	it('터치는 문서 값을 읽는다 — main 의 0 을 읽으면 복원이 늘 맨 위로 간다', () => {
		setPointer(true);
		expect(pageScrollTop()).toBe(123);
		expect(pageScrollHeight()).toBe(9000);
		expect(pageViewportHeight()).toBe(800);
	});
});
