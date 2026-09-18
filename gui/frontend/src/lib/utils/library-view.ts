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
