// DEV-392: 태그 **정의 편집**이 태그 목록 페이지에 있다.
//
// 예전엔 사용 현황은 여기, 정의(색·설명) 편집은 관리 페이지에 따로 있어서 여기서 본 색을
// 바꾸려면 관리 페이지로 가야 했다. 합친 뒤에 지켜야 할 것을 실제 렌더로 본다 — 정의된
// 태그는 편집·삭제, 쓰이기만 하는 태그는 정의 만들기, 코어가 못 받는 이름에는 버튼 없음.

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/svelte';

const listTagDefs = vi.fn();
const upsertTagDef = vi.fn();
const deleteTagDef = vi.fn();
vi.mock('$lib/api/admin', () => ({
	adminApi: {
		listTagDefs: () => listTagDefs(),
		upsertTagDef: (b: unknown) => upsertTagDef(b),
		deleteTagDef: (s: string) => deleteTagDef(s)
	}
}));
vi.mock('$lib/api/quests', () => ({
	questsApi: {
		list: () =>
			Promise.resolve([
				{ quest_id: 'DEV-001', title: 'a', tags: ['frontend', 'used-only', '한글태그'] }
			])
	}
}));
vi.mock('$lib/api/rules', () => ({ rulesApi: { list: () => Promise.resolve({ entries: [] }) } }));
vi.mock('$lib/api/library', () => ({ libraryApi: { list: () => Promise.resolve([]) } }));

function rowOf(tag: string): HTMLElement {
	const chip = screen.getByText(`#${tag}`);
	return chip.closest('li.tag-item') as HTMLElement;
}

async function open() {
	const { default: Page } = await import('./+page.svelte');
	render(Page);
	await screen.findByText('#frontend');
}

describe('DEV-392 태그 목록에서 정의를 편집한다', () => {
	beforeEach(() => {
		listTagDefs.mockReset();
		upsertTagDef.mockReset();
		deleteTagDef.mockReset();
		listTagDefs.mockResolvedValue([
			{ slug: 'frontend', color: '#ff0000', description: '화면 쪽' }
		]);
		upsertTagDef.mockResolvedValue({});
	});

	it('정의된 태그는 설명이 보이고 편집·삭제가 있다', async () => {
		await open();
		const row = rowOf('frontend');
		expect(row.textContent).toContain('화면 쪽');
		expect(row.querySelectorAll('.row-actions button').length).toBe(2);
	});

	it('쓰이기만 하는 태그는 그 이름으로 정의를 만든다', async () => {
		await open();
		const row = rowOf('used-only');
		const btns = row.querySelectorAll('.row-actions button');
		expect(btns.length).toBe(1);
		await fireEvent.click(btns[0]);
		const save = row.querySelector('.editor .btn.save') as HTMLButtonElement;
		// 이름은 이미 정해져 있다 — 입력란을 또 받지 않는다.
		expect(row.querySelector('.editor .slug')).toBeNull();
		await fireEvent.click(save);
		expect(upsertTagDef).toHaveBeenCalledWith(expect.objectContaining({ slug: 'used-only' }));
	});

	it('코어가 못 받는 이름(한글 등)에는 정의 버튼을 안 그린다', async () => {
		await open();
		expect(rowOf('한글태그').querySelector('.row-actions button')).toBeNull();
	});

	it('새 정의는 `-` 가 든 이름도 받는다 — 코어 규칙과 같다', async () => {
		await open();
		await fireEvent.click(screen.getByText('+ 새 태그 정의'));
		const slug = document.querySelector('.tag-item.new .slug') as HTMLInputElement;
		await fireEvent.input(slug, { target: { value: 'needs-review' } });
		await fireEvent.click(document.querySelector('.tag-item.new .btn.save') as HTMLButtonElement);
		expect(upsertTagDef).toHaveBeenCalledWith(expect.objectContaining({ slug: 'needs-review' }));
	});
});
