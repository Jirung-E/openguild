// REQ-020: 설명이 **화면에 실제로 그려지는가**.
//
// API 통과 시험(`lib/api/plugins.test.ts`)은 값이 오는 것까지만 본다. 그 값이
// 마크업에 닿는지는 다른 문제고, 이 저장소는 그 간극에서 두 번 데였다 —
// `tags` 가 이름은 맞는데 항상 비어 나갔고(DEV-381), `.scale-hint` 는 정의가
// 있는데 그 요소에는 안 맞는 selector 였다(DEV-383). 둘 다 "있다" 는 검사는
// 통과했다.
//
// 그래서 여기서는 플러그인 화면을 실제로 렌더하고, 설명이 있는 것과 없는 것을
// 나란히 둔다. 없는 쪽에 빈 자리가 생기면 그것도 결함이다.

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen } from '@testing-library/svelte';
import { fireEvent, waitFor } from '@testing-library/svelte';
import type { PluginSource, PluginStatus, PluginView } from '$lib/api/plugins';

const status = vi.fn();
// DEV-400 / DEV-394
const sources = vi.fn(async () => [] as PluginSource[]);
const addFolder = vi.fn();
const usePlugin = vi.fn<(s: string, f: string) => Promise<void>>(async () => {});
const stopUsing = vi.fn<(s: string, f: string) => Promise<void>>(async () => {});
const reload = vi.fn();
const pickFolder = vi.fn();
vi.mock('@tauri-apps/plugin-dialog', () => ({ open: (o: unknown) => pickFolder(o) }));
// 허용/철회 버튼은 `pluginsManageable() && status.manageable` 일 때만 그려진다
// — 시험마다 갈아 끼울 수 있어야 한다.
const manageable = vi.fn(() => false);
vi.mock('$lib/api/plugins', () => ({
	pluginApi: {
		status: () => status(),
		allow: vi.fn(),
		revoke: vi.fn(),
		allowAll: vi.fn(),
		revokeAll: vi.fn(),
		setAutoAllow: vi.fn(),
		sources: () => sources(),
		addFolder: (p: string) => addFolder(p),
		use: (s: string, f: string) => usePlugin(s, f),
		stopUsing: (s: string, f: string) => stopUsing(s, f),
		removeSource: vi.fn(),
		reload: () => reload()
	},
	pluginsManageable: () => manageable()
}));

function plugin(name: string, description: string | null): PluginView {
	return {
		name,
		description,
		on: ['quest.created'],
		scope: ['gui'],
		handlers: [
			{
				label: '1번째 줄',
				stage: 'post',
				events: ['quest.created'],
				call: null,
				with: [],
				action: null,
				action_kind: 'post',
				action_target: 'https://example.test/hook'
			}
		],
		actions: [],
		scripts: [],
		data_dir: null,
		inputs: [],
		script_src: null,
		granted: true,
		runs_here: true
	};
}

function statusWith(plugins: PluginView[]): PluginStatus {
	return { plugins, errors: [], auto_allow: false, manageable: false, no_guild: false, problems: [] };
}

