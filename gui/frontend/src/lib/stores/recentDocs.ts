// DEV-276: 최근 본 문서 — 타이틀바 검색 팔레트 옆 "최근" 버튼의 소스.
//
// 사용자 요청("검색 팔레트 옆에 '지금 열려있는 탭' 버튼")을 확인한 결과
// **현재 창에서 최근 방문한 문서 목록**(브라우저 '최근 탭' 식)으로 확정.
// 앱에 탭 개념은 없고, 뒤로가기는 한 칸씩만 가므로 "아까 그 문서"로 바로
// 점프하는 수단이 없었다.
//
// 추적 대상: 문서 성격의 라우트만 — 퀘스트/캠페인 상세, 규칙, 도서관.
// 목록/보드/설정 등 "탐색 화면"은 다시 찾아갈 필요가 적어 제외(노이즈 방지).
//
// 영속: sessionStorage — "지금 보고 있는 세션의 최근"이라는 의미에 맞고,
// 앱을 껐다 켜면 초기화되는 게 자연스럽다(recents(길드 목록)와 별개).

import { writable, get } from 'svelte/store';
// BUG-159: 제목은 cross-link 인덱스에서 조회 — DOM 스크래핑보다 정확/최신.
import { questIndexNs, type IndexedRef } from '$lib/stores/questIndex';

export type RecentKind = 'quest' | 'campaign' | 'rule' | 'book';

export interface RecentDoc {
	/** 라우트 경로 — 클릭 시 이동 대상이자 중복 판정 키. */
	href: string;
	kind: RecentKind;
	/** 식별자 — DEV-001 / C-001 / 규칙 slug / BOOK-001. */
	label: string;
	/**
	 * 제목 — **저장하지 않고 표시 시점에 인덱스에서 조회**(recentDocTitle).
	 *
	 * BUG-159: 예전엔 방문 시 화면의 `main h1` 을 긁어 저장했는데 페이지마다
	 * h1 의 의미가 달라 엉뚱한 값이 들어갔다 — 규칙은 `# {slug}`(라벨과 중복),
	 * 도서관은 첫 h1 이 페이지 제목 "도서관"이라 모든 문서가 같은 제목으로
	 * 보였다. cross-link 인덱스(questIndexNs)가 이미 ID→제목을 들고 있고
	 * reindex 시 갱신되므로 그걸 쓰는 게 정확하고 최신이다(문서 이름을
	 * 바꿔도 목록이 따라온다).
	 *
	 * 하위호환: 예전 세션이 남긴 값이 있으면 인덱스 조회 실패 시 폴백.
	 */
	title?: string;
	/** 마지막 방문 시각(ms) — 정렬용. */
	ts: number;
}

const KEY = 'openguild.recentDocs';
/** 목록 상한 — 드롭다운 한 화면에 들어오는 정도. */
export const MAX_RECENT_DOCS = 12;

function load(): RecentDoc[] {
	if (typeof sessionStorage === 'undefined') return [];
	try {
		const raw = sessionStorage.getItem(KEY);
		if (!raw) return [];
		const parsed: unknown = JSON.parse(raw);
		if (!Array.isArray(parsed)) return [];
		// 스키마가 바뀐 예전 값이 섞여도 앱이 죽지 않게 필드 단위 방어.
		return parsed
			.filter(
				(x): x is RecentDoc =>
					!!x &&
					typeof x === 'object' &&
					typeof (x as RecentDoc).href === 'string' &&
					typeof (x as RecentDoc).label === 'string'
			)
			.slice(0, MAX_RECENT_DOCS);
	} catch {
		return [];
	}
}

export const recentDocs = writable<RecentDoc[]>(load());

function persist(list: RecentDoc[]) {
	if (typeof sessionStorage === 'undefined') return;
	try {
		sessionStorage.setItem(KEY, JSON.stringify(list));
	} catch {
		/* 용량 초과 등 — 최근 목록은 보조 기능이라 조용히 무시 */
	}
}

