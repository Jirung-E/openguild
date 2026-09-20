// DEV-419: 탐색기처럼 고르기. 규칙이 어긋나면 "3개 옮기기" 가 엉뚱한 셋을 옮긴다.

import { describe, it, expect } from 'vitest';
import { click, clear, all, prune, EMPTY, type Picked } from './multi-select';

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