async function openPluginsTab() {
	// DEV-393: 설정 페이지의 탭이던 것을 관리 페이지로 옮기며 컴포넌트로 뗐다 — 컴포넌트를 바로 그린다.
	const { default: Section } = await import('./AdminPluginsSection.svelte');
	render(Section);
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

// DEV-403: 줄 목록과 내보내는 곳이 **화면에 실제로** 나온다 — 허용할 때 무엇에 동의하는지다.
describe('DEV-403 줄 목록', () => {
	beforeEach(() => {
		status.mockReset();
		manageable.mockReturnValue(false);
	});

	it('줄마다 언제 → 무엇이, 이름 붙인 동작은 대상과 함께 보인다', async () => {
		const p: PluginView = {
			...plugin('tg', null),
			handlers: [
				{
					label: '상태 알림',
					stage: 'post',
					events: ['quest.status_changed'],
					call: 'on_status',
					with: ['subject'],
					action: null,
					action_kind: null,
					action_target: null
				},
				{
					label: '삭제 기록',
					stage: 'pre',
					events: ['quest.deleted'],
					call: null,
					with: [],
					action: 'log',
					action_kind: null,
					action_target: null
				}
			],
			actions: [{ name: 'telegram', kind: 'post', target: 'https://api.telegram.org/x' }]
		};
		status.mockResolvedValue(statusWith([p]));
		await openPluginsTab();
		await screen.findByText('tg');
		const lines = Array.from(document.querySelectorAll('.plugin-lines li')).map((li) =>
			li.textContent?.replace(/\s+/g, ' ').trim()
		);
		expect(lines).toEqual([
			'바뀐 뒤 quest.status_changed → on_status() 읽음: subject 상태 알림',
			'바뀌기 전 quest.deleted → log 삭제 기록'
		]);
		expect(document.querySelector('.plugin-dests')?.textContent).toContain(
			'https://api.telegram.org/x'
		);
	});

	it('줄에 바로 적은 동작은 종류와 대상이 보인다', async () => {
		status.mockResolvedValue(statusWith([plugin('plain', null)]));
		await openPluginsTab();
		await screen.findByText('plain');
		expect(document.querySelector('.plugin-lines li')?.textContent).toContain(
			'post https://example.test/hook'
		);
		expect(document.querySelector('.plugin-dests')).toBeNull();
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
		return { plugins, errors: [], auto_allow: false, manageable: true, no_guild: false, problems: [] };
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
			scripts: ['main.rhai'],
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

// BUG-288: 일괄 조작은 **한 자리에**, 개별 조작은 **언제든.**
//
// 예전엔 "전부 허용" 이 목록 맨 아래, 해제가 위 안내문에 흩어져 있었고, 둘 다 모드를
// 켜고 끄는 것이라 켜 둔 동안 개별 철회를 막았다(BUG-285 는 막힌 버튼을 글로 바꿨을
// 뿐이다). admin: "버튼은 그냥 '전체 해제 / 전체 허용'의 역할만 하고 개별 조작도 그대로
// 먹히는게 자연스러운 동작 아니냐?"
describe('BUG-288 전체 허용·전체 해제·자동 허용', () => {
	beforeEach(() => {
		status.mockReset();
		manageable.mockReturnValue(true);
	});

	function withAuto(on: boolean, plugins: PluginView[]): PluginStatus {
		return { plugins, errors: [], auto_allow: on, manageable: true, no_guild: false, problems: [] };
	}

	it('세 조작이 목록 위 한 줄에 있다', async () => {
		status.mockResolvedValue(withAuto(false, [{ ...plugin('tg', null), granted: true }]));
		await openPluginsTab();
		await screen.findByText('tg');

		const bar = document.querySelector('.plugin-bulk')!;
		expect(bar).not.toBeNull();
		expect(bar.querySelectorAll('button').length).toBe(2);
		expect(bar.querySelector('input[type="checkbox"]')).not.toBeNull();
		// 목록보다 앞에 있다.
		const list = document.querySelector('.plugin-list')!;
		expect(bar.compareDocumentPosition(list) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
		// 옛 자리의 버튼·글이 남지 않는다.
		expect(document.querySelector('.plugin-trusted, .plugin-trusted-note, .btn-plain.trust')).toBeNull();
	});

	it('자동 허용 중에도 개별 철회 버튼이 눌린다', async () => {
		status.mockResolvedValue(withAuto(true, [{ ...plugin('tg', null), granted: true }]));
		await openPluginsTab();
		await screen.findByText('tg');

		const btn = document.querySelector('.plugin-actions button') as HTMLButtonElement;
		expect(btn).not.toBeNull();
		expect(btn.disabled).toBe(false);
		const auto = document.querySelector('.plugin-bulk input[type="checkbox"]') as HTMLInputElement;
		expect(auto.checked).toBe(true);
	});
});

// DEV-400: 길드 밖 플러그인 — 더하기, 출처, 빼기, 소스 목록. DEV-394: 다시 읽기.
describe('DEV-400 플러그인 추가·소스', () => {
	function st(plugins: PluginView[], manage = true): PluginStatus {
		return { plugins, errors: [], auto_allow: false, manageable: manage, no_guild: false, problems: [] };
	}
	const fromSource = (name: string, granted = false): PluginView => ({
		...plugin(name, null),
		granted,
		source: 'mine',
		dir: `/Users/me/mine/${name}`,
		scripts: ['main.rhai'],
		script_src: `// ${name} script`
	});
	const srcList = (used: boolean): PluginSource[] => [
		{
			name: 'mine',
			path: '/Users/me/mine',
			problem: null,
			plugins: [
				{ name: 'echo', folder: 'echo-dir', dir: '/Users/me/mine/echo-dir', used },
				{ name: 'bee', folder: 'bee-dir', dir: '/Users/me/mine/bee-dir', used: false }
			]
		},
		{ name: 'gone', path: '/Volumes/usb/plugins', problem: '폴더를 읽지 못했습니다', plugins: [] }
	];

	beforeEach(() => {
		status.mockReset();
		sources.mockReset().mockResolvedValue([]);
		addFolder.mockReset();
		usePlugin.mockClear();
		stopUsing.mockClear();
		reload.mockReset();
		pickFolder.mockReset();
		manageable.mockReturnValue(true);
	});

	it('출처가 보이고, 소스에서 온 것에만 빼기 버튼이 있다 — 짝(소스, 폴더)으로 뺀다', async () => {
		status.mockResolvedValue(st([plugin('in-guild', null), fromSource('echo', true)]));
		sources.mockResolvedValue(srcList(true));
		await openPluginsTab();
		await screen.findByText('in-guild');

		const item = (n: string) =>
			Array.from(document.querySelectorAll('li.plugin')).find((li) =>
				li.querySelector('strong')?.textContent === n
			)!;
		expect(item('in-guild').textContent).toContain('출처: 이 길드');
		expect(item('echo').textContent).toContain('출처: 소스 mine');
		expect(item('echo').textContent).toContain('/Users/me/mine/echo');

		const stopLabel = (li: Element) =>
			Array.from(li.querySelectorAll('.plugin-actions button')).find((b) =>
				b.getAttribute('aria-label')?.startsWith('이 길드에서 빼기')
			);
		expect(stopLabel(item('in-guild'))).toBeUndefined();
		await waitFor(() => expect(stopLabel(item('echo'))).toBeTruthy());
		await fireEvent.click(stopLabel(item('echo'))!);
		await waitFor(() => expect(stopUsing).toHaveBeenCalledWith('mine', 'echo-dir'));
	});

	it('소스 목록 — 쓰는 것/안 쓰는 것, 사라진 폴더의 이유', async () => {
		status.mockResolvedValue(st([fromSource('echo', true)]));
		sources.mockResolvedValue(srcList(true));
		await openPluginsTab();
		await screen.findByText('/Volumes/usb/plugins');

		expect(screen.getByText('폴더를 읽지 못했습니다')).toBeTruthy();
		const row = (n: string) =>
			Array.from(document.querySelectorAll('.source-plugins li')).find((li) =>
				li.textContent?.includes(n)
			)!;
		expect(row('echo').textContent).toContain('쓰는 중');
		const use = row('bee').querySelector('button')!;
		expect(use.textContent).toContain('이 길드에서 쓰기');
		await fireEvent.click(use);
		await waitFor(() => expect(usePlugin).toHaveBeenCalledWith('mine', 'bee-dir'));
	});

	it('폴더 하나짜리를 더하면 그 항목으로 데려가 스크립트를 펼쳐 둔다', async () => {
		status.mockResolvedValueOnce(st([])).mockResolvedValue(st([fromSource('echo')]));
		pickFolder.mockResolvedValue('/Users/me/mine/echo-dir');
		addFolder.mockResolvedValue({ kind: 'used', source: 'mine', folder: '.', name: 'echo' });
		await openPluginsTab();
		await fireEvent.click(await screen.findByText('+ 플러그인 추가'));

		await waitFor(() => expect(addFolder).toHaveBeenCalledWith('/Users/me/mine/echo-dir'));
		await waitFor(() => expect(document.querySelector('li.plugin.fresh')).not.toBeNull());
		// 무엇에 동의하는지 — 스크립트가 이미 펼쳐져 있고, 허용 버튼이 같은 자리에 있다.
		expect(screen.getByText('// echo script')).toBeTruthy();
		const fresh = document.querySelector('li.plugin.fresh')!;
		expect(fresh.querySelector('.btn-go')?.textContent).toContain('허용');
	});

	it('여럿 든 폴더는 등록만 한다 — 아무것도 펼치거나 켜지 않는다', async () => {
		status.mockResolvedValue(st([]));
		pickFolder.mockResolvedValue('/Users/me/mine');
		addFolder.mockResolvedValue({ kind: 'registered', source: 'mine', plugins: 2 });
		await openPluginsTab();
		await fireEvent.click(await screen.findByText('+ 플러그인 추가'));
		await waitFor(() => expect(addFolder).toHaveBeenCalled());
		expect(usePlugin).not.toHaveBeenCalled();
		expect(document.querySelector('li.plugin.fresh')).toBeNull();
	});

	it('폴더 고르기를 취소하면 아무 일도 없다', async () => {
		status.mockResolvedValue(st([]));
		pickFolder.mockResolvedValue(null);
		await openPluginsTab();
		await fireEvent.click(await screen.findByText('+ 플러그인 추가'));
		await waitFor(() => expect(pickFolder).toHaveBeenCalled());
		expect(addFolder).not.toHaveBeenCalled();
	});

	it('다시 읽기 — 받은 상태로 목록을 바꾼다', async () => {
		status.mockResolvedValue(st([{ ...plugin('tg', null), granted: true }]));
		reload.mockResolvedValue(st([{ ...plugin('tg', null), granted: false }]));
		await openPluginsTab();
		await screen.findByText('돌고 있음');
		await fireEvent.click(screen.getByText('다시 읽기'));
		await waitFor(() => expect(reload).toHaveBeenCalled());
		expect(await screen.findByText('동의 대기 — 안 돕니다')).toBeTruthy();
	});

	it('웹(관리 불가)에서는 추가·다시 읽기·소스 목록이 없고, 소스를 묻지도 않는다', async () => {
		manageable.mockReturnValue(false);
		status.mockResolvedValue(st([fromSource('echo', true)], false));
		await openPluginsTab();
		await screen.findByText('echo');
		expect(screen.queryByText('+ 플러그인 추가')).toBeNull();
		expect(screen.queryByText('다시 읽기')).toBeNull();
		expect(screen.queryByText('소스 (이 기계)')).toBeNull();
		expect(sources).not.toHaveBeenCalled();
		// 출처는 조회라 보인다.
		expect(document.body.textContent).toContain('출처: 소스 mine');
	});
});
