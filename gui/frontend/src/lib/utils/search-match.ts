// BUG-371: 목록 · 보드의 검색을 누가 판정하나.
//
// 목록 · 보드는 본문 없이 받는다(BUG-210 slim — 수신량). 그래서 받은 퀘스트로는 제목 · 번호만 찾을 수 있고,
// 본문 · 댓글 · 첨부 이름은 서버(앱은 코어)가 판정해야 한다. 칸에는 '제목 / 본문' 이라고 적혀 있는데 본문을
// 못 찾던 것이 이 갈래가 '댓글 · 첨부 포함' 에만 있어서였다.

export interface SearchScope {
	titleOnly: boolean;
	comments: boolean;
	attachments: boolean;
}

/** 검색어가 있고 받은 목록에 없는 곳(본문 · 댓글 · 첨부)까지 봐야 하면 true. */
export function needsServerSearch(search: string, o: SearchScope): boolean {
	if (!search.trim()) return false;
	return !o.titleOnly || o.comments || o.attachments;
}
