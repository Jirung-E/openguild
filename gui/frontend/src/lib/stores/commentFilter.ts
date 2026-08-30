// REQ-017: 댓글 필터 — 전체 / 토론만 / 미해결 토론만.
//
// 예전엔 '토론만' 켜기·끄기 2단계였다. 실제로 보고 싶은 것은 셋이다 — 지금
// 손이 필요한 **미해결 토론**, 지나간 것까지 포함한 **토론 전체**, 그리고
// **전부**. 2단계로는 미해결만 추리려면 눈으로 훑어야 했고, 토론이 쌓인
// 퀘스트에서 해결된 것이 섞여 나오면 '토론만' 의 쓸모가 그만큼 떨어진다.
//
// **순환 순서는 좁혀 나가는 방향이다** (admin 결정): 전체 → 토론만 →
// 미해결만 → 전체. 기본값에서 시작해 한 번 누를 때마다 범위가 준다.
//
// **영속은 localStorage** (admin 결정). 보기 모드(compact/full)와 같은 계층 —
// 앱을 껐다 켜도 유지된다. 퀘스트별이 아니라 전역이다: "나는 미해결 토론만
// 본다" 는 그 사람의 작업 습관이지 특정 퀘스트의 성질이 아니다.
//
// 판정 기준은 홈의 "토론 댓글" 컨베이어(`?focus=discussion`, [[BUG-238]])와
// 같다 — 미해결 = `discussion && !resolved`. 두 곳이 다른 기준을 쓰면 홈에서
// 넘어왔는데 여기선 안 보이는 일이 생긴다.

/** 댓글 목록에 걸리는 필터. */
export type CommentFilter = 'all' | 'discussion' | 'unresolved';

/** 순환 순서 그대로. 좁혀 나가는 방향. */
export const COMMENT_FILTERS: CommentFilter[] = ['all', 'discussion', 'unresolved'];
export const DEFAULT_COMMENT_FILTER: CommentFilter = 'all';

const KEY = 'openguild.commentFilter';

/** 다음 단계. 마지막에서 처음으로 돌아온다. */
export function nextCommentFilter(cur: CommentFilter): CommentFilter {
	const i = COMMENT_FILTERS.indexOf(cur);
	// 모르는 값이 들어와도 멈추지 않는다 — 첫 단계 다음으로 보낸다.
	if (i < 0) return COMMENT_FILTERS[1];
	return COMMENT_FILTERS[(i + 1) % COMMENT_FILTERS.length];
}

export function loadCommentFilter(): CommentFilter {
	try {
		if (typeof localStorage === 'undefined') return DEFAULT_COMMENT_FILTER;
		const raw = localStorage.getItem(KEY);
		return COMMENT_FILTERS.includes(raw as CommentFilter)
			? (raw as CommentFilter)
			: DEFAULT_COMMENT_FILTER;
	} catch {
		return DEFAULT_COMMENT_FILTER;
	}
}

export function saveCommentFilter(f: CommentFilter): void {
	try {
		if (typeof localStorage === 'undefined') return;
		localStorage.setItem(KEY, f);
	} catch {
		/* quota / disabled — 표시 설정이라 무시해도 된다. */
	}
}

/** 필터가 보는 최소 단위. entry 전체를 넘길 필요가 없다. */
export type CommentLike = { discussion?: boolean | null; resolved?: boolean | null };

/**
 * 이 댓글이 현재 필터의 **대상**인가.
 *
 * 목록에서 숨기는 데 쓰지 않는다 — 스레드 문맥을 지키려고 비대상은 dim 만
 * 한다([[DEV-213]]). 여기는 "dim 할지" 의 반대말이다.
 */
export function matchesCommentFilter(f: CommentFilter, e: CommentLike): boolean {
	if (f === 'all') return true;
	if (!e.discussion) return false;
	if (f === 'discussion') return true;
	return !e.resolved;
}
