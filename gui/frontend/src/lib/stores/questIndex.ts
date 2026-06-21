/**
 * DEV-140: 알려진 퀘스트 / 캠페인 ID 인덱스 — 본문 cross-link 용 공유 캐시.
 *
 * `[[DEV-033]]` / `[[C-001]]` 위키문법을 링크로 렌더할 때, 그리고 에디터에서
 * `XXX-NNN` 입력 시 자동완성 후보를 제공할 때 "그 ID 가 실재하는지" 판단이
 * 필요하다. 매 렌더마다 fetch 하면 낭비라 한 번 적재 후 메모, reindex 시 갱신.
 *
 * 미존재 ID 도 링크 자체는 만들되 빨강으로 표시 (사용자 결정) — 그래서
 * `known` set 은 "빨강 여부" 판정에만 쓰인다.
 */
import { writable, get } from 'svelte/store';
import { questsApi } from '$lib/api/quests';
import { campaignsApi } from '$lib/api/campaigns';
import { reindexBump } from '$lib/stores/reindex';

export interface IndexedRef {
	/** 표시용 — 자동완성 detail / 링크 title 에. */
	title: string;
	kind: 'quest' | 'campaign';
}

/** ID (대문자) → ref. quest_id ("DEV-033") + campaign_slug ("C-001") 통합. */
export const questIndex = writable<Map<string, IndexedRef>>(new Map());

let loaded = false;
let inflight: Promise<void> | null = null;

/**
 * 인덱스 적재 — 최초 1회 fetch 후 메모. `force` 면 재적재 (reindex 후).
 * 실패해도 조용히 빈 채로 둔다 (cross-link 은 보조 기능, 본문 표시 방해 X).
 */
export async function loadQuestIndex(force = false): Promise<void> {
	if (loaded && !force) return;
	if (inflight) return inflight;
	inflight = (async () => {
		try {
			const [quests, campaigns] = await Promise.all([questsApi.list(), campaignsApi.list()]);
			const next = new Map<string, IndexedRef>();
			for (const q of quests) {
				next.set(q.quest_id.toUpperCase(), { title: q.title, kind: 'quest' });
			}
			for (const c of campaigns) {
				next.set(c.campaign_slug.toUpperCase(), { title: c.title, kind: 'campaign' });
			}
			questIndex.set(next);
			loaded = true;
		} catch {
			/* 보조 기능 — 실패 시 빈 인덱스 유지 */
		} finally {
			inflight = null;
		}
	})();
	return inflight;
}

// reindex 후 인덱스 갱신 — 새로 추가된 퀘스트가 빨강 표시에서 벗어나도록.
// 최초 subscribe 의 즉시 호출(값 0)은 건너뛴다.
let bumpSeen = false;
reindexBump.subscribe(() => {
	if (!bumpSeen) {
		bumpSeen = true;
		return;
	}
	if (loaded) loadQuestIndex(true);
});

/** 동기 조회 — 이미 적재된 인덱스에서 ref 반환 (없으면 null). */
export function lookupRef(id: string): IndexedRef | null {
	return get(questIndex).get(id.toUpperCase()) ?? null;
}

/** 퀘스트/캠페인 ID 토큰 형식: 영문 prefix(2자+) - 숫자. 예 DEV-033, C-001, BUG-12. */
export const REF_TOKEN = /^[A-Za-z]{1,}-\d+$/;

/** ID 종류에 맞는 상세 페이지 경로. */
export function refHref(id: string, kind: 'quest' | 'campaign'): string {
	return kind === 'campaign'
		? `/campaigns/${encodeURIComponent(id)}`
		: `/quests/${encodeURIComponent(id)}`;
}
