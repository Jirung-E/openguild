// DEV-393: 관리 페이지는 탭으로 나뉘고, 플러그인은 설정 페이지가 아니라 여기 있다.
//
// 기준 — 설정은 **앱 전체**(어떤 길드를 열든 같다), 관리는 **지금 연 길드**. 플러그인 목록·
// 동의·설정값은 전부 길드별이라 관리 쪽이다. 탭마다 제 섹션만 보이는지, 설정 페이지에서
// 플러그인이 빠졌는지를 실제 렌더로 본다.

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/svelte';

// BUG-310: 어느 탭인지는 이제 **주소**가 안다. 그래서 여기서도 주소를 진짜처럼 흉내낸다 —
// `goto` 가 주소를 바꾸고, 화면이 그 주소를 보고 다시 그린다. 안 그러면 탭을 눌러도
// 아무 일도 안 일어난다.
const nav = vi.hoisted(() => {
	const subs = new Set<(v: unknown) => void>();
	let url = new URL('http://x/admin');
	const emit = () => subs.forEach((f) => f({ url }));
	return {
		page: {
			subscribe: (f: (v: unknown) => void) => {
				subs.add(f);
				f({ url });
				return () => subs.delete(f);
			}
		},
		goto: (href: string) => {
			url = new URL(href, 'http://x');
			emit();
			return Promise.resolve();
		},
		reset: (path: string) => {
			url = new URL(path, 'http://x');
			emit();
		},
		here: () => url.pathname + url.search
	};
});
vi.mock('$app/stores', () => ({ page: nav.page }));
vi.mock('$app/navigation', () => ({ goto: nav.goto }));

vi.mock('$lib/api/admin', () => ({
	adminApi: {
		listSnapshots: () => Promise.resolve([]),
		listTypes: () => Promise.resolve([]),
		listStatuses: () => Promise.resolve([])
	}
}));
const status = vi.fn();
vi.mock('$lib/api/plugins', () => ({
	pluginApi: { status: () => status() },
	pluginsManageable: () => false
}));
vi.mock('$lib/api/updater', () => ({
	updateState: { subscribe: (f: (v: unknown) => void) => (f({ kind: 'idle' }), () => {}) },
	checkForUpdate: vi.fn()
}));

function tabs(): HTMLButtonElement[] {
	return Array.from(document.querySelectorAll<HTMLButtonElement>('.side nav button.tab'));
}

describe('DEV-393 관리 페이지 탭', () => {
	beforeEach(() => {
		nav.reset('/admin');
		status.mockReset();
		status.mockResolvedValue({
			plugins: [],
			errors: [],
			auto_allow: false,
			manageable: false,
			no_guild: false,
			problems: []
		});
	});

	it('탭 넷 — 퀘스트 구성 · 플러그인 · 백업 · 진단, 첫 탭은 구성만', async () => {
		const { default: Page } = await import('./+page.svelte');
		render(Page);
		expect(tabs().map((b) => b.textContent?.trim())).toEqual([
			'퀘스트 구성',
			'플러그인',
			'백업',
			'진단'
		]);
		// 구성 탭에는 백업·진단 섹션이 없다.
		expect(document.body.textContent).not.toContain('Reindex');
		expect(status).not.toHaveBeenCalled();
	});

	it('플러그인 탭을 누르면 그때 플러그인 목록을 읽어 그린다', async () => {
		const { default: Page } = await import('./+page.svelte');
		render(Page);
		await fireEvent.click(tabs()[1]);
		expect(status).toHaveBeenCalled();
		expect(await screen.findByText('플러그인', { selector: 'h2' })).toBeTruthy();
	});

	it('진단 탭에 정합성 점검과 정비가 함께 있다', async () => {
		const { default: Page } = await import('./+page.svelte');
		render(Page);
		await fireEvent.click(tabs()[3]);
		expect(document.body.textContent).toContain('Reindex');
		expect(document.querySelectorAll('.panel section').length).toBe(2);
	});

	// BUG-310: 탭이 화면 안 변수로만 있어서, 다른 페이지에 갔다 오면 늘 첫 탭이었고 탭을
	// 옮겨도 뒤로 가기가 페이지를 아예 벗어났다. 주소에 적으면 둘 다 풀린다.
	it('탭을 누르면 주소에 남는다 — 돌아와도 그 탭', async () => {
		const { default: Page } = await import('./+page.svelte');
		render(Page);
		await fireEvent.click(tabs()[1]);
		expect(nav.here()).toBe('/admin?tab=plugins');
	});

	it('주소에 적힌 탭으로 열린다', async () => {
		nav.reset('/admin?tab=backup');
		const { default: Page } = await import('./+page.svelte');
		render(Page);
		expect(tabs()[2].getAttribute('aria-pressed')).toBe('true');
	});

	// 첫 탭은 안 적는다 — `/admin` 과 `/admin?tab=structure` 가 기록에 둘로 쌓이면 뒤로
	// 가기가 한 번 헛돈다.
	it('첫 탭으로 돌아오면 주소가 깨끗해진다', async () => {
		const { default: Page } = await import('./+page.svelte');
		render(Page);
		await fireEvent.click(tabs()[2]);
		await fireEvent.click(tabs()[0]);
		expect(nav.here()).toBe('/admin');
	});

	it('설정 페이지에는 플러그인 탭이 없다', async () => {
		nav.reset('/settings');
		const { default: Settings } = await import('../settings/+page.svelte');
		render(Settings);
		const labels = Array.from(document.querySelectorAll('button[aria-pressed]')).map((b) =>
			b.textContent?.trim()
		);
		expect(labels).not.toContain('플러그인');
		expect(status).not.toHaveBeenCalled();
	});
});
