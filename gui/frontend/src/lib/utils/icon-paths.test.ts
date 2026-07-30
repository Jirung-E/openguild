// DEV-302: 아이콘 도형 단일 출처 + 문자열 SVG 조립 헬퍼.
//
// 회귀 대상: 보드 노드처럼 SVG 를 문자열로 만드는 코드가 이모지(⏱ ⛺ 💬)를
// 그대로 넣어 OS 마다 컬러 이모지로 렌더되던 문제. 도형은 Icon.svelte 와 같은
// 상수를 써야 하고, data URL 안에서는 currentColor 가 안 먹는다.

import { describe, it, expect } from 'vitest';
import { ICON_SHAPES, iconSvgGroup, type IconName } from './icon-paths';
import { makeQuestNodeSvgUrl } from './quest-node-svg';
import type { Quest } from '$lib/types';

const NAMES = Object.keys(ICON_SHAPES) as IconName[];

/** 기본 emoji presentation 범위 — check-no-emoji-icons.mjs 와 같은 기준. */
function hasColorEmoji(s: string): boolean {
	for (const ch of s) {
		const cp = ch.codePointAt(0)!;
		if (
			(cp >= 0x1f300 && cp <= 0x1faff) ||
			cp === 0x2705 ||
			cp === 0x274c ||
			(cp >= 0x2753 && cp <= 0x2755) ||
			(cp >= 0x2b06 && cp <= 0x2b07) ||
			cp === 0x26fa ||
			(cp >= 0x231a && cp <= 0x231b) ||
			(cp >= 0x23f0 && cp <= 0x23fa)
		) {
			return true;
		}
	}
	return false;
}

describe('ICON_SHAPES', () => {
	it('모든 아이콘이 비어있지 않은 SVG 도형을 가진다', () => {
		expect(NAMES.length).toBeGreaterThan(0);
		for (const name of NAMES) {
			expect(ICON_SHAPES[name], name).toMatch(/<(path|circle|rect|ellipse)\b/);
		}
	});

	it('도형 안에 이모지가 없다', () => {
		for (const name of NAMES) {
			expect(hasColorEmoji(ICON_SHAPES[name]), name).toBe(false);
		}
	});
});

describe('iconSvgGroup', () => {
	it('위치·크기를 transform 으로 반영한다 (16 기준 scale)', () => {
		const g = iconSvgGroup('clock', { x: 20, y: 30, size: 8, color: '#abc' });
		expect(g).toContain('translate(20,30)');
		expect(g).toContain('scale(0.5)'); // 8 / 16
		expect(g).toContain('stroke="#abc"');
	});

	it('currentColor 를 명시 색으로 치환한다 (data URL 에서는 색 문맥이 없다)', () => {
		// select 아이콘의 내부 원은 fill="currentColor" 로 그린다.
		expect(ICON_SHAPES.select).toContain('currentColor');
		const g = iconSvgGroup('select', { x: 0, y: 0, color: '#ff0000' });
		expect(g).not.toContain('currentColor');
		expect(g.match(/#ff0000/g)?.length).toBeGreaterThanOrEqual(2);
	});
});

function quest(over: Partial<Quest> = {}): Quest {
	return {
		id: 1,
		quest_id: 'DEV-001',
		quest_type_id: 1,
		type_prefix: 'DEV',
		type_color: '#4A90D9',
		number: 1,
		title: '제목',
		description: null,
		status_id: 1,
		status_slug: 'open',
		status_name_en: 'Open',
		status_name_ko: '게시됨',
		status_color: '#8B95A1',
		urgency: 3,
		parent_quest_id: null,
		created_at: '',
		updated_at: '',
		required_due: null,
		earliest_campaign_due: null,
		...over
	} as Quest;
}

/** data URL → SVG 소스. */
function svgOf(url: string): string {
	return decodeURIComponent(url.replace(/^data:image\/svg\+xml;charset=utf-8,/, ''));
}

describe('makeQuestNodeSvgUrl — 기한 아이콘 (DEV-302)', () => {
	it('노드 SVG 에 이모지가 들어가지 않는다', () => {
		const svg = svgOf(makeQuestNodeSvgUrl(quest({ required_due: '2026-08-01' })));
		expect(hasColorEmoji(svg)).toBe(false);
	});

	it('quest 기한이면 시계 도형, 캠페인 기한이면 텐트 도형', () => {
		const own = svgOf(makeQuestNodeSvgUrl(quest({ required_due: '2026-08-01' })));
		expect(own).toContain(ICON_SHAPES.clock);
		expect(own).not.toContain(ICON_SHAPES.campaign);

		const fromCampaign = svgOf(
			makeQuestNodeSvgUrl(quest({ earliest_campaign_due: '2026-08-01' }))
		);
		expect(fromCampaign).toContain(ICON_SHAPES.campaign);
		expect(fromCampaign).not.toContain(ICON_SHAPES.clock);
	});

	it('기한이 없으면 아이콘도 없다', () => {
		const svg = svgOf(makeQuestNodeSvgUrl(quest()));
		expect(svg).not.toContain(ICON_SHAPES.clock);
		expect(svg).not.toContain(ICON_SHAPES.campaign);
	});
});
