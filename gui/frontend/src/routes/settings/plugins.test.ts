// REQ-020: 설명이 **화면에 실제로 그려지는가**.
//
// API 통과 시험(`lib/api/plugins.test.ts`)은 값이 오는 것까지만 본다. 그 값이
// 마크업에 닿는지는 다른 문제고, 이 저장소는 그 간극에서 두 번 데였다 —
// `tags` 가 이름은 맞는데 항상 비어 나갔고(DEV-381), `.scale-hint` 는 정의가
// 있는데 그 요소에는 안 맞는 selector 였다(DEV-383). 둘 다 "있다" 는 검사는
// 통과했다.
//
// 그래서 여기서는 설정 페이지를 실제로 렌더하고, 설명이 있는 것과 없는 것을
// 나란히 둔다. 없는 쪽에 빈 자리가 생기면 그것도 결함이다.

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/svelte';
import type { PluginStatus, PluginView } from '$lib/api/plugins';

const status = vi.fn();
// 허용/철회 버튼은 `pluginsManageable() && status.manageable` 일 때만 그려진다
// — 시험마다 갈아 끼울 수 있어야 한다.
const manageable = vi.fn(() => false);
vi.mock('$lib/api/plugins', () => ({
	pluginApi: {
		status: () => status(),
		allow: vi.fn(),
		revoke: vi.fn(),
		setTrusted: vi.fn()
	},
	pluginsManageable: () => manageable()
}));

// 업데이트 확인은 Tauri 를 부른다 — 이 시험의 관심사가 아니다.
vi.mock('$lib/api/updater', () => ({
	updateState: { subscribe: (f: (v: unknown) => void) => (f({ kind: 'idle' }), () => {}) },
	checkForUpdate: vi.fn()
}));

function plugin(name: string, description: string | null): PluginView {
	return {
		name,
		description,
		on: ['quest.created'],
		scope: ['gui'],
		action: 'post',
		target: 'https://example.test/hook',
		script: null,
		data_dir: null,
		script_src: null,
		granted: true,
		runs_here: true
	};
}

function statusWith(plugins: PluginView[]): PluginStatus {
	return { plugins, errors: [], trusted: false, manageable: false, no_guild: false, problems: [] };
}

async function openPluginsTab() {
	const { default: Page } = await import('./+page.svelte');
	render(Page);
	// 탭 버튼은 `aria-pressed` 를 갖는다 — 문구는 언어에 따라 바뀌므로
	// 이름으로 찾지 않는다.
	const tabs = screen.getAllByRole('button').filter((b) => b.hasAttribute('aria-pressed'));
	await fireEvent.click(tabs[tabs.length - 1]);
}

describe('REQ-020 설정 화면의 플러그인 설명', () => {
	beforeEach(() => {
		status.mockReset();
		manageable.mockReturnValue(false);
	});

	it('설명이 있으면 그대로 보이고, 없으면 빈 자리를 안 만든다', async () => {
		status.mockResolvedValue(
			statusWith([
				plugin('telegram-quest-status', 'notify 태그가 붙은 퀘스트의 상태가 바뀌면 알립니다.'),
				plugin('plain-hook', null)
			])
		);
		await openPluginsTab();

		expect(
			await screen.findByText('notify 태그가 붙은 퀘스트의 상태가 바뀌면 알립니다.')
		).toBeTruthy();

		// 설명 없는 플러그인의 항목에는 설명 자리가 아예 없어야 한다.
		const items = document.querySelectorAll('li.plugin');
		expect(items.length).toBe(2);
		const plain = Array.from(items).find((li) => li.textContent?.includes('plain-hook'));
		expect(plain?.querySelector('.plugin-desc')).toBeNull();
	});

	it('설명은 번역을 타지 않는다 — 작성자가 쓴 문장 그대로다', async () => {
		// 플러그인 작성자가 쓴 데이터라 `t()` 를 태울 대상이 아니다. 어느
		// 언어로 보든 원문이 나와야 한다.
		const raw = 'Sends a Telegram message. Needs TELEGRAM_BOT_TOKEN.';
		status.mockResolvedValue(statusWith([plugin('tg', raw)]));
		await openPluginsTab();
		expect(await screen.findByText(raw)).toBeTruthy();
	});
});

// BUG-277: 버튼의 **보이는 문구**와 **들리는 이름**은 다른 것이다.
//
// DEV-383 이 "버튼 이름이 전부 같음" 을 지적했을 때 보이는 텍스트에 플러그인
// 이름을 붙여서 고쳤다. 그건 접근성을 화면의 비용으로 산 것이었다 — 이름은
// 같은 항목 세 줄 위에 이미 있다. 두 축을 따로 단언해야 한 축을 고치다 다른
// 축을 되돌리는 일이 안 생긴다.
describe('BUG-277 버튼 이름은 aria-label 로만 구분한다', () => {
	beforeEach(() => {
		status.mockReset();
		manageable.mockReturnValue(true);
	});

	function manageableStatus(plugins: PluginView[]): PluginStatus {
		return { plugins, errors: [], trusted: false, manageable: true, no_guild: false, problems: [] };
	}

	it('보이는 문구에는 이름이 없고, 접근 이름에는 있다', async () => {
		const granted = { ...plugin('telegram-quest-status', null), granted: true };
		const pending = { ...plugin('desktop-notify', null), granted: false };
		status.mockResolvedValue(manageableStatus([granted, pending]));
		await openPluginsTab();

		// 관리 가능한 상태여야 버튼이 그려진다.
		await screen.findByText('telegram-quest-status');
		const buttons = Array.from(document.querySelectorAll('.plugin-actions button'));
		expect(buttons.length).toBe(2);

		for (const b of buttons) {
			expect(b.textContent?.trim()).not.toContain('telegram-quest-status');
			expect(b.textContent?.trim()).not.toContain('desktop-notify');
		}

		// 들리는 이름은 서로 달라야 한다 — 그게 DEV-383 이 고치려던 것이다.
		const labels = buttons.map((b) => b.getAttribute('aria-label'));
		expect(labels.every((l) => !!l)).toBe(true);
		expect(new Set(labels).size).toBe(2);
		expect(labels.some((l) => l!.includes('telegram-quest-status'))).toBe(true);
		expect(labels.some((l) => l!.includes('desktop-notify'))).toBe(true);
	});

	it('스크립트 펼침 버튼도 마찬가지다', async () => {
		const withScript = {
			...plugin('has-script', null),
			script: 'transform.rhai',
			script_src: 'fn should_send(e) { true }'
		};
		status.mockResolvedValue(manageableStatus([withScript]));
		await openPluginsTab();
		await screen.findByText('has-script');

		const toggle = document.querySelector('button.link-btn[aria-expanded]');
		expect(toggle).not.toBeNull();
		expect(toggle!.textContent?.trim()).not.toContain('has-script');
		expect(toggle!.getAttribute('aria-label')).toContain('has-script');
	});
});
