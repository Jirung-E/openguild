// BUG-034: effectiveQuestDue 헬퍼 — required_due vs earliest_campaign_due
// 중 더 빠른 날짜를 반환. SVG / Home 임박 분류 양쪽에서 사용 → 회귀 방지 핵심.

import { describe, it, expect } from 'vitest';
import { effectiveQuestDue } from './quest-node-svg';
import type { Quest } from '$lib/types';

// 최소 Quest 헬퍼 — 기한 관련 필드만 의미 있음, 나머지는 dummy.
function q(opts: { required_due?: string | null; earliest_campaign_due?: string | null }): Quest {
	return {
		id: 1,
		quest_id: 'DEV-001',
		quest_type_id: 1,
		type_prefix: 'DEV',
		type_color: '#4A90D9',
		number: 1,
		title: 't',
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
		required_due: opts.required_due ?? null,
		earliest_campaign_due: opts.earliest_campaign_due ?? null
	};
}

describe('effectiveQuestDue', () => {
	it('둘 다 없음 → none', () => {
		expect(effectiveQuestDue(q({}))).toEqual({ date: null, source: 'none' });
	});

	it('quest 만 있음 → quest', () => {
		expect(effectiveQuestDue(q({ required_due: '2026-07-01' }))).toEqual({
			date: '2026-07-01',
			source: 'quest'
		});
	});

	it('campaign 만 있음 → campaign', () => {
		expect(effectiveQuestDue(q({ earliest_campaign_due: '2026-06-15' }))).toEqual({
			date: '2026-06-15',
			source: 'campaign'
		});
	});

	it('quest 가 더 빠름 → quest', () => {
		expect(
			effectiveQuestDue(q({ required_due: '2026-06-01', earliest_campaign_due: '2026-06-15' }))
		).toEqual({ date: '2026-06-01', source: 'quest' });
	});

	it('campaign 이 더 빠름 → campaign (캠페인 끝나기 전에 퀘스트도 끝나야 함)', () => {
		expect(
			effectiveQuestDue(q({ required_due: '2026-07-01', earliest_campaign_due: '2026-06-15' }))
		).toEqual({ date: '2026-06-15', source: 'campaign' });
	});

	it('같은 날짜 → quest (우선)', () => {
		// 동률 시엔 quest 가 직접 명시된 의도이므로 우선.
		expect(
			effectiveQuestDue(q({ required_due: '2026-06-15', earliest_campaign_due: '2026-06-15' }))
		).toEqual({ date: '2026-06-15', source: 'quest' });
	});

	it('빈 문자열 / 공백 trim — 미설정으로 취급', () => {
		expect(effectiveQuestDue(q({ required_due: '', earliest_campaign_due: '' }))).toEqual({
			date: null,
			source: 'none'
		});
		expect(effectiveQuestDue(q({ required_due: '   ' }))).toEqual({
			date: null,
			source: 'none'
		});
	});
});
