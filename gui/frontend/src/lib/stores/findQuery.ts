// BUG-382: 페이지 내 검색(FindBar)의 지금 검색어 — 접혀서 화면에 없는 글을 가진 쪽이 본다.
//
// 찾기는 화면 글자(DOM)만 읽는다. 접힌 댓글은 내용이 아예 그려지지 않아 못 찾았다. 그래서 검색어를 여기
// 두고, 접힌 내용을 가진 컴포넌트(댓글 영역)가 **검색어가 든 것만 잠시 펼친다** — 저장해 둔 접기 상태는
// 그대로이고, 찾기를 닫으면(빈 값) 다시 접힌다. 펼쳐져 DOM 이 바뀌면 찾기 바가 다시 찾는다.
import { writable } from 'svelte/store';

export const findQuery = writable('');

/** 찾기 바와 같은 기준(대소문자 무시, 부분 일치)으로 글에 검색어가 있는지. 빈 검색어는 false. */
export function hasFindText(text: string, query: string): boolean {
	const q = query.toLowerCase();
	return q.length > 0 && text.toLowerCase().includes(q);
}
