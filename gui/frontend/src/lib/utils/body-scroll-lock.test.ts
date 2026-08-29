import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import { lockBodyScroll } from './body-scroll-lock';

/**
 * BUG-199 의 잠금 장치. BUG-248 에서 검색 팔레트가 네 번째 사용처가 되면서
 * 테스트를 붙였다 — **새면 페이지가 영구히 스크롤되지 않는다.** 참조 계수가
 * 어긋나거나 해제가 두 번 불려 계수가 음수로 가면 그렇게 된다.
 */
describe('lockBodyScroll', () => {
	beforeEach(() => {
		document.body.style.overflow = '';
		document.body.style.overscrollBehavior = '';
	});

	it('잠그면 overflow / overscroll-behavior 를 건다', () => {
		const release = lockBodyScroll();
		expect(document.body.style.overflow).toBe('hidden');
		expect(document.body.style.overscrollBehavior).toBe('contain');
		release();
	});

	it('풀면 원래 값으로 되돌린다', () => {
		document.body.style.overflow = 'scroll';
		document.body.style.overscrollBehavior = 'auto';
		const release = lockBodyScroll();
		expect(document.body.style.overflow).toBe('hidden');
		release();
		expect(document.body.style.overflow).toBe('scroll');
		expect(document.body.style.overscrollBehavior).toBe('auto');
	});

	/** 모달 위에 모달 — 마지막 하나가 풀릴 때만 되돌아가야 한다. */
	it('겹쳐 잠그면 마지막 해제에서만 풀린다', () => {
		const a = lockBodyScroll();
		const b = lockBodyScroll();
		a();
		expect(document.body.style.overflow).toBe('hidden'); // 아직 b 가 남았다
		b();
		expect(document.body.style.overflow).toBe('');
	});

	/**
	 * 같은 해제 함수를 두 번 불러도 계수가 더 줄면 안 된다. 줄면 다른 모달이
	 * 아직 열려 있는데 잠금이 풀리거나, 반대로 계수가 꼬여 영영 안 풀린다.
	 */
	it('같은 해제를 두 번 불러도 안전하다', () => {
		const a = lockBodyScroll();
		const b = lockBodyScroll();
		a();
		a(); // 중복 호출 — 무시돼야 한다
		expect(document.body.style.overflow).toBe('hidden');
		b();
		expect(document.body.style.overflow).toBe('');
	});

	it('잠금 없이 해제를 반복해도 다음 잠금이 정상 동작한다', () => {
		const a = lockBodyScroll();
		a();
		a();
		a();
		const b = lockBodyScroll();
		expect(document.body.style.overflow).toBe('hidden');
		b();
		expect(document.body.style.overflow).toBe('');
	});

	/**
	 * BUG-257: 스크롤하는 것이 문서가 아니라 `<main>` 이 됐다. body 에 잠금을
	 * 걸면 body 는 이미 `overflow: hidden` 이라 아무 일도 안 일어나고 모달 뒤
	 * 페이지가 그대로 스크롤된다(= BUG-199 회귀). 그래서 잠금은 **그때의
	 * 스크롤 컨테이너**를 잡아야 한다.
	 */
	describe('스크롤 컨테이너가 있으면 그쪽을 잠근다', () => {
		let main: HTMLElement;

		beforeEach(() => {
			main = document.createElement('main');
			main.style.overflowY = 'auto';
			document.body.appendChild(main);
		});

		afterEach(() => {
			main.remove();
		});

		it('body 가 아니라 main 에 건다', () => {
			const release = lockBodyScroll();
			expect(main.style.overflow).toBe('hidden');
			expect(main.style.overscrollBehavior).toBe('contain');
			expect(document.body.style.overflow).toBe('');
			release();
			expect(main.style.overflow).toBe('');
		});

		it('잠근 뒤 main 이 갈려도 잠갔던 노드로 되돌린다', () => {
			const release = lockBodyScroll();
			const replaced = document.createElement('main');
			document.body.appendChild(replaced);
			release();
			// 새 main 을 건드리지 않고, 원래 잠근 쪽만 되돌아간다.
			expect(main.style.overflow).toBe('');
			expect(replaced.style.overflow).toBe('');
			replaced.remove();
		});
	});
});
