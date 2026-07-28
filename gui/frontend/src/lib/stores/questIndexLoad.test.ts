/**
 * BUG-173: 인덱스 적재 실패/부분실패가 "실재하는 문서의 cross-link 가 빨강"
 * 으로 굳어지지 않도록 하는 규칙을 고정한다.
 *
 * 기존 동작의 문제:
 *  - quests/campaigns 에 개별 catch 가 없어 하나만 실패해도 인덱스가 통째로 빔.
 *  - rules/library 가 실패해도 `loaded = true` 라 그 종류는 세션 내내 재시도 없음.
 *  - 실패가 완전히 조용해 진단이 불가능.
 *
 * 모듈 상태(`loaded`)를 다루므로 각 케이스마다 `vi.resetModules()` 로 새로
 * import 한다.
 */
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { get } from 'svelte/store';

const questsList = vi.fn();
const campaignsList = vi.fn();
const rulesList = vi.fn();
const libraryList = vi.fn();

vi.mock('$lib/api/quests', () => ({ questsApi: { list: () => questsList() } }));
vi.mock('$lib/api/campaigns', () => ({ campaignsApi: { list: () => campaignsList() } }));
vi.mock('$lib/api/rules', () => ({ rulesApi: { list: () => rulesList() } }));
vi.mock('$lib/api/library', () => ({ libraryApi: { list: () => libraryList() } }));

const QUESTS = [{ quest_id: 'DEV-001', title: '퀘스트 하나' }];
const CAMPAIGNS = [{ campaign_slug: 'C-001', title: '캠페인' }];
const RULES = { entries: [{ slug: 'some-rule', content: '# 규칙 제목' }] };
const BOOKS = [{ book_id: 'BOOK-001', title: '책', path: '' }];

async function freshModule() {
	vi.resetModules();
	return await import('./questIndex');
}

beforeEach(() => {
	vi.clearAllMocks();
	questsList.mockResolvedValue(QUESTS);
	campaignsList.mockResolvedValue(CAMPAIGNS);
	rulesList.mockResolvedValue(RULES);
	libraryList.mockResolvedValue(BOOKS);
	vi.spyOn(console, 'warn').mockImplementation(() => {});
});

describe('BUG-173 loadQuestIndex 실패 내성', () => {
	it('전부 성공하면 네 종류가 모두 인덱스에 들어간다', async () => {
		const m = await freshModule();
		await m.loadQuestIndex();
		const bare = get(m.questIndex);
		expect(bare.get('DEV-001')?.kind).toBe('quest');
		expect(bare.get('C-001')?.kind).toBe('campaign');
		expect(bare.get('BOOK-001')?.kind).toBe('book');
		expect(bare.get('SOME-RULE')?.kind).toBe('rule');
	});

	it('quests 가 실패해도 나머지는 인덱스에 남는다 (전체 실패로 번지지 않음)', async () => {
		questsList.mockRejectedValue(new Error('network'));
		const m = await freshModule();
		await m.loadQuestIndex();
		const bare = get(m.questIndex);
		expect(bare.get('DEV-001')).toBeUndefined();
		// 예전엔 Promise.all reject → 인덱스가 통째로 비었다.
		expect(bare.get('C-001')?.kind).toBe('campaign');
		expect(bare.get('BOOK-001')?.kind).toBe('book');
	});

	it('하나라도 실패하면 memo 하지 않고 다음 호출에서 재시도한다', async () => {
		libraryList.mockRejectedValueOnce(new Error('boom'));
		const m = await freshModule();
		await m.loadQuestIndex();
		expect(get(m.questIndex).get('BOOK-001')).toBeUndefined();

		// force 없이 다시 호출 — 예전엔 loaded=true 라 그대로 끝났다.
		await m.loadQuestIndex();
		expect(libraryList).toHaveBeenCalledTimes(2);
		expect(get(m.questIndex).get('BOOK-001')?.kind).toBe('book');
	});

	it('성공 후에는 memo 되어 재요청하지 않는다 (force 일 때만 재적재)', async () => {
		const m = await freshModule();
		await m.loadQuestIndex();
		await m.loadQuestIndex();
		expect(questsList).toHaveBeenCalledTimes(1);

		await m.loadQuestIndex(true);
		expect(questsList).toHaveBeenCalledTimes(2);
	});

	it('일시적 실패가 이미 잘 보이던 링크를 빨갛게 만들지 않는다 (이전 값 보존)', async () => {
		const m = await freshModule();
		await m.loadQuestIndex();
		expect(get(m.questIndex).get('DEV-001')?.kind).toBe('quest');

		// 재적재 시 quests 만 실패 — 기존 DEV-001 은 유지돼야 한다.
		questsList.mockRejectedValue(new Error('flaky'));
		await m.loadQuestIndex(true);
		expect(get(m.questIndex).get('DEV-001')?.kind).toBe('quest');
	});

	it('실패는 조용히 삼키지 않고 경고를 남긴다 (진단 가능해야 함)', async () => {
		questsList.mockRejectedValue(new Error('network'));
		const m = await freshModule();
		await m.loadQuestIndex();
		expect(console.warn).toHaveBeenCalled();
	});
});

describe('BUG-173 refreshIndexForMissing 쿨다운', () => {
	it('쿨다운 안에서는 한 번만 재적재한다 (렌더마다 네트워크를 때리지 않음)', async () => {
		const m = await freshModule();
		await m.loadQuestIndex();
		expect(questsList).toHaveBeenCalledTimes(1);

		m.refreshIndexForMissing();
		m.refreshIndexForMissing();
		m.refreshIndexForMissing();
		// 마이크로태스크 큐 소진 대기.
		await new Promise((r) => setTimeout(r, 0));
		expect(questsList).toHaveBeenCalledTimes(2);
	});
});
