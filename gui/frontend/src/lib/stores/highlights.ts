// DEV-443: 개인 형광펜 — 대상마다 목록을 한 번만 받아 그 대상의 모든 글(본문 · 댓글 · 메모)이 나눠 쓴다.
// 댓글이 서른 개면 서른 번 묻게 되므로, 대상 하나에 요청 하나다.
import { writable, type Readable, type Writable } from 'svelte/store';
import { highlightsApi, type Highlight, type NewHighlight } from '$lib/api/highlights';

interface Entry {
	store: Writable<Highlight[]>;
	loadedAt: number;
	loading: Promise<void> | null;
}

const entries = new Map<string, Entry>();
/** 이보다 오래된 목록은 화면이 다시 열릴 때 새로 받는다 — 다른 창 · 다른 기기에서 칠한 것. */
const FRESH_MS = 3000;

function entry(target: string): Entry {
	let e = entries.get(target);
	if (!e) {
		e = { store: writable([]), loadedAt: 0, loading: null };
		entries.set(target, e);
	}
	return e;
}

function load(target: string, e: Entry) {
	if (e.loading) return;
	e.loading = highlightsApi
		.list(target)
		.then((list) => {
			e.store.set(list);
			e.loadedAt = Date.now();
		})
		.catch(() => {
			/* 형광펜은 없어도 되는 편의다 — 못 받으면 안 칠할 뿐, 글은 그대로 보인다. */
		})
		.finally(() => {
			e.loading = null;
		});
}

/** 대상의 형광펜 목록. 오래됐으면 새로 받는다. */
export function highlightsOf(target: string): Readable<Highlight[]> {
	const e = entry(target);
	if (Date.now() - e.loadedAt > FRESH_MS) load(target, e);
	return { subscribe: e.store.subscribe };
}

export async function addHighlight(target: string, body: NewHighlight): Promise<Highlight> {
	const h = await highlightsApi.add(target, body);
	entry(target).store.update((list) => [...list, h]);
	return h;
}

export async function removeHighlight(target: string, id: string): Promise<void> {
	await highlightsApi.remove(target, id);
	entry(target).store.update((list) => list.filter((x) => x.id !== id));
}
