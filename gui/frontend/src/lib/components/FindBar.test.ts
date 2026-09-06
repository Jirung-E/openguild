// BUG-268: 찾기 창을 닫아도 강조가 남는다(admin 보고 — 노란색·파란색 둘 다).
//
// jsdom 에는 CSS Custom Highlight API 가 없다. 없으면 `supportsHighlightApi()`
// 가 false 라 `canHighlight` 가 꺼지고 **칠하지도 지우지도 않는다** — 그
// 상태로 테스트하면 아무것도 검증하지 못한다. 그래서 컴포넌트가 초기화되기
// **전에** 대역을 세운다(`canHighlight` 는 초기화 시점에 한 번 계산된다).
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/svelte';

/** `CSS.highlights` 대역 — set/delete 호출을 그대로 기록한다. */
class FakeRegistry {
	map = new Map<string, unknown>();
	calls: string[] = [];
	set(name: string, h: unknown) {
		this.calls.push(`set:${name}`);
		this.map.set(name, h);
		return this;
	}
	delete(name: string) {
		this.calls.push(`delete:${name}`);
		return this.map.delete(name);
	}
	get(name: string) {
		return this.map.get(name);
	}
}

let registry: FakeRegistry;

beforeEach(() => {
	registry = new FakeRegistry();
	// eslint-disable-next-line @typescript-eslint/no-explicit-any
	(globalThis as any).CSS = { highlights: registry };
	// eslint-disable-next-line @typescript-eslint/no-explicit-any
	(globalThis as any).Highlight = class {
		ranges: unknown[];
		cleared = false;
		constructor(...ranges: unknown[]) {
			this.ranges = ranges;
		}
		clear() {
			this.cleared = true;
			this.ranges = [];
		}
	};
	document.body.innerHTML = '<main><p>hello world hello</p></main>';
});

afterEach(() => {
	// eslint-disable-next-line @typescript-eslint/no-explicit-any
	delete (globalThis as any).CSS;
	// eslint-disable-next-line @typescript-eslint/no-explicit-any
	delete (globalThis as any).Highlight;
	vi.clearAllMocks();
});

/** 대역을 세운 뒤에 import 해야 모듈 평가 시점 판정이 대역을 본다. */
async function mount(onclose = () => {}) {
	const FindBar = (await import('./FindBar.svelte')).default;
	const root = document.querySelector('main') as HTMLElement;
	return render(FindBar, { props: { root, onclose } });
}

async function search(text: string) {
	const input = screen.getByRole('textbox');
	await fireEvent.input(input, { target: { value: text } });
}

describe('FindBar — 강조 정리', () => {
	it('칠하기 자체가 동작한다 — 이게 false 면 아래 테스트가 의미 없다', async () => {
		await mount();
		await search('hello');
		expect(registry.calls.some((c) => c === 'set:og-find')).toBe(true);
	});

	it('✕ 로 닫으면 두 강조가 모두 지워진다', async () => {
		await mount();
		await search('hello');
		registry.calls.length = 0;
		await fireEvent.click(screen.getByRole('button', { name: /닫기|close/i }));
		expect(registry.calls).toContain('delete:og-find');
		expect(registry.calls).toContain('delete:og-find-current');
		expect(registry.map.has('og-find')).toBe(false);
		expect(registry.map.has('og-find-current')).toBe(false);
	});

	it('Escape 로 닫아도 지워진다', async () => {
		await mount();
		await search('hello');
		registry.calls.length = 0;
		await fireEvent.keyDown(screen.getByRole('search'), { key: 'Escape' });
		expect(registry.map.has('og-find')).toBe(false);
		expect(registry.map.has('og-find-current')).toBe(false);
	});

	// BUG-268: 레지스트리에서 빼는 것만으로는 화면이 안 지워지는 환경이 있다
	// (WebKit 계열의 무효화 문제로 보인다). Highlight 자체를 비워야 그 자리가
	// 확실히 다시 그려진다. **비우기가 delete 보다 먼저** 나가야 한다 —
	// 뺀 뒤에 비우면 그리기와 무관해진다.
	it('지울 때 Highlight 를 비우고, 비우기가 delete 보다 먼저다', async () => {
		await mount();
		await search('hello');
		const all = registry.get('og-find') as { cleared: boolean };
		const cur = registry.get('og-find-current') as { cleared: boolean };
		expect(all).toBeTruthy();
		expect(cur).toBeTruthy();
		registry.calls.length = 0;
		await fireEvent.click(screen.getByRole('button', { name: /닫기|close/i }));
		expect(all.cleared).toBe(true);
		expect(cur.cleared).toBe(true);
	});

	it('컴포넌트가 사라져도 지워진다 — 페이지 이동 경로', async () => {
		const { unmount } = await mount();
		await search('hello');
		unmount();
		expect(registry.map.has('og-find')).toBe(false);
		expect(registry.map.has('og-find-current')).toBe(false);
	});
});
