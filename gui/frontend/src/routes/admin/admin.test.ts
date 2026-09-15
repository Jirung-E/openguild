// DEV-393: 관리 페이지는 탭으로 나뉘고, 플러그인은 설정 페이지가 아니라 여기 있다.
//
// 기준 — 설정은 **앱 전체**(어떤 길드를 열든 같다), 관리는 **지금 연 길드**. 플러그인 목록·
// 동의·설정값은 전부 길드별이라 관리 쪽이다. 탭마다 제 섹션만 보이는지, 설정 페이지에서
// 플러그인이 빠졌는지를 실제 렌더로 본다.

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/svelte';

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

	it('설정 페이지에는 플러그인 탭이 없다', async () => {
		const { default: Settings } = await import('../settings/+page.svelte');
		render(Settings);
		const labels = Array.from(document.querySelectorAll('button[aria-pressed]')).map((b) =>
			b.textContent?.trim()
		);
		expect(labels).not.toContain('플러그인');
		expect(status).not.toHaveBeenCalled();
	});
});
