// DEV-276: 최근 본 문서 — 라우트 분류 / 중복 승격 / 상한 / 제목 보강.

import { describe, it, expect, beforeEach, vi } from 'vitest';
import { get } from 'svelte/store';
import type { RecentDoc, RecentKind } from './recentDocs';

async function loadFresh() {
	vi.resetModules();
	return await import('./recentDocs');
}

describe('classifyDocRoute', () => {
	it('문서 라우트만 인식 — 퀘스트/캠페인 상세, 규칙, 도서관', async () => {
		const m = await loadFresh();
		expect(m.classifyDocRoute('/quests/DEV-001')).toEqual({ kind: 'quest', label: 'DEV-001' });
		expect(m.classifyDocRoute('/campaigns/C-001')).toEqual({ kind: 'campaign', label: 'C-001' });
		expect(m.classifyDocRoute('/rules?slug=release-process')).toEqual({
			kind: 'rule',
			label: 'release-process'
		});
		expect(m.classifyDocRoute('/library?id=BOOK-003')).toEqual({ kind: 'book', label: 'BOOK-003' });
	});

	it('탐색 화면은 제외 — 목록/보드/설정/관리 등', async () => {
		const m = await loadFresh();
		expect(m.classifyDocRoute('/')).toBeNull();
		expect(m.classifyDocRoute('/?view=board')).toBeNull();
		expect(m.classifyDocRoute('/settings')).toBeNull();
		expect(m.classifyDocRoute('/admin')).toBeNull();
		expect(m.classifyDocRoute('/worklog')).toBeNull();
		// 대상 미선택 상태의 규칙/도서관 목록도 제외(쿼리 없음).
		expect(m.classifyDocRoute('/rules')).toBeNull();
		expect(m.classifyDocRoute('/library')).toBeNull();
		// 캠페인 '새로 만들기' 는 문서가 아님.
		expect(m.classifyDocRoute('/campaigns/new')).toBeNull();
	});

	it('URL 인코딩된 식별자를 디코드 — 한글 규칙 slug 등', async () => {
		const m = await loadFresh();
		expect(m.classifyDocRoute(`/quests/${encodeURIComponent('DEV-001')}`)).toEqual({
			kind: 'quest',
			label: 'DEV-001'
		});
	});
});

describe('recentDocs store', () => {
	beforeEach(() => {
		sessionStorage.clear();
	});

	it('최신이 앞 — 방문 순서 역순으로 쌓임', async () => {
		const m = await loadFresh();
		m.pushRecentDoc({ href: '/quests/DEV-001', kind: 'quest', label: 'DEV-001', title: 'a' });
		m.pushRecentDoc({ href: '/quests/DEV-002', kind: 'quest', label: 'DEV-002', title: 'b' });
		expect(get(m.recentDocs).map((d) => d.label)).toEqual(['DEV-002', 'DEV-001']);
	});

	it('재방문은 새로 쌓지 않고 맨 앞으로 승격', async () => {
		const m = await loadFresh();
		m.pushRecentDoc({ href: '/quests/DEV-001', kind: 'quest', label: 'DEV-001', title: 'a' });
		m.pushRecentDoc({ href: '/quests/DEV-002', kind: 'quest', label: 'DEV-002', title: 'b' });
		m.pushRecentDoc({ href: '/quests/DEV-001', kind: 'quest', label: 'DEV-001', title: 'a' });
		const list = get(m.recentDocs);
		expect(list).toHaveLength(2);
		expect(list[0].label).toBe('DEV-001');
	});

	// BUG-159: 제목은 저장하지 않고 표시 시점에 cross-link 인덱스에서 조회.
	it('recentDocTitle — 인덱스의 제목을 kind:ID 로 정확히 조회', async () => {
		const m = await loadFresh();
		const ns = new Map([
			['quest:DEV-001', { title: '첫 번째 퀘스트', kind: 'quest' as const }],
			['book:BOOK-001', { title: '설계 결정 기록', kind: 'book' as const }],
			// 같은 번호라도 종류가 다르면 별개 — 네임스페이스 조회라 안 섞인다.
			['campaign:C-001', { title: '베타 0.4.0', kind: 'campaign' as const }]
		]);
		const doc = (kind: RecentKind, label: string): RecentDoc => ({
			href: `/x/${label}`,
			kind,
			label,
			ts: 0
		});
		expect(m.recentDocTitle(doc('quest', 'DEV-001'), ns)).toBe('첫 번째 퀘스트');
		expect(m.recentDocTitle(doc('book', 'BOOK-001'), ns)).toBe('설계 결정 기록');
		expect(m.recentDocTitle(doc('campaign', 'C-001'), ns)).toBe('베타 0.4.0');
	});

	it('recentDocTitle — 인덱스에 없으면 예전 세션 저장값 폴백, 그것도 없으면 빈 문자열', async () => {
		const m = await loadFresh();
		const empty = new Map();
		expect(
			m.recentDocTitle({ href: '/x', kind: 'quest', label: 'DEV-9', title: '옛 제목', ts: 0 }, empty)
		).toBe('옛 제목');
		expect(m.recentDocTitle({ href: '/x', kind: 'quest', label: 'DEV-9', ts: 0 }, empty)).toBe('');
	});

	it('recentDocTitle — 규칙 slug 는 대소문자 무시 조회', async () => {
		const m = await loadFresh();
		const ns = new Map([
			['rule:릴리즈-절차', { title: '릴리즈 패키지 절차', kind: 'rule' as const, slug: '릴리즈-절차' }]
		]);
		expect(
			m.recentDocTitle({ href: '/rules?slug=릴리즈-절차', kind: 'rule', label: '릴리즈-절차', ts: 0 }, ns)
		).toBe('릴리즈 패키지 절차');
	});

	it('상한 초과 시 오래된 항목부터 밀려남', async () => {
		const m = await loadFresh();
		for (let i = 1; i <= m.MAX_RECENT_DOCS + 3; i++) {
			m.pushRecentDoc({
				href: `/quests/DEV-${i}`,
				kind: 'quest',
				label: `DEV-${i}`,
				title: ''
			});
		}
		const list = get(m.recentDocs);
		expect(list).toHaveLength(m.MAX_RECENT_DOCS);
		// 가장 최근이 앞, 가장 오래된 3개는 사라짐.
		expect(list[0].label).toBe(`DEV-${m.MAX_RECENT_DOCS + 3}`);
		expect(list.some((d) => d.label === 'DEV-1')).toBe(false);
	});

	it('remove / clear', async () => {
		const m = await loadFresh();
		m.pushRecentDoc({ href: '/quests/DEV-001', kind: 'quest', label: 'DEV-001', title: '' });
		m.pushRecentDoc({ href: '/quests/DEV-002', kind: 'quest', label: 'DEV-002', title: '' });
		m.removeRecentDoc('/quests/DEV-001');
		expect(get(m.recentDocs).map((d) => d.label)).toEqual(['DEV-002']);
		m.clearRecentDocs();
		expect(get(m.recentDocs)).toEqual([]);
	});

	it('sessionStorage 로 복원 — 손상된 값은 무시하고 빈 목록', async () => {
		sessionStorage.setItem(
			'openguild.recentDocs',
			JSON.stringify([{ href: '/quests/DEV-009', kind: 'quest', label: 'DEV-009', title: 'x', ts: 1 }])
		);
		let m = await loadFresh();
		expect(get(m.recentDocs)[0].label).toBe('DEV-009');

		sessionStorage.setItem('openguild.recentDocs', '{not json');
		m = await loadFresh();
		expect(get(m.recentDocs)).toEqual([]);
	});
});
