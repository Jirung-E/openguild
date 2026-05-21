import { describe, it, expect } from 'vitest';
import { formatTs, formatRelative } from './datetime';

describe('formatTs', () => {
	it('legacy SQLite 형식 (공백 + TZ 마커 없음) → UTC 가정', () => {
		// "no TZ marker" 규칙으로 UTC 해석.
		const out = formatTs('2026-05-15 16:13:09');
		// UTC 16:13 → KST(+9) 01:13 다음날, UTC(+0) 16:13 등 TZ 따라 흔들림.
		// 형식 자체는 항상 YYYY-MM-DD HH:mm.
		expect(out).toMatch(/^2026-05-1[456] \d{2}:\d{2}$/);
	});

	it('ISO Z 형식 (UTC 명시) 도 처리', () => {
		const out = formatTs('2026-05-15T16:13:09Z');
		expect(out).toMatch(/^2026-05-1[456] \d{2}:\d{2}$/);
	});

	it('DEV-041: ISO + TZ offset (Git 식) — JS Date 가 자동 변환', () => {
		// KST(+09:00) 13:13 = UTC 04:13 = 다른 로컬 TZ 의 다양한 시각.
		const out = formatTs('2026-05-15T13:13:09+09:00');
		// 결과 형식만 확인 (절대 시각은 동일하므로 TZ 따라 로컬 다름).
		expect(out).toMatch(/^2026-05-1[45] \d{2}:\d{2}$/);
	});

	it('빈 문자열 / null / undefined → 빈 문자열', () => {
		expect(formatTs('')).toBe('');
		expect(formatTs(null)).toBe('');
		expect(formatTs(undefined)).toBe('');
	});

	it('파싱 실패 시 원본 반환', () => {
		expect(formatTs('not-a-date')).toBe('not-a-date');
	});
});

describe('formatRelative', () => {
	// 모든 비교는 UTC 명시 (Z) 으로 둬서 환경 TZ 영향 제거.
	const now = new Date('2026-05-20T12:00:00Z');

	it('< 60s → "방금" (legacy ts 는 UTC 가정)', () => {
		const ts = '2026-05-20 11:59:30';
		expect(formatRelative(ts, now)).toBe('방금');
	});

	it('< 60분 → "X분 전"', () => {
		const ts = '2026-05-20 11:25:00';
		expect(formatRelative(ts, now)).toBe('35분 전');
	});

	it('< 24시간 → "X시간 전"', () => {
		const ts = '2026-05-20 08:00:00';
		expect(formatRelative(ts, now)).toBe('4시간 전');
	});

	it('< 7일 → "X일 전"', () => {
		const ts = '2026-05-18 12:00:00';
		expect(formatRelative(ts, now)).toBe('2일 전');
	});

	it('>= 7일 → 절대값', () => {
		const ts = '2026-05-01 12:00:00';
		const out = formatRelative(ts, now);
		// 같은 절대 시각 (UTC 12:00) — TZ 따라 로컬 시각 다름.
		expect(out).toMatch(/^2026-05-0[12] \d{2}:\d{2}$/);
	});

	it('미래 시각 → 절대값 (음수 분 표시 안 함)', () => {
		const ts = '2026-05-21 12:00:00';
		const out = formatRelative(ts, now);
		expect(out).toMatch(/^2026-05-2[12]/);
	});

	it('DEV-041: Git 식 (+offset) 도 정확히 비교 — 절대 시각 일치', () => {
		// UTC 11:25 = KST 20:25. 같은 절대 순간.
		const ts = '2026-05-20T20:25:00+09:00';
		expect(formatRelative(ts, now)).toBe('35분 전');
	});

	it('빈 / null → 빈 문자열', () => {
		expect(formatRelative(null)).toBe('');
		expect(formatRelative(undefined)).toBe('');
		expect(formatRelative('')).toBe('');
	});
});
