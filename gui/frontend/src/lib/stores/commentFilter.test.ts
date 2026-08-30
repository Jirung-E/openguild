// REQ-017: 댓글 필터 3단계.
//
// 두 가지가 깨지면 안 된다. 하나는 **순환이 세 단계를 다 돈다**는 것 —
// 한 단계를 건너뛰면 그 상태에 영영 못 간다. 다른 하나는 **미해결 판정이
// 홈 컨베이어(BUG-238)와 같다**는 것 — 기준이 갈리면 홈에서 넘어왔는데
// 여기선 안 보이는 일이 생긴다.
import { describe, it, expect, beforeEach } from 'vitest';
import {
	nextCommentFilter,
	loadCommentFilter,
	saveCommentFilter,
	matchesCommentFilter,
	COMMENT_FILTERS,
	DEFAULT_COMMENT_FILTER,
	type CommentFilter
} from './commentFilter';

describe('nextCommentFilter — 순환', () => {
	it('전체 → 토론만 → 미해결만 → 전체 (좁혀 나가는 방향)', () => {
		expect(nextCommentFilter('all')).toBe('discussion');
		expect(nextCommentFilter('discussion')).toBe('unresolved');
		expect(nextCommentFilter('unresolved')).toBe('all');
	});

	it('세 번 누르면 제자리 — 어느 단계에서 시작해도', () => {
		for (const start of COMMENT_FILTERS) {
			const back = nextCommentFilter(nextCommentFilter(nextCommentFilter(start)));
			expect(back).toBe(start);
		}
	});

	it('세 단계를 모두 지난다 — 하나라도 건너뛰면 그 상태에 못 간다', () => {
		const seen = new Set<CommentFilter>();
		let cur: CommentFilter = 'all';
		for (let i = 0; i < 3; i++) {
			seen.add(cur);
			cur = nextCommentFilter(cur);
		}
		expect(seen.size).toBe(3);
	});

	it('모르는 값이 들어와도 멈추지 않는다 — 저장소가 오염됐을 수 있다', () => {
		expect(COMMENT_FILTERS).toContain(nextCommentFilter('bogus' as CommentFilter));
	});
});

describe('matchesCommentFilter — 판정', () => {
	const plain = { discussion: false, resolved: false };
	const open = { discussion: true, resolved: false };
	const done = { discussion: true, resolved: true };

	it('전체는 다 통과', () => {
		for (const e of [plain, open, done]) {
			expect(matchesCommentFilter('all', e)).toBe(true);
		}
	});

	it('토론만 — 해결된 토론도 포함한다 (지나간 것까지 보는 단계다)', () => {
		expect(matchesCommentFilter('discussion', plain)).toBe(false);
		expect(matchesCommentFilter('discussion', open)).toBe(true);
		expect(matchesCommentFilter('discussion', done)).toBe(true);
	});

	it('미해결만 — 홈 컨베이어(BUG-238)와 같은 기준: discussion && !resolved', () => {
		expect(matchesCommentFilter('unresolved', plain)).toBe(false);
		expect(matchesCommentFilter('unresolved', open)).toBe(true);
		expect(matchesCommentFilter('unresolved', done)).toBe(false);
	});

	it('필드가 없거나 null 이어도 던지지 않는다 — 서버가 안 내려줄 수 있다', () => {
		expect(matchesCommentFilter('unresolved', {})).toBe(false);
		expect(matchesCommentFilter('discussion', { discussion: null })).toBe(false);
		expect(matchesCommentFilter('unresolved', { discussion: true, resolved: null })).toBe(true);
	});
});

describe('영속 — localStorage', () => {
	beforeEach(() => {
		localStorage.clear();
	});

	it('저장된 것이 없으면 전체', () => {
		expect(loadCommentFilter()).toBe(DEFAULT_COMMENT_FILTER);
		expect(DEFAULT_COMMENT_FILTER).toBe('all');
	});

	it('왕복한다 — 앱을 껐다 켜도 유지되는 것이 요점이다', () => {
		saveCommentFilter('unresolved');
		expect(loadCommentFilter()).toBe('unresolved');
		saveCommentFilter('all');
		expect(loadCommentFilter()).toBe('all');
	});

	it('모르는 값이 저장돼 있으면 기본값', () => {
		localStorage.setItem('openguild.commentFilter', 'fortnight');
		expect(loadCommentFilter()).toBe(DEFAULT_COMMENT_FILTER);
	});
});
