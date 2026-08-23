/**
 * REQ-006: 작업 기록 compact 뷰의 묶음 로직.
 *
 * 원래 `routes/worklog/+page.svelte` 안에 있었다. 컴포넌트 안에 있으면 단위
 * 테스트를 붙일 수 없어 — 첫 등장 순서 유지·시각 범위 표기·뱃지 중복 제거가
 * 전부 육안 확인에만 의존했다 — 여기로 분리했다. 페이지는 이 함수들을 쓴다.
 */
import type { ActivityRow } from '$lib/api/worklog';

/** 하루치 활동을 문서 단위로 묶은 결과. */
export interface DocGroup {
	slug: string;
	/** 그룹 링크 — 그 문서의 **첫** 활동을 가리킨다(댓글이면 그 댓글까지). */
	href: string;
	rows: ActivityRow[];
	/** 그룹 안 조작 종류(중복 제거, 등장 순) — 뱃지로 요약 표시. */
	kinds: string[];
	/** 가장 이른 / 늦은 시각 (ISO 전체). 표기는 `groupTimeLabel` 이 만든다. */
	fromTs: string;
	toTs: string;
}

/** 활동 한 건이 가리키는 문서 경로. */
export function activityHref(a: ActivityRow): string {
	// DEV-288: 규칙/도서관 활동은 해당 문서 페이지로(딥링크 쿼리는 각 페이지가 처리).
	if (a.kind === 'rule') return `/rules?slug=${encodeURIComponent(a.slug)}`;
	if (a.kind === 'book') return `/library?id=${encodeURIComponent(a.slug)}`;
	const base = /^C-\d+$/.test(a.slug)
		? `/campaigns/${encodeURIComponent(a.slug)}`
		: `/quests/${encodeURIComponent(a.slug)}`;
	// DEV-296: 댓글/토론 활동은 그 댓글까지 — 문서만 열면 긴 목록에서 다시
	// 찾아야 했다. 댓글 섹션이 `?comment=N` 을 보고 스크롤 + 강조한다.
	return a.ref_id != null ? `${base}?comment=${a.ref_id}` : base;
}

/** 주/월 뷰용 날짜별 그룹핑. 서버가 시간순으로 주므로 연속 구간만 묶는다. */
export function groupByDay(activities: ActivityRow[]): { date: string; rows: ActivityRow[] }[] {
	const out: { date: string; rows: ActivityRow[] }[] = [];
	for (const a of activities) {
		const d = a.ts.slice(0, 10);
		const last = out[out.length - 1];
		if (last && last.date === d) last.rows.push(a);
		else out.push({ date: d, rows: [a] });
	}
	return out;
}

/** 하루치 rows 를 문서(slug) 단위로 묶는다. 문서의 첫 등장 순서를 유지한다. */
export function groupByDoc(rows: ActivityRow[]): DocGroup[] {
	const byslug = new Map<string, DocGroup>();
	for (const a of rows) {
		let g = byslug.get(a.slug);
		if (!g) {
			g = { slug: a.slug, href: activityHref(a), rows: [], kinds: [], fromTs: a.ts, toTs: a.ts };
			byslug.set(a.slug, g);
		}
		g.rows.push(a);
		if (!g.kinds.includes(a.kind)) g.kinds.push(a.kind);
		if (a.ts < g.fromTs) g.fromTs = a.ts;
		if (a.ts > g.toTs) g.toTs = a.ts;
	}
	return [...byslug.values()];
}

/**
 * 그룹 헤더의 시각 표기 — `01:22` 또는 범위면 `01:22–01:26`.
 *
 * 예전엔 `rows.length > 1` 로 범위 여부를 정해서, 같은 분에 두 번 조작한 문서가
 * `00:01–00:01` 로 나왔다(실 데이터에 131건). 건수가 아니라 **표시되는 값이
 * 실제로 다른지**로 판단한다.
 */
export function groupTimeLabel(g: DocGroup): string {
	const from = g.fromTs.slice(11, 16);
	const to = g.toTs.slice(11, 16);
	return from === to ? from : `${from}–${to}`;
}

/** 요약은 첫 줄만 — 여러 줄 본문이 행 높이를 밀어내지 않도록. */
export function firstLine(s: string): string {
	return s.split('\n', 1)[0] ?? '';
}
