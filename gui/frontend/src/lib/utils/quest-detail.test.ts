// DEV-372: [[BUG-262]] 의 회귀 방지선.
//
// 상세 페이지의 상태 변경은 예전에 왕복 세 번이었다 — PATCH 하고, 퀘스트를
// **통째로 다시 받고**, 이력 블록을 재마운트했다. 정작 PATCH 응답은 버렸다.
// 지금은 그 응답을 그대로 쓴다.
//
// 여기서 지키는 계약은 둘이다:
//   1. 상태 관련 필드는 응답 값으로 **갱신된다**.
//   2. 관계·태그·첨부처럼 상태와 무관한 것은 **그대로 남는다**.
//
// 2번이 깨지면 상태를 바꾼 순간 서브퀘스트/첨부 목록이 사라지는데, 재조회가
// 없으니 되돌아오지도 않는다. 실서버 대조(병합 결과 == 재조회 결과)로 한 번
// 확인했지만 그건 사람이 한 번 본 것뿐이라 여기 고정한다.
import { describe, it, expect } from 'vitest';
import { applyStatusUpdate } from './quest-detail';
import type { Quest, QuestDetail } from '$lib/types';

function baseQuest(over: Partial<Quest> = {}): Quest {
	return {
		id: 13,
		quest_id: 'DEV-007',
		quest_type_id: 3,
		type_prefix: 'DEV',
		type_color: '#4A90D9',
		number: 7,
		title: '본문 cross-link 데모',
		status_id: 2,
		status_slug: 'in_progress',
		status_name_en: 'In Progress',
		status_name_ko: '진행 중',
		status_color: '#4A90D9',
		urgency: 3,
		parent_quest_id: null,
		created_at: '2026-08-30T16:02:00+09:00',
		updated_at: '2026-09-06T16:15:04+09:00',
		comment_count: 6,
		discussion_unresolved: 1,
		discussion_resolved: 1,
		...over
	} as Quest;
}

function baseDetail(): QuestDetail {
	return {
		...baseQuest(),
		description: '본문',
		sub_quests: [baseQuest({ id: 4, quest_id: 'BUG-004' })],
		prerequisites: [baseQuest({ id: 5, quest_id: 'DEV-005' })],
		successors: [baseQuest({ id: 6, quest_id: 'DEV-006' })],
		tags: ['api', 'backend'],
		attachments: [{ path: 'attachments/preview-1.png', name: 'preview.png' }],
		position: null
	} as QuestDetail;
}

describe('applyStatusUpdate — BUG-262', () => {
	it('상태 필드가 응답 값으로 갱신된다', () => {
		const next = applyStatusUpdate(
			baseDetail(),
			baseQuest({
				status_id: 3,
				status_slug: 'testing',
				status_name_ko: '테스트 중',
				status_name_en: 'Testing',
				status_color: '#A47AE2',
				updated_at: '2026-09-06T17:00:00+09:00'
			})
		);
		expect(next.status_slug).toBe('testing');
		expect(next.status_id).toBe(3);
		expect(next.status_name_ko).toBe('테스트 중');
		expect(next.status_color).toBe('#A47AE2');
		expect(next.updated_at).toBe('2026-09-06T17:00:00+09:00');
	});

	it('관계·태그·첨부·본문은 그대로 남는다 — 이게 재조회를 없앤 근거다', () => {
		const before = baseDetail();
		const next = applyStatusUpdate(before, baseQuest({ status_slug: 'testing', status_id: 3 }));
		expect(next.sub_quests).toEqual(before.sub_quests);
		expect(next.prerequisites).toEqual(before.prerequisites);
		expect(next.successors).toEqual(before.successors);
		expect(next.tags).toEqual(before.tags);
		expect(next.attachments).toEqual(before.attachments);
		expect(next.description).toBe(before.description);
	});

	it('응답에서 빠진 키는 기존 값을 유지한다 — 서버가 빈 값을 생략한다', () => {
		// 서버는 `skip_serializing_if` 로 tags/description 등을 생략할 수 있다.
		const before = baseDetail();
		const slim = baseQuest({ status_slug: 'testing' });
		delete (slim as Partial<Quest>).comment_count;
		const next = applyStatusUpdate(before, slim);
		expect(next.comment_count).toBe(6);
		expect(next.tags).toEqual(['api', 'backend']);
	});

	it('댓글 수 같은 계산 필드는 응답 값이 이긴다 — 0 으로 덮이면 뱃지가 사라진다', () => {
		const next = applyStatusUpdate(
			baseDetail(),
			baseQuest({ status_slug: 'testing', comment_count: 7, discussion_unresolved: 0 })
		);
		expect(next.comment_count).toBe(7);
		expect(next.discussion_unresolved).toBe(0);
	});

	it('원본을 건드리지 않는다 — 실패 시 되돌릴 수 있어야 한다', () => {
		const before = baseDetail();
		const snapshot = JSON.parse(JSON.stringify(before));
		applyStatusUpdate(before, baseQuest({ status_slug: 'testing' }));
		expect(before).toEqual(snapshot);
	});
});
