// BUG-296: 도서관 화면에서 **목록 쪽을 그릴지**를 한 곳에서 정한다.
//
// 자식창(크로스링크·검색의 "새 창으로 열기")은 그 문서 하나만 보라고 띄운 창이다. 돌아갈
// 목록이 없으니 '← 목록' 버튼은 눌러도 말이 안 되고, 트리(폴더+문서 목록)도 그 창의 용도와
// 어긋난다. TitleBar 가 뒤로·검색을 숨기는 것과 같은 판정을 쓴다.
//
// 판단을 템플릿의 `{#if}` 안에 두면 두 자리가 따로 놀기 쉬워(한쪽만 고치면 트리는 사라졌는데
// 버튼은 남는다) 여기로 뺐다.

export type LibraryViewMode = 'tree' | 'explorer';

/** 좌측 트리(폴더 + 문서 목록)와 그 옆 드래그 핸들을 그리나. */
export function showsTree(mode: LibraryViewMode, isChildWindow: boolean): boolean {
	return mode === 'tree' && !isChildWindow;
}

/** 아이콘 보기에서 문서를 연 뒤의 '← 목록' 버튼을 그리나. */
export function showsBackToList(mode: LibraryViewMode, isChildWindow: boolean): boolean {
	return mode === 'explorer' && !isChildWindow;
}

/** 문서가 창 전체 폭을 쓰나 — 목록이 없으면 언제나 그렇다. */
export function isSingleColumn(mode: LibraryViewMode, isChildWindow: boolean): boolean {
	return !showsTree(mode, isChildWindow);
}

/** BUG-298: 목록을 다시 받을 때인가.
 *
 * 목록은 진입 때 한 번만 받았다. 그래서 CLI·에이전트가 밖에서 만든 문서는 **폴더를 나갔다
 * 들어와도** 안 보였다(폴더 이동은 이미 받아 둔 목록을 걸러 보는 것뿐이다). 앱과 CLI 를 같이
 * 쓰는 것이 이 도구의 기본 사용법이라, 옛 목록을 계속 보여 주면 "안 만들어졌나" 하고 다시
 * 만들게 된다.
 *
 * 그렇다고 폴더를 옮길 때마다 통째로 받으면 큰 길드에서 낭비다 — 방금 받았으면 건너뛴다.
 */
export function shouldRefresh(
	now: number,
	lastLoadedAt: number,
	loading: boolean,
	staleMs = 1500
): boolean {
	if (loading) return false;
	return now - lastLoadedAt >= staleMs;
}

/** 밖으로 낼 것 — 복사·내보내기가 무엇을 집을지. */
export interface TakeOut {
	/** 고른 것들. 화면이 쓰는 열쇠 그대로 — 폴더는 `folder:<경로>`, 문서는 문서 번호. */
	picks?: string[];
	id?: string;
	folder?: string;
}

/**
 * BUG-314: **고른 것이 먼저다**(admin).
 *
 * 예전에는 고른 것을 아예 안 봤다 — 타일 셋을 골라 놓고 [복사] 를 눌러도 지금 폴더가 통째로
 * 복사됐다. 탐색기에서 셋을 골라 `⌘C` 를 눌렀는데 폴더가 붙는 셈이다.
 *
 * 순서는 **고른 것 → 열어 둔 문서 → 지금 폴더**다. 마지막 것이 기본인 이유는, 아무것도 안
 * 고르고 [복사] 를 눌렀을 때 "지금 보고 있는 것" 이 그 폴더이기 때문이다.
 */
export function takeOutTarget(
	pickedIds: Iterable<string>,
	selectedId: string | null,
	explorerPath: string
): TakeOut {
	const picks = [...pickedIds];
	if (picks.length > 0) return { picks };
	if (selectedId) return { id: selectedId };
	return { folder: explorerPath };
}
