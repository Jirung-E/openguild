// DEV-443: 형광펜 자리 잡기 — 고른 범위 ↔ 인용, 글이 바뀌어도 같은 글을 다시 찾기.
import { describe, it, expect, afterEach } from 'vitest';
import { collectText, rangeOffsets, quoteAt, findQuote, rangeAt, offsetOf } from './text-quote';

afterEach(() => {
	document.body.innerHTML = '';
});

function root(html: string): HTMLElement {
	const d = document.createElement('div');
	d.innerHTML = html;
	document.body.appendChild(d);
	return d;
}

describe('DEV-443 text-quote', () => {
	it('문단 · 굵게 · 목록을 넘는 범위를 글자 위치로, 다시 같은 범위로', () => {
		const r = root('<p>앞 글 <b>굵은 말</b> 뒤</p><ul><li>하나</li><li>둘</li></ul>');
		const map = collectText(r);
		expect(map.text).toBe('앞 글 굵은 말 뒤하나둘');
		const b = r.querySelector('b')!.firstChild!;
		const li2 = r.querySelectorAll('li')[1].firstChild!;
		const range = document.createRange();
		range.setStart(b, 3); // "말" 앞
		range.setEnd(li2, 1); // "둘" 뒤
		const [s, e] = rangeOffsets(map, range)!;
		expect(map.text.slice(s, e)).toBe('말 뒤하나둘');
		const back = rangeAt(map, s, e)!;
		expect(back.toString()).toBe('말 뒤하나둘');
	});

	it('요소 경계(문단 사이)도 글자 위치로', () => {
		const r = root('<p>가나</p><p>다라</p>');
		const map = collectText(r);
		expect(offsetOf(map, r, 1)).toBe(2); // 두 번째 문단 앞
		expect(offsetOf(map, r, 2)).toBe(4); // 끝
	});

	it('버튼 · 그림(svg) 안의 글은 모으지 않는다', () => {
		const r = root('<p>본문<button>복사</button></p><svg><text>도형</text></svg><p>끝</p>');
		expect(collectText(r).text).toBe('본문끝');
	});

	it('같은 글이 여러 번이면 앞뒤 글자로 가린다', () => {
		const text = '사과 하나. 배 하나. 사과 둘.';
		const at = text.lastIndexOf('사과');
		const q = quoteAt(text, at, at + 2);
		expect(findQuote(text, q)).toEqual([at, at + 2]);
	});

	it('본문이 앞에서 바뀌어도 따라간다', () => {
		const text = '머리말. 중요한 문장이 여기. 끝.';
		const s = text.indexOf('중요한');
		const q = quoteAt(text, s, s + 6);
		const edited = '새로 쓴 긴 머리말이 붙었다. 중요한 문장이 여기. 끝.';
		const [a, b] = findQuote(edited, q)!;
		expect(edited.slice(a, b)).toBe('중요한 문장');
	});

	it('칠한 글 자체가 바뀌면 못 찾는다 — 어림으로 칠하지 않는다', () => {
		const text = '이 문장을 칠했다.';
		const q = quoteAt(text, 2, 5);
		expect(findQuote('이 글귀를 칠했다.', q)).toBeNull();
	});
});
