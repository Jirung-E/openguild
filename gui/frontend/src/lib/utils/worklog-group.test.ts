import { describe, it, expect } from 'vitest';
import type { ActivityRow } from '$lib/api/worklog';
import {
	activityHref,
	groupByDay,
	groupByDoc,
	groupTimeLabel,
	firstLine
} from './worklog-group';

function row(ts: string, slug: string, kind = 'status', extra: Partial<ActivityRow> = {}): ActivityRow {
	return { ts, kind, slug, summary: `${kind} on ${slug}`, ...extra } as ActivityRow;
}

describe('groupByDoc — REQ-006 compact 묶음', () => {
	it('같은 문서의 조작을 하나로 묶는다', () => {
		const groups = groupByDoc([
			row('2026-08-23T01:13:22+09:00', 'DEV-365'),
			row('2026-08-23T01:20:14+09:00', 'DEV-365'),
			row('2026-08-23T01:27:03+09:00', 'BUG-242', 'created')
		]);
		expect(groups.map((g) => g.slug)).toEqual(['DEV-365', 'BUG-242']);
		expect(groups[0].rows).toHaveLength(2);
		expect(groups[1].rows).toHaveLength(1);
	});

	/**
	 * 핵심 계약: 시간 흐름이 뒤집히면 안 된다. 건수순으로 정렬하면 늦게 시작한
	 * 문서가 위로 올라와 그날의 진행 순서를 읽을 수 없게 된다.
	 */
	it('문서의 첫 등장 순서를 유지한다 — 건수순이 아니다', () => {
		const groups = groupByDoc([
			row('2026-08-23T01:00:00+09:00', 'A'), // 1건이지만 먼저 등장
			row('2026-08-23T02:00:00+09:00', 'B'),
			row('2026-08-23T03:00:00+09:00', 'B'),
			row('2026-08-23T04:00:00+09:00', 'B')
		]);
		expect(groups.map((g) => g.slug)).toEqual(['A', 'B']);
	});

	it('중간에 다른 문서가 끼어도 원래 그룹으로 되돌아간다', () => {
		const groups = groupByDoc([
			row('2026-08-23T01:00:00+09:00', 'A'),
			row('2026-08-23T02:00:00+09:00', 'B'),
			row('2026-08-23T03:00:00+09:00', 'A')
		]);
		expect(groups.map((g) => g.slug)).toEqual(['A', 'B']);
		expect(groups[0].rows).toHaveLength(2);
	});

	it('조작 종류를 등장 순으로 중복 없이 모은다', () => {
		const groups = groupByDoc([
			row('2026-08-23T01:00:00+09:00', 'A', 'created'),
			row('2026-08-23T02:00:00+09:00', 'A', 'status'),
			row('2026-08-23T03:00:00+09:00', 'A', 'status'),
			row('2026-08-23T04:00:00+09:00', 'A', 'comment')
		]);
		expect(groups[0].kinds).toEqual(['created', 'status', 'comment']);
	});

	it('fromTs / toTs 는 순서가 뒤섞여 들어와도 최소·최대를 잡는다', () => {
		const groups = groupByDoc([
			row('2026-08-23T05:00:00+09:00', 'A'),
			row('2026-08-23T01:00:00+09:00', 'A'),
			row('2026-08-23T09:00:00+09:00', 'A')
		]);
		expect(groups[0].fromTs).toBe('2026-08-23T01:00:00+09:00');
		expect(groups[0].toTs).toBe('2026-08-23T09:00:00+09:00');
	});

	/** 그룹 링크는 첫 활동을 가리킨다 — 댓글이면 그 댓글까지. */
	it('href 는 첫 활동 기준이다', () => {
		const groups = groupByDoc([
			row('2026-08-23T01:00:00+09:00', 'DEV-001', 'comment', { ref_id: 3 }),
			row('2026-08-23T02:00:00+09:00', 'DEV-001', 'comment', { ref_id: 7 })
		]);
		expect(groups[0].href).toBe('/quests/DEV-001?comment=3');
	});

	it('빈 입력은 빈 배열', () => {
		expect(groupByDoc([])).toEqual([]);
	});
});

describe('groupTimeLabel', () => {
	function group(from: string, to: string, n = 2): Parameters<typeof groupTimeLabel>[0] {
		return { slug: 'A', href: '', rows: new Array(n).fill(row(from, 'A')), kinds: [], fromTs: from, toTs: to };
	}

	it('단일 시각은 그대로', () => {
		expect(groupTimeLabel(group('2026-08-23T01:22:00+09:00', '2026-08-23T01:22:00+09:00', 1))).toBe('01:22');
	});

	it('다른 시각이면 범위로', () => {
		expect(groupTimeLabel(group('2026-08-23T01:22:00+09:00', '2026-08-23T01:26:00+09:00'))).toBe('01:22–01:26');
	});

	/**
	 * 회귀: 예전엔 `rows.length > 1` 로 판단해 같은 분에 두 번 조작한 문서가
	 * `00:01–00:01` 로 나왔다(실 데이터 131건). 초가 달라도 분이 같으면
	 * 표기가 같으므로 범위로 쓰지 않는다.
	 */
	it('건수가 여럿이어도 분이 같으면 범위로 쓰지 않는다', () => {
		expect(groupTimeLabel(group('2026-08-23T00:01:05+09:00', '2026-08-23T00:01:47+09:00', 3))).toBe('00:01');
	});
});

describe('activityHref', () => {
	it('퀘스트 / 캠페인을 구분한다', () => {
		expect(activityHref(row('t', 'DEV-001'))).toBe('/quests/DEV-001');
		expect(activityHref(row('t', 'C-007'))).toBe('/campaigns/C-007');
	});

	it('규칙 / 도서관은 전용 페이지로', () => {
		expect(activityHref(row('t', 'restore-behavior', 'rule'))).toBe('/rules?slug=restore-behavior');
		expect(activityHref(row('t', 'BOOK-001', 'book'))).toBe('/library?id=BOOK-001');
	});

	it('댓글 활동은 그 댓글까지 딥링크 (DEV-296)', () => {
		expect(activityHref(row('t', 'DEV-001', 'comment', { ref_id: 12 }))).toBe('/quests/DEV-001?comment=12');
	});

	it('slug 를 URL 인코딩한다', () => {
		expect(activityHref(row('t', 'a b/c', 'rule'))).toBe('/rules?slug=a%20b%2Fc');
	});
});

describe('groupByDay', () => {
	it('날짜가 바뀌는 지점에서 나눈다', () => {
		const days = groupByDay([
			row('2026-08-22T23:00:00+09:00', 'A'),
			row('2026-08-23T01:00:00+09:00', 'B'),
			row('2026-08-23T02:00:00+09:00', 'C')
		]);
		expect(days.map((d) => d.date)).toEqual(['2026-08-22', '2026-08-23']);
		expect(days[1].rows).toHaveLength(2);
	});
});

describe('firstLine', () => {
	it('여러 줄 요약은 첫 줄만', () => {
		expect(firstLine('제목\n본문\n더')).toBe('제목');
	});
	it('한 줄은 그대로', () => {
		expect(firstLine('그대로')).toBe('그대로');
	});
});
