// DEV-453: 퀘스트 보드 · 목록은 한 메뉴('퀘스트')이고, 페이지 안에서 [보드 | 목록] 을 고른다.
// 메뉴를 누르면 마지막에 본 쪽으로 간다 — 길드가 아니라 사람의 취향이라 하나만 기억한다.

export type QuestView = 'board' | 'list';

const KEY = 'openguild.questView';

export function loadQuestView(): QuestView {
	try {
		return localStorage.getItem(KEY) === 'list' ? 'list' : 'board';
	} catch {
		return 'board';
	}
}

export function saveQuestView(view: QuestView): void {
	try {
		localStorage.setItem(KEY, view);
	} catch {
		/* 저장소를 못 쓰면 기억만 못 한다 */
	}
}

/** 메뉴의 '퀘스트' 가 가는 곳. */
export function questViewHref(view: QuestView = loadQuestView()): string {
	return `/?view=${view}`;
}
