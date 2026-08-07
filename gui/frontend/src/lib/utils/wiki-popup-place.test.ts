import { describe, it, expect } from 'vitest';
import {
	computeWikiPlace,
	clampWikiLeft,
	isWikiCaretVisible,
	WIKI_MARGIN
} from './wiki-popup-place';

describe('computeWikiPlace', () => {
	// BUG-209: 아래 공간이 넉넉하면 caret 바로 아래로.
	it('아래 공간이 충분하면 top 배치(bottom=null)', () => {
		const place = computeWikiPlace(100, 120, 5, 0, 800);
		expect(place.bottom).toBeNull();
		expect(place.top).toBe(120);
	});

	// 아래 공간이 좁고 위가 더 넓으면 flip.
	it('아래 공간이 부족하고 위가 더 넓으면 bottom 배치로 flip', () => {
		// 뷰포트 800, caret 이 아래쪽(750~770)에 있어 아래 여유가 거의 없음.
		const place = computeWikiPlace(750, 770, 10, 0, 800);
		expect(place.top).toBeNull();
		expect(place.bottom).toBe(800 - 750);
	});

	// 위/아래 둘 다 좁아도 최소 68px(2항목분) 은 보장.
	it('공간이 아주 좁아도 maxH 하한(68) 은 유지', () => {
		const place = computeWikiPlace(50, 60, 20, 500, 100);
		expect(place.maxH).toBeGreaterThanOrEqual(68);
	});

	// WIKI_MAX_H(224) 이상으로는 안 커짐 — 공간이 남아도.
	it('공간이 넉넉해도 maxH 는 224 를 넘지 않음', () => {
		const place = computeWikiPlace(100, 120, 3, 0, 4000);
		expect(place.maxH).toBeLessThanOrEqual(224);
	});

	// measuredH(실측)가 있으면 itemCount 어림값 대신 그걸 사용.
	it('measuredH 가 있으면 그 값 기준으로 flip 여부 판단', () => {
		// 아래 공간 100px, 실측 높이 150px 이 아래보다 크고 위(300) 가 아래보다
		// 넓으면 flip.
		const place = computeWikiPlace(300, 700, 1, 150, 800);
		expect(place.top).toBeNull(); // flip = bottom 배치
	});
});

describe('clampWikiLeft', () => {
	it('충분히 안쪽이면 그대로', () => {
		expect(clampWikiLeft(100, 1000)).toBe(100);
	});

	it('왼쪽 여백보다 작으면 WIKI_MARGIN 으로 clamp', () => {
		expect(clampWikiLeft(-50, 1000)).toBe(WIKI_MARGIN);
	});

	it('오른쪽으로 넘치면 팝업 폭만큼 물려 clamp', () => {
		// 뷰포트 400 이면 popW = min(352, 400-16) = 352, 우측 한계 = 400-352-8 = 40.
		const left = clampWikiLeft(999, 400);
		expect(left).toBe(400 - 352 - WIKI_MARGIN);
	});
});

describe('isWikiCaretVisible', () => {
	it('편집기 보이는 영역 안이면 true', () => {
		expect(isWikiCaretVisible(100, 120, 0, 500, 800)).toBe(true);
	});

	it('caret 이 편집기 스크롤 위로 넘어가면 false', () => {
		// 편집기 자체는 -50~200 에 있지만 caret 은 그보다 위(-100~-90).
		expect(isWikiCaretVisible(-100, -90, -50, 200, 800)).toBe(false);
	});

	it('caret 이 편집기 아래로 넘어가면 false', () => {
		expect(isWikiCaretVisible(600, 620, 0, 500, 800)).toBe(false);
	});

	it('편집기 자체가 뷰포트 아래로 스크롤돼도 뷰포트 기준으로 클램프', () => {
		// 편집기 bottom 이 뷰포트(800) 보다 훨씬 아래(2000) 여도 뷰포트로 클램프되므로
		// caret 이 뷰포트 안이면 보이는 것으로 판정.
		expect(isWikiCaretVisible(100, 120, 0, 2000, 800)).toBe(true);
	});
});
