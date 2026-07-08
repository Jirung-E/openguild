/**
 * DEV-140: 알려진 퀘스트 / 캠페인 ID 인덱스 — 본문 cross-link 용 공유 캐시.
 *
 * `[[DEV-033]]` / `[[C-001]]` 위키문법을 링크로 렌더할 때, 그리고 에디터에서
 * `XXX-NNN` 입력 시 자동완성 후보를 제공할 때 "그 ID 가 실재하는지" 판단이
 * 필요하다. 매 렌더마다 fetch 하면 낭비라 한 번 적재 후 메모, reindex 시 갱신.
 *
 * 미존재 ID 도 링크 자체는 만들되 빨강으로 표시 (사용자 결정) — 그래서
 * `known` set 은 "빨강 여부" 판정에만 쓰인다.
 *
 * DEV-219: quest/campaign/rules/library ID 는 서로 다른 네임스페이스에서
 * 나오지만(퀘스트 타입 prefix 는 사용자가 자유롭게 커스텀 가능) 겹칠 수
 * 있다 — 예: `types add --prefix C` 를 하면 퀘스트 C-001 이 캠페인 C-001 과
 * 충돌. `[[kind:ID]]` 명시 네임스페이스 문법으로 충돌을 "구분"한다(금지 아님,
 * admin/claude 합의 — 댓글 #2~#4). 접두 없는 `[[ID]]` 는 하위호환으로 계속
 * 동작 — 우선순위 quest > campaign > library > rules 로 고정(charset 상
 * REF_TOKEN 형식만 quest/campaign/book 후보, 그 외는 rule).
 */
import { writable, get } from 'svelte/store';
import { questsApi } from '$lib/api/quests';
import { campaignsApi } from '$lib/api/campaigns';
import { rulesApi } from '$lib/api/rules';
// DEV-218: 도서관 문서(BOOK-NNN)도 cross-link 대상.
import { libraryApi } from '$lib/api/library';
import { reindexBump } from '$lib/stores/reindex';

export type Kind = 'quest' | 'campaign' | 'rule' | 'book';

/** DEV-219: `[[kind:ID]]` 접두 별칭 — 긴 이름 + 짧은 별칭 모두 허용(소문자만 매칭, 호출부에서 lowercase). */
export const KIND_ALIASES: Record<string, Kind> = {
	quest: 'quest',
	q: 'quest',
	campaign: 'campaign',
	c: 'campaign',
	rule: 'rule',
	rules: 'rule',
	r: 'rule',
	book: 'book',
	library: 'book',
	lib: 'book'
};

/** DEV-219: 자동완성이 항상 삽입하는 정규 네임스페이스 단어 (admin 결정 — 접두 강제). */
export const KIND_NAMESPACE: Record<Kind, string> = {
	quest: 'quest',
	campaign: 'campaign',
	rule: 'rules',
	book: 'library'
};

export interface IndexedRef {
	/** 표시용 — 자동완성 detail / 링크 title 에. */
	title: string;
	kind: Kind;
	/**
	 * DEV-173: 규칙 전용 — 원본 대소문자 slug. 규칙 slug 는 파일명이라
	 * 대소문자를 보존해야 href (`/rules?slug=..`) 가 정확하다.
	 * quest/campaign 은 ID 가 항상 대문자 정규형이라 불필요.
	 */
	slug?: string;
	/**
	 * DEV-239: 도서관 문서 전용 — 소속 폴더 경로 ("" = 최상위). cross-link
	 * 자동완성이 `[[경로/제목` 형태로 타이핑해도 후보를 찾을 수 있게 매칭에
	 * 사용 — 실제 삽입되는 링크는 항상 `[[BOOK-NNN]]` (경로 자체는 링크
	 * 문법에 없음, admin 결정).
	 */
	path?: string;
}

/**
 * ID (대문자) → ref. quest_id ("DEV-033") + campaign_slug ("C-001") + 규칙 slug 통합.
 *
 * DEV-219: 네 종류가 flat 하게 한 맵을 공유해 ID 가 겹치면 나중에 적재된
 * 쪽이 먼저 것을 덮어썼다(quest→campaign→rule→book 순으로 set, book 최종
 * 승자). 이제 quest > campaign > library > rules 우선순위로 **의도적으로**
 * 낮은 순위부터 set — 접두 없는 `[[ID]]` 는 이 맵으로 풀린다(하위호환).
 */
export const questIndex = writable<Map<string, IndexedRef>>(new Map());

/** DEV-219: `kind:ID` 로 키 지어 명시 네임스페이스 조회 — 충돌 없이 항상 정확한 대상. */
export const questIndexNs = writable<Map<string, IndexedRef>>(new Map());

let loaded = false;
let inflight: Promise<void> | null = null;

function nsKey(kind: Kind, id: string): string {
	return `${kind}:${id.toUpperCase()}`;
}

