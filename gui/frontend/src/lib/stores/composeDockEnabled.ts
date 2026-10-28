// DEV-442: 댓글 · 메모 팝업([[ComposeDock]])을 켜고 끈다(admin).
//
// DEV-459(admin): **입력창 종류마다 따로** 기억한다 — 댓글 · 답글 · 메모. 예전에는 앱 전체에 값이
// 하나라 메모에서 끄면 댓글도 같이 꺼졌다. 다시 켜도 남는다. 기본은 켜짐이라 꺼 둔 곳만 'false' 가
// 적혀 있다.

import { writable } from 'svelte/store';

export type ComposeDockKind = 'comment' | 'reply' | 'memo';

/** DEV-442 때의 값 하나 — 종류별 값이 아직 없으면 이걸 따른다(꺼 둔 사람이 갑자기 켜진 것을 보지 않게). */
const OLD_KEY = 'openguild.composeDock';
const KINDS: ComposeDockKind[] = ['comment', 'reply', 'memo'];

function keyOf(kind: ComposeDockKind): string {
	return `${OLD_KEY}.${kind}`;
}

function load(kind: ComposeDockKind): boolean {
	try {
		return (localStorage.getItem(keyOf(kind)) ?? localStorage.getItem(OLD_KEY)) !== 'false';
	} catch {
		return true;
	}
}

function loadAll(): Record<ComposeDockKind, boolean> {
	return Object.fromEntries(KINDS.map((k) => [k, load(k)])) as Record<ComposeDockKind, boolean>;
}

export const composeDockEnabled = writable<Record<ComposeDockKind, boolean>>(loadAll());

export function toggleComposeDock(kind: ComposeDockKind) {
	composeDockEnabled.update((all) => {
		const next = !all[kind];
		try {
			localStorage.setItem(keyOf(kind), String(next));
		} catch {
			/* 무시 — 못 적어도 이번 판에서는 동작한다. */
		}
		return { ...all, [kind]: next };
	});
}

/** 시험용 — 모두 켜짐으로 되돌린다(값은 모듈에 하나라 시험끼리 샌다). */
export function resetComposeDock() {
	composeDockEnabled.set(loadAll());
}
