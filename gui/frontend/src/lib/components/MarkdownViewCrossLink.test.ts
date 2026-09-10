// BUG-282: **없는 것은 링크가 아니다.**
//
// `[[BOOK-999]]` 처럼 존재하지 않는 문서를 가리키는 크로스링크는 빨갛게 칠하고
// "존재하지 않는 …" 툴팁까지 띄우면서, 정작 **누르면 그대로 이동했다** — 지워진
// 도서관 문서를 누르면 `/library?id=…` 로 튕겨 나갔다. 색만 보고 "이건 처리됐다"
// 고 판단하기 쉬운 종류라, 두 축을 따로 단언한다:
//   ① 없는 것은 href 가 없다   ② 있는 것은 여전히 href 가 있다
//
// 셋째 시험은 [[BUG-173]] 회귀 방지다. 인덱스는 세션당 1회 적재라, GUI 를 켜둔
// 채 CLI 가 만든 문서를 가리키는 링크는 실재하는데도 빨강이었다. 나중에 인덱스가
// 채워지면 다시 링크가 되어야 하는데, href 를 끊는 이번 수정이 그 경로를 막으면
// 안 된다.

import { describe, it, expect, beforeEach, vi } from 'vitest';
import { render } from '@testing-library/svelte';
import { tick } from 'svelte';
import MarkdownView from './MarkdownView.svelte';
import { questIndex, questIndexNs, type IndexedRef } from '$lib/stores/questIndex';

// 인덱스 적재는 네트워크를 탄다 — 시험은 스토어를 직접 채운다.
vi.mock('$lib/api/client', () => ({
	api: { get: vi.fn(async () => []) }
}));

function seed(entries: Array<[string, IndexedRef]>) {
	questIndex.set(new Map(entries));
	questIndexNs.set(new Map(entries.map(([k, v]) => [`${v.kind}:${k}`, v])));
}

const BOOK: IndexedRef = { kind: 'book', title: '있는 문서' } as IndexedRef;

async function renderBody(source: string) {
	const { container } = render(MarkdownView, { source });
	await tick();
	await tick();
	return container;
}

describe('BUG-282 없는 크로스링크는 누를 수 없다', () => {
	beforeEach(() => {
		seed([['BOOK-001', BOOK]]);
	});

	it('없는 문서를 가리키면 href 가 없다', async () => {
		const c = await renderBody('본문 [[BOOK-999]] 끝');
		const a = c.querySelector('a.xlink');
		expect(a).not.toBeNull();
		expect(a!.classList.contains('missing')).toBe(true);
		expect(a!.hasAttribute('href')).toBe(false);
	});

	it('있는 문서는 여전히 href 를 갖는다', async () => {
		const c = await renderBody('본문 [[BOOK-001]] 끝');
		const a = c.querySelector('a.xlink');
		expect(a).not.toBeNull();
		expect(a!.classList.contains('missing')).toBe(false);
		expect(a!.getAttribute('href')).toBe('/library?id=BOOK-001');
	});

	// BUG-173 회귀 방지 — 늦게 적재된 인덱스가 링크를 되살려야 한다.
	it('인덱스가 나중에 채워지면 다시 링크가 된다', async () => {
		const c = await renderBody('본문 [[BOOK-777]] 끝');
		expect(c.querySelector('a.xlink')!.hasAttribute('href')).toBe(false);

		seed([
			['BOOK-001', BOOK],
			['BOOK-777', { kind: 'book', title: '늦게 온 문서' } as IndexedRef]
		]);
		await tick();
		await tick();

		const a = c.querySelector('a.xlink')!;
		expect(a.classList.contains('missing')).toBe(false);
		expect(a.getAttribute('href')).toBe('/library?id=BOOK-777');
	});
});
