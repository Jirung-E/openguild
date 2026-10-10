// DEV-419: 탐색기처럼 고르기. 규칙이 어긋나면 "3개 옮기기" 가 엉뚱한 셋을 옮긴다.

import { describe, it, expect } from 'vitest';
import { click, clear, all, prune, arrow, hitsIn, marquee, EMPTY, type Picked } from './multi-select';

const order = ['a', 'b', 'c', 'd', 'e'];
const ids = (p: Picked) => [...p.ids].sort();

describe('DEV-419 여러 개 고르기', () => {
	it('그냥 누르면 그것만', () => {
		const s = click(EMPTY, 'c', {}, order);
		expect(ids(s)).toEqual(['c']);
		expect(s.anchor).toBe('c');
	});

	it('⌘/Ctrl 은 토글 — 눌렀던 것을 다시 누르면 빠진다', () => {
		let s = click(EMPTY, 'a', {}, order);
		s = click(s, 'c', { toggle: true }, order);
		expect(ids(s)).toEqual(['a', 'c']);
		s = click(s, 'a', { toggle: true }, order);
		expect(ids(s)).toEqual(['c']);
	});

	it('Shift 는 기준점부터 범위 — 화면 순서를 따른다', () => {
		let s = click(EMPTY, 'b', {}, order);
		s = click(s, 'd', { range: true }, order);
		expect(ids(s)).toEqual(['b', 'c', 'd']);
	});

	it('Shift 로 늘렸다 줄일 수 있다 — 기준점은 안 움직인다', () => {
		let s = click(EMPTY, 'b', {}, order);
		s = click(s, 'e', { range: true }, order);
		expect(ids(s)).toEqual(['b', 'c', 'd', 'e']);
		s = click(s, 'c', { range: true }, order);
		expect(ids(s)).toEqual(['b', 'c']);
		expect(s.anchor).toBe('b');
	});

	it('거꾸로 고른 범위도 같다', () => {
		let s = click(EMPTY, 'd', {}, order);
		s = click(s, 'b', { range: true }, order);
		expect(ids(s)).toEqual(['b', 'c', 'd']);
	});

	it('기준점이 없으면 Shift 는 그냥 누름', () => {
		const s = click(EMPTY, 'c', { range: true }, order);
		expect(ids(s)).toEqual(['c']);
	});

	it('빈 곳을 누르면 풀린다 / 전체 선택', () => {
		expect(ids(clear())).toEqual([]);
		expect(ids(all(order))).toEqual([...order].sort());
	});

	it('목록에서 사라진 것은 떨군다 — 없는 것을 고른 채로 두지 않는다', () => {
		let s = click(EMPTY, 'b', {}, order);
		s = click(s, 'd', { range: true }, order);
		const after = prune(s, ['b', 'e']); // c, d 가 지워졌다
		expect(ids(after)).toEqual(['b']);
		// 기준점이 사라졌으면 비운다 — 다음 Shift 가 엉뚱한 범위를 잡지 않게.
		expect(prune(s, ['c']).anchor).toBeNull();
	});
});

describe('DEV-422 방향키 · 사각형 선택', () => {
	// 한 줄에 3칸: a b c / d e
	const sel = (id: string): Picked => ({ ids: new Set([id]), anchor: id, lead: id });

	it('아무것도 안 골랐으면 첫 칸', () => {
		expect(ids(arrow(EMPTY, order, 3, 'right'))).toEqual(['a']);
	});

	it('좌우는 한 칸, 위아래는 한 줄', () => {
		expect(ids(arrow(sel('b'), order, 3, 'right'))).toEqual(['c']);
		expect(ids(arrow(sel('b'), order, 3, 'down'))).toEqual(['e']);
		expect(ids(arrow(sel('e'), order, 3, 'up'))).toEqual(['b']);
	});

	it('끝에서는 머문다 — 아래 줄에 칸이 없으면 안 움직인다', () => {
		expect(ids(arrow(sel('c'), order, 3, 'down'))).toEqual(['c']);
		expect(ids(arrow(sel('a'), order, 3, 'left'))).toEqual(['a']);
		expect(ids(arrow(sel('e'), order, 3, 'right'))).toEqual(['e']);
	});

	it('Shift+방향키는 기준점을 두고 늘렸다 줄인다', () => {
		let s = arrow(sel('b'), order, 3, 'right', true);
		s = arrow(s, order, 3, 'right', true);
		expect(ids(s)).toEqual(['b', 'c', 'd']);
		s = arrow(s, order, 3, 'left', true);
		expect(ids(s)).toEqual(['b', 'c']);
		expect(s.anchor).toBe('b');
	});

	it('누른 뒤 방향키는 누른 자리에서 출발한다 (Shift 범위 끝 포함)', () => {
		let s = click(EMPTY, 'a', {}, order);
		s = click(s, 'c', { range: true }, order);
		expect(ids(arrow(s, order, 3, 'right'))).toEqual(['d']);
	});

	it('사각형에 걸친 것만 맞는다', () => {
		const items = [
			{ id: 'a', left: 0, top: 0, right: 10, bottom: 10 },
			{ id: 'b', left: 20, top: 0, right: 30, bottom: 10 },
			{ id: 'c', left: 0, top: 20, right: 10, bottom: 30 }
		];
		expect(hitsIn(items, { left: 5, top: 5, right: 25, bottom: 8 })).toEqual(['a', 'b']);
		expect(hitsIn(items, { left: 11, top: 11, right: 19, bottom: 19 })).toEqual([]);
	});

	it('수식키에 따라 바꾸기 · 더하기 · 뒤집기', () => {
		const base: Picked = { ids: new Set(['a', 'b']), anchor: 'a', lead: 'b' };
		expect(ids(marquee(base, ['b', 'c'], 'replace'))).toEqual(['b', 'c']);
		expect(ids(marquee(base, ['b', 'c'], 'add'))).toEqual(['a', 'b', 'c']);
		expect(ids(marquee(base, ['b', 'c'], 'toggle'))).toEqual(['a', 'c']);
		// 아무것도 안 지나가면 바꾸기는 비우고, 더하기는 그대로.
		expect(ids(marquee(base, [], 'replace'))).toEqual([]);
		expect(ids(marquee(base, [], 'add'))).toEqual(['a', 'b']);
	});
});