/**
 * BUG-181: kind/label → 정규 href. SearchPalette 의 전역 인덱스(`all`)가 문서당
 * 하나의 정규 href 를 쓰는 것과 **반드시 문자열까지 일치**해야 recent 모드의
 * `order.has(i.href)` 매칭이 성립한다. 퀘스트 네비게이션(Board/List/Nav 등)이
 * `?from=board` 류 추적 쿼리를 붙이는데, 그 원본 URL 을 그대로 저장하면 인덱스의
 * 쿼리 없는 href 와 영영 안 맞아 "최근 본 문서"에서 퀘스트만 누락됐다.
 */
export function canonicalDocHref(kind: RecentKind, label: string): string {
	switch (kind) {
		case 'quest':
			return `/quests/${label}`;
		case 'campaign':
			return `/campaigns/${encodeURIComponent(label)}`;
		case 'rule':
			return `/rules?slug=${encodeURIComponent(label)}`;
		case 'book':
			return `/library?id=${encodeURIComponent(label)}`;
	}
}

/** 방문 기록. 같은 href 는 새로 쌓지 않고 맨 앞으로 끌어올린다(재방문). */
export function pushRecentDoc(doc: Omit<RecentDoc, 'ts'>): void {
	if (!doc.href) return;
	recentDocs.update((list) => {
		const prev = list.find((d) => d.href === doc.href);
		const merged: RecentDoc = {
			...doc,
			// 예전 세션이 저장해둔 제목은 인덱스 조회 실패 시 폴백용으로만 보존.
			title: doc.title || prev?.title,
			ts: Date.now()
		};
		const next = [merged, ...list.filter((d) => d.href !== doc.href)].slice(0, MAX_RECENT_DOCS);
		persist(next);
		return next;
	});
}

/**
 * BUG-159: 표시용 제목 — cross-link 인덱스(kind:ID 네임스페이스)에서 조회.
 * 없으면 예전 세션 값 → 그것도 없으면 빈 문자열(라벨만 표시).
 *
 * 인덱스는 `[[..]]` 렌더/자동완성이 쓰는 것과 같은 캐시라, 목록을 여는
 * 시점에 이미 적재돼 있는 경우가 대부분이고 reindex 시 자동 갱신된다.
 */
export function recentDocTitle(d: RecentDoc, ns: Map<string, IndexedRef>): string {
	const ref = ns.get(`${d.kind}:${d.label.toUpperCase()}`);
	return ref?.title || d.title || '';
}

/** 표시용 제목(스토어 직접 조회 — 컴포넌트 밖에서 쓸 때). */
export function recentDocTitleNow(d: RecentDoc): string {
	return recentDocTitle(d, get(questIndexNs));
}

/** 삭제된 문서 등으로 더 이상 유효하지 않은 항목 제거 — 목록에서 직접 X. */
export function removeRecentDoc(href: string): void {
	recentDocs.update((list) => {
		const next = list.filter((d) => d.href !== href);
		persist(next);
		return next;
	});
}

export function clearRecentDocs(): void {
	recentDocs.set([]);
	persist([]);
}

/**
 * 라우트 경로 → 추적 대상이면 {kind, label}, 아니면 null.
 *
 * 규칙/도서관은 쿼리스트링으로 대상을 고르는 구조(`/rules?slug=`,
 * `/library?id=`)라 경로만으론 판별이 안 된다 — 쿼리까지 포함한 href 를
 * 받아 파싱한다.
 */
export function classifyDocRoute(href: string): { kind: RecentKind; label: string } | null {
	// 상대 경로만 다루므로 base 는 형식상 필요.
	let url: URL;
	try {
		url = new URL(href, 'http://x');
	} catch {
		return null;
	}
	const path = url.pathname;
	const seg = path.split('/').filter(Boolean);
	if (seg[0] === 'quests' && seg[1]) {
		return { kind: 'quest', label: decodeURIComponent(seg[1]) };
	}
	if (seg[0] === 'campaigns' && seg[1] && seg[1] !== 'new') {
		return { kind: 'campaign', label: decodeURIComponent(seg[1]) };
	}
	if (seg[0] === 'rules') {
		const slug = url.searchParams.get('slug');
		return slug ? { kind: 'rule', label: slug } : null;
	}
	if (seg[0] === 'library') {
		const id = url.searchParams.get('id');
		return id ? { kind: 'book', label: id } : null;
	}
	return null;
}

/** 현재 목록 스냅샷 — 테스트/디버그용. */
export function snapshotRecentDocs(): RecentDoc[] {
	return get(recentDocs);
}