/**
 * 인덱스 적재 — 최초 1회 fetch 후 메모. `force` 면 재적재 (reindex 후).
 * 실패해도 조용히 빈 채로 둔다 (cross-link 은 보조 기능, 본문 표시 방해 X).
 */
export async function loadQuestIndex(force = false): Promise<void> {
	if (loaded && !force) return;
	if (inflight) return inflight;
	inflight = (async () => {
		try {
			// DEV-173: 규칙도 인덱스에 — 실패해도 quest/campaign 은 살린다
			// (rules 는 보조 대상이라 개별 catch).
			const [quests, campaigns, rules, books] = await Promise.all([
				questsApi.list(),
				campaignsApi.list(),
				rulesApi.list().catch(() => null),
				// DEV-218: 도서관 — rules 와 같은 이유로 개별 catch.
				libraryApi.list().catch(() => null)
			]);
			const ns = new Map<string, IndexedRef>();
			const bare = new Map<string, IndexedRef>();
			// DEV-219: bare 맵은 quest > campaign > library > rules 우선순위 —
			// 낮은 순위부터 set 해 마지막(quest)이 충돌 시 승자가 되게 한다.
			for (const r of rules?.entries ?? []) {
				// 제목 = 본문 첫 `# 헤딩` (없으면 slug 그대로).
				const heading = /^#\s+(.+)$/m.exec(r.content ?? '')?.[1]?.trim();
				const ref: IndexedRef = { title: heading || r.slug, kind: 'rule', slug: r.slug };
				ns.set(nsKey('rule', r.slug), ref);
				bare.set(r.slug.toUpperCase(), ref);
			}
			for (const b of books ?? []) {
				// BOOK-NNN 은 XXX-NNN 형식이라 기존 정규식에 자동 포함 — 인덱스에
				// 실으면 렌더/자동완성이 그대로 동작 (DEV-218).
				const ref: IndexedRef = { title: b.title, kind: 'book', path: b.path };
				ns.set(nsKey('book', b.book_id), ref);
				bare.set(b.book_id.toUpperCase(), ref);
			}
			for (const c of campaigns) {
				const ref: IndexedRef = { title: c.title, kind: 'campaign' };
				ns.set(nsKey('campaign', c.campaign_slug), ref);
				bare.set(c.campaign_slug.toUpperCase(), ref);
			}
			for (const q of quests) {
				const ref: IndexedRef = { title: q.title, kind: 'quest' };
				ns.set(nsKey('quest', q.quest_id), ref);
				bare.set(q.quest_id.toUpperCase(), ref);
			}
			questIndex.set(bare);
			questIndexNs.set(ns);
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

/**
 * DEV-219: `[[kind:ID]]` 토큰을 접두/나머지로 분리. 접두가 KIND_ALIASES 에
 * 없으면(혹은 `:` 자체가 없으면) kind=null, id=토큰 그대로 — 하위호환.
 */
export function parseCrossLinkToken(token: string): { kind: Kind | null; id: string } {
	const i = token.indexOf(':');
	if (i > 0) {
		const prefix = token.slice(0, i).toLowerCase();
		const kind = KIND_ALIASES[prefix];
		if (kind) return { kind, id: token.slice(i + 1) };
	}
	return { kind: null, id: token };
}

/** 동기 조회 — 접두 없는 bare 토큰 전용(하위호환). 이미 적재된 인덱스에서 ref 반환. */
export function lookupRef(id: string): IndexedRef | null {
	return get(questIndex).get(id.toUpperCase()) ?? null;
}

/** DEV-219: 접두 유무와 무관하게 토큰을 완전히 풀어 kind/ref 를 함께 반환. */
export function resolveCrossLinkToken(token: string): { kind: Kind | null; id: string; ref: IndexedRef | null } {
	const { kind, id } = parseCrossLinkToken(token);
	if (kind) {
		return { kind, id, ref: get(questIndexNs).get(nsKey(kind, id)) ?? null };
	}
	const ref = lookupRef(id);
	return { kind: ref?.kind ?? null, id, ref };
}

/** 퀘스트/캠페인 ID 토큰 형식: 영문 prefix(2자+) - 숫자. 예 DEV-033, C-001, BUG-12. */
export const REF_TOKEN = /^[A-Za-z]{1,}-\d+$/;

/** ID 종류에 맞는 상세 페이지 경로. 규칙은 원본 대소문자 slug 필요 (DEV-173). */
export function refHref(id: string, kind: Kind, slug?: string): string {
	if (kind === 'rule') {
		return `/rules?slug=${encodeURIComponent(slug ?? id.toLowerCase())}`;
	}
	// DEV-218: 도서관 딥링크 — /library?id=BOOK-NNN.
	if (kind === 'book') {
		return `/library?id=${encodeURIComponent(id)}`;
	}
	return kind === 'campaign'
		? `/campaigns/${encodeURIComponent(id)}`
		: `/quests/${encodeURIComponent(id)}`;
}
