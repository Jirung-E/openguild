// BUG-325(admin): 추가한 적 없는 예제가 새 길드에 보인다.
//
// 소스는 길드가 아니라 **기계**에 등록된다. 그래서 한 번 등록하면 그 기계의 모든 길드가
// 그 목록을 본다 — 이 길드에서 아무것도 안 쓰는데 펼쳐져 있으면 "내가 언제 추가했지"
// 가 된다. 숨기지는 않는다(그러면 지울 수도 없다) — **접는다**.

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render } from '@testing-library/svelte';
import { tick } from 'svelte';

const status = vi.fn();
const sources = vi.fn();
vi.mock('$lib/api/plugins', () => ({
	pluginApi: {
		status: () => status(),
		sources: () => sources()
	},
	pluginsManageable: () => true
}));
vi.mock('$lib/stores/toast', () => ({ showToast: vi.fn() }));

import AdminPluginsSection from './AdminPluginsSection.svelte';

function source(name: string, used: boolean[]) {
	return {
		name,
		path: `/src/${name}`,
		problem: null as string | null,
		plugins: used.map((u, i) => ({
			name: `${name}-${i}`,
			folder: `f${i}`,
			dir: `/src/${name}/f${i}`,
			used: u
		}))
	};
}

function heads() {
	return [...document.querySelectorAll('.src-head')] as HTMLElement[];
}
function openOf(i: number) {
	return heads()[i]?.getAttribute('aria-expanded');
}
function listShown(i: number) {
	return !!heads()[i]?.parentElement?.querySelector('.source-plugins');
}

describe('BUG-325 소스 접기', () => {
	beforeEach(() => {
		document.body.innerHTML = '';
		status.mockResolvedValue({
			plugins: [],
			errors: [],
			auto_allow: false,
			manageable: true,
			no_guild: false,
			problems: [],
			examples_url: 'https://example.test/examples'
		});
	});

	it('이 길드에서 쓰는 것이 없으면 접혀 있다', async () => {
		sources.mockResolvedValue([source('예제모음', [false, false, false])]);
		render(AdminPluginsSection, { props: {} });
		await tick();
		await tick();
		await new Promise((r) => setTimeout(r, 20));
		expect(openOf(0)).toBe('false');
		expect(listShown(0), '접혔는데 목록이 보인다').toBe(false);
		// 몇 개인지와 "안 쓴다" 는 접힌 채로도 보여야 한다 — 안 그러면 왜 접혔는지 모른다.
		expect(heads()[0].textContent).toContain('3');
	});

	it('하나라도 쓰고 있으면 펼쳐져 있다', async () => {
		sources.mockResolvedValue([source('쓰는모음', [false, true])]);
		render(AdminPluginsSection, { props: {} });
		await tick();
		await tick();
		await new Promise((r) => setTimeout(r, 20));
		expect(openOf(0)).toBe('true');
		expect(listShown(0)).toBe(true);
	});

	// 접는 것이지 숨기는 것이 아니다 — 눌러서 언제든 편다(안 그러면 소스를 지울 수도 없다).
	it('눌러서 편다', async () => {
		sources.mockResolvedValue([source('예제모음', [false])]);
		render(AdminPluginsSection, { props: {} });
		await tick();
		await new Promise((r) => setTimeout(r, 20));
		expect(openOf(0)).toBe('false');
		heads()[0].click();
		await tick();
		expect(openOf(0)).toBe('true');
		expect(listShown(0)).toBe(true);
	});

	// 깨진 소스는 접지 않는다 — 안 보이면 왜 안 도는지 영영 모른다.
	it('문제가 있는 소스는 펼쳐 둔다', async () => {
		const s = source('깨진모음', [false]);
		s.problem = '폴더가 없습니다';
		sources.mockResolvedValue([s]);
		render(AdminPluginsSection, { props: {} });
		await tick();
		await new Promise((r) => setTimeout(r, 20));
		expect(openOf(0)).toBe('true');
	});
});
