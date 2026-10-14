// DEV-442: 댓글 · 메모 팝업([[ComposeDock]])을 켜고 끈다(admin).
//
// 앱 전체에 하나뿐인 취향이다 — 좁게/넓게(`openguild.composeCompact`)와 같다. 한 입력창에서 끄면
// 모든 입력창에서 꺼지고, 다시 켜도 남는다. 기본은 켜짐이라 꺼 둔 사람만 'false' 가 적혀 있다.

import { writable } from 'svelte/store';

const KEY = 'openguild.composeDock';

function load(): boolean {
	try {
		return localStorage.getItem(KEY) !== 'false';
	} catch {
		return true;
	}
}

export const composeDockEnabled = writable<boolean>(load());

export function toggleComposeDock() {
	composeDockEnabled.update((on) => {
		const next = !on;
		try {
			localStorage.setItem(KEY, String(next));
		} catch {
			/* 무시 — 못 적어도 이번 판에서는 동작한다. */
		}
		return next;
	});
}
