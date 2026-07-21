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

export type RecentKind = 'quest' | 'campaign' | 'rule' | 'book';

export interface RecentDoc {
	/** 라우트 경로 — 클릭 시 이동 대상이자 중복 판정 키. */
	href: string;
	kind: RecentKind;
	/** 식별자 — DEV-001 / C-001 / 규칙 slug / BOOK-001. */
	label: string;
	/** 제목 — 로드 전이면 빈 문자열(라벨만 표시). */
	title: string;
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
 * 방문 기록. 같은 href 는 새로 쌓지 않고 맨 앞으로 끌어올린다(재방문).
 * `title` 이 나중에 로드되는 경우가 많아, 기존 항목의 제목은 비어있지 않은
 * 새 값이 올 때만 덮어쓴다(빈 값으로 지워지지 않게).
 */
export function pushRecentDoc(doc: Omit<RecentDoc, 'ts'>): void {
	if (!doc.href) return;
	recentDocs.update((list) => {
		const prev = list.find((d) => d.href === doc.href);
		const merged: RecentDoc = {
			...doc,
			title: doc.title || prev?.title || '',
			ts: Date.now()
		};
		const next = [merged, ...list.filter((d) => d.href !== doc.href)].slice(0, MAX_RECENT_DOCS);
		persist(next);
		return next;
	});
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
