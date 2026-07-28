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

/** DEV-219 후속: kind → 한국어 표시 라벨. 이전엔 editor-links.ts /
 *  MarkdownView.svelte 에 각각 중복 정의돼 drift 위험 — 여기 하나로 통합. */
export const KIND_LABEL: Record<Kind, string> = {
	quest: '퀘스트',
	campaign: '캠페인',
	rule: '규칙',
	book: '도서관'
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
 *
 * BUG-173: 실패는 **조용히 넘기되 memo 하지 않는다**. 예전엔 어떤 이유로든
 * 실패해도 `loaded` 판정이 어긋나 인덱스가 굳어버려, 실재하는 문서의
 * cross-link 가 계속 빨강으로 남았다. 아래 세 가지를 지킨다:
 *
 * 1. **네 소스 모두 개별 catch** — 하나가 실패해도 나머지는 인덱스에 들어간다
 *    (예전엔 quests/campaigns 에 catch 가 없어 하나만 실패해도 `Promise.all`
 *    이 reject → 인덱스가 통째로 빈 채로 남았다).
 * 2. **하나라도 실패하면 `loaded` 를 세우지 않는다** — 다음 호출이 재시도한다.
 *    예전엔 rules/library 가 null 이어도 `loaded = true` 라 그 종류의 링크는
 *    세션 내내 빨강으로 고착됐다.
 * 3. **실패분은 기존 값을 보존** — 새로 못 받은 종류는 이전 인덱스 항목을
 *    유지해, 일시적 실패가 이미 잘 보이던 링크를 빨갛게 만들지 않는다.
 */
export async function loadQuestIndex(force = false): Promise<void> {
	if (loaded && !force) return;
	if (inflight) return inflight;
	inflight = (async () => {
		try {
			// BUG-173: 네 소스 모두 개별 catch — 부분 실패를 전체 실패로 만들지 않는다.
			const [quests, campaigns, rules, books] = await Promise.all([
				questsApi.list().catch((e) => {
					console.warn('[questIndex] quests 적재 실패 — 재시도 예정', e);
					return null;
				}),
				campaignsApi.list().catch((e) => {
					console.warn('[questIndex] campaigns 적재 실패 — 재시도 예정', e);
					return null;
				}),
				rulesApi.list().catch((e) => {
					console.warn('[questIndex] rules 적재 실패 — 재시도 예정', e);
					return null;
				}),
				// DEV-218: 도서관 — rules 와 같은 이유로 개별 catch.
				libraryApi.list().catch((e) => {
					console.warn('[questIndex] library 적재 실패 — 재시도 예정', e);
					return null;
				})
			]);
			const allOk = !!quests && !!campaigns && !!rules && !!books;
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
			for (const c of campaigns ?? []) {
				const ref: IndexedRef = { title: c.title, kind: 'campaign' };
				ns.set(nsKey('campaign', c.campaign_slug), ref);
				bare.set(c.campaign_slug.toUpperCase(), ref);
			}
			for (const q of quests ?? []) {
				const ref: IndexedRef = { title: q.title, kind: 'quest' };
				ns.set(nsKey('quest', q.quest_id), ref);
				bare.set(q.quest_id.toUpperCase(), ref);
			}
			// BUG-173(3): 실패한 종류는 이전 값을 보존 — 일시 실패가 이미 잘
			// 보이던 링크를 빨갛게 만들지 않도록 기존 항목 위에 덮어쓴다.
			if (!allOk) {
				for (const [k, v] of get(questIndex)) if (!bare.has(k)) bare.set(k, v);
				for (const [k, v] of get(questIndexNs)) if (!ns.has(k)) ns.set(k, v);
			}
			questIndex.set(bare);
			questIndexNs.set(ns);
			// BUG-173(2): 전부 성공했을 때만 memo — 아니면 다음 호출이 재시도.
			loaded = allOk;
		} catch (e) {
			// 개별 catch 가 있어 여기까지 오는 건 예기치 못한 경우 — 조용히
			// 삼키지 말고 남긴다(진단 불가가 이 버그의 원인 중 하나였다).
			console.warn('[questIndex] 인덱스 적재 중 예기치 못한 오류', e);
		} finally {
			inflight = null;
		}
	})();
	return inflight;
}

/**
 * BUG-173: "실재하는데 빨강" 자가 치유 — missing 링크를 만났을 때 한 번만
 * 인덱스를 다시 받아본다.
 *
 * 인덱스는 세션당 1회만 적재(memo)되는데, 이 프로젝트는 **CLI/에이전트가 GUI 를
 * 켜둔 채로 퀘스트·문서를 계속 만든다**. 그렇게 나중에 생긴 문서를 가리키는
 * cross-link 는 인덱스에 없으니 빨강으로 남고, 새로고침 타이밍에 따라 "될 때도
 * 안 될 때도" 있는 것처럼 보였다.
 *
 * 그래서 렌더 결과에 missing 이 하나라도 있으면 **쿨다운(기본 15초) 안에서 1회**
 * 강제 재적재를 예약한다. 성공하면 `questIndex` 가 갱신되고, 이미 그려진 anchor
 * 들은 DEV-256 의 재-resolve 경로로 자동으로 파랑이 된다.
 *
 * 진짜로 없는 링크(오타 등)는 재적재해도 그대로 빨강이며, 쿨다운 때문에 매
 * 렌더마다 네트워크를 때리지 않는다.
 */
const REFRESH_COOLDOWN_MS = 15_000;
let lastMissingRefreshAt = 0;

export function refreshIndexForMissing(): void {
	const now = Date.now();
	if (now - lastMissingRefreshAt < REFRESH_COOLDOWN_MS) return;
	lastMissingRefreshAt = now;
	void loadQuestIndex(true);
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
