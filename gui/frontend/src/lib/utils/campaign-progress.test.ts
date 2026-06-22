import { describe, it, expect } from 'vitest';
import { isCampaignDone } from './campaign-progress';

describe('isCampaignDone (DEV-093 fix2)', () => {
	it('둘 다 없음 → false', () => {
		expect(
			isCampaignDone({
				checklist_total: 0,
				checklist_checked: 0,
				quest_total: 0,
				quest_done: 0
			})
		).toBe(false);
	});

	it('체크리스트만 있음, 100% → true', () => {
		expect(
			isCampaignDone({
				checklist_total: 3,
				checklist_checked: 3,
				quest_total: 0,
				quest_done: 0
			})
		).toBe(true);
	});

	it('체크리스트만 있음, 미완료 → false', () => {
		expect(
			isCampaignDone({
				checklist_total: 3,
				checklist_checked: 2,
				quest_total: 0,
				quest_done: 0
			})
		).toBe(false);
	});

	it('quest 만 있음, 100% → true', () => {
		expect(
			isCampaignDone({
				checklist_total: 0,
				checklist_checked: 0,
				quest_total: 5,
				quest_done: 5
			})
		).toBe(true);
	});

	it('quest 만 있음, 미완료 → false', () => {
		expect(
			isCampaignDone({
				checklist_total: 0,
				checklist_checked: 0,
				quest_total: 5,
				quest_done: 4
			})
		).toBe(false);
	});

	// 핵심 회귀 케이스 — fix2 이전엔 true 였음.
	it('체크리스트 100% + quest 미완료 → false (회귀)', () => {
		expect(
			isCampaignDone({
				checklist_total: 3,
				checklist_checked: 3,
				quest_total: 5,
				quest_done: 4
			})
		).toBe(false);
	});

	it('체크리스트 미완료 + quest 100% → false', () => {
		expect(
			isCampaignDone({
				checklist_total: 3,
				checklist_checked: 2,
				quest_total: 5,
				quest_done: 5
			})
		).toBe(false);
	});

	it('둘 다 100% → true', () => {
		expect(
			isCampaignDone({
				checklist_total: 3,
				checklist_checked: 3,
				quest_total: 5,
				quest_done: 5
			})
		).toBe(true);
	});

	it('quest_total / quest_done 가 null 이어도 안전 (서버 미배포 fallback)', () => {
		expect(
			isCampaignDone({
				checklist_total: 3,
				checklist_checked: 3,
				quest_total: null,
				quest_done: null
			})
		).toBe(true); // checklist 만 있다고 간주.
	});

	it('quest_total / quest_done 가 undefined 여도 안전', () => {
		expect(
			isCampaignDone({
				checklist_total: 3,
				checklist_checked: 3
			})
		).toBe(true);
	});
});
