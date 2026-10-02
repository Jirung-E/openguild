// DEV-435: 같은 본문 동시 편집 — 앱(꼬리표 문자열)과 서버(409)가 같은 충돌로 풀리고, 고른 대로 글이 다시 이어진다.

import { describe, it, expect } from 'vitest';
import {
	assemble,
	editConflictFromHttp,
	editConflictFromInvoke,
	EditConflictError,
	EDIT_CONFLICT_TAG,
	type EditConflict
} from './editConflict';
import { __test_only } from './transport';

const conflict: EditConflict = {
	reason: 'text',
	message: '같은 줄',
	current: '하나\n셋 — 남\n',
	segments: [
		{ kind: 'same', text: '하나\n' },
		{ kind: 'conflict', base: '셋\n', current: '셋 — 남\n', mine: '셋 — 나\n' }
	],
	new_id: null
};

describe('DEV-435 편집 충돌', () => {
	it('앱이 던진 꼬리표 문자열을 충돌로 푼다 — 다른 오류는 그대로', () => {
		const e = editConflictFromInvoke(EDIT_CONFLICT_TAG + JSON.stringify(conflict));
		expect(e).toBeInstanceOf(EditConflictError);
		expect(e?.conflict.segments).toHaveLength(2);
		expect(e?.message).toBe('같은 줄');
		expect(editConflictFromInvoke('quest not found')).toBeNull();
	});

	it('서버의 409 본문을 충돌로 푼다 — 409 가 아니거나 conflict 가 없으면 아니다', () => {
		expect(editConflictFromHttp(409, { error: '같은 줄', conflict })).toBeInstanceOf(
			EditConflictError
		);
		expect(editConflictFromHttp(409, { error: '새 schema' })).toBeNull();
		expect(editConflictFromHttp(400, { conflict })).toBeNull();
	});

	it('고른 대로 글을 다시 잇는다 — 지금 것 / 내 것 / 둘 다', () => {
		expect(assemble(conflict.segments, ['current'])).toBe('하나\n셋 — 남\n');
		expect(assemble(conflict.segments, ['mine'])).toBe('하나\n셋 — 나\n');
		expect(assemble(conflict.segments, ['both'])).toBe('하나\n셋 — 남\n셋 — 나\n');
	});

	it('저장할 때 시작할 때 본 글을 앱 명령에 싣는다', () => {
		const r = __test_only.routeToInvoke;
		expect(
			r({ method: 'PATCH', path: '/api/library/BOOK-001', body: { body: '새', base_body: '옛' } })
		).toEqual({
			cmd: 'update_book',
			args: { bookId: 'BOOK-001', title: null, body: '새', path: null, baseBody: '옛' }
		});
		expect(
			r({ method: 'PUT', path: '/api/rules/r', body: { content: '새', base_content: '옛' } })
		).toEqual({ cmd: 'set_rule', args: { slug: 'r', content: '새', baseContent: '옛' } });
		expect(
			r({
				method: 'PATCH',
				path: '/api/quests/by/DEV-001/comments/3',
				body: { body: '새', base_body: '옛' }
			})
		).toEqual({
			cmd: 'update_comment',
			args: { slug: 'DEV-001', id: 3, body: '새', baseBody: '옛' }
		});
		expect(
			r({
				method: 'PATCH',
				path: '/api/campaigns/C-001/comments/4',
				body: { body: '새', base_body: '옛' }
			})
		).toEqual({
			cmd: 'update_campaign_comment',
			args: { slug: 'C-001', id: 4, body: '새', baseBody: '옛' }
		});
	});
});
