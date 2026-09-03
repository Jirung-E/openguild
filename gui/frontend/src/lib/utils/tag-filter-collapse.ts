// REQ-019: 태그 필터 줄의 접힘 상태.
//
// 태그가 많은 길드에서 필터 줄이 화면을 몇 줄씩 차지해 목록을 아래로 밀어냈다
// (admin 보고). 기본은 접고, 누르면 펼친다.
//
// **화면별 sessionStorage** 다(admin 결정). 페이지를 오가도 유지되고 앱을 새로
// 켜면 접힌 기본으로 돌아온다 — [[BUG-259]] 의 작업기록 펼침과 같은 계층이다.
// "지금 이 화면을 어떻게 보고 있나" 는 오래 남길 취향이 아니다.
//
// 도서관은 사이드바 뷰와 탐색기 뷰가 **같은 필터**를 그린다. 같은 키를 주면
// 뷰를 바꿔도 접힘이 이어진다.

const PREFIX = 'openguild.tagFilterOpen.';

function session(): Storage | null {
	try {
		return typeof sessionStorage === 'undefined' ? null : sessionStorage;
	} catch {
		return null;
	}
}

/** 기본은 접힘. 저장된 값이 없거나 이상하면 그대로 접힘이다. */
export function loadTagFilterOpen(key: string): boolean {
	const s = session();
	if (!s) return false;
	try {
		return s.getItem(PREFIX + key) === '1';
	} catch {
		return false;
	}
}

export function saveTagFilterOpen(key: string, open: boolean): void {
	const s = session();
	if (!s) return;
	try {
		s.setItem(PREFIX + key, open ? '1' : '0');
	} catch {
		/* quota / disabled — 표시 상태라 무시해도 된다. */
	}
}

/**
 * 접힌 상태에서 어떤 칩을 그릴 것인가.
 *
 * **고른 태그는 접혀 있어도 보여야 한다.** 필터가 걸린 채 통째로 숨으면
 * 목록이 왜 줄었는지 알 수 없고 되돌릴 수단도 없다 — 원래 문제보다 나쁘다.
 *
 * 순서는 원래 목록 순서를 지킨다. 고른 것만 앞으로 당기면 펼쳤을 때 칩이
 * 자리를 옮겨 어디를 눌렀는지 놓친다.
 */
export function visibleTags(
	tags: readonly string[],
	selected: ReadonlySet<string>,
	open: boolean
): string[] {
	if (open) return [...tags];
	return tags.filter((t) => selected.has(t));
}

/**
 * 토글에 적을 개수.
 *
 * 펼쳤을 때는 전체 개수를, 접었을 때도 전체 개수를 보여 준다 — "여기 태그가
 * 몇 개 있다" 가 눌러 볼 이유가 되기 때문이다. 고른 개수는 칩 자체로 보인다.
 */
export function toggleCount(tags: readonly string[]): number {
	return tags.length;
}
