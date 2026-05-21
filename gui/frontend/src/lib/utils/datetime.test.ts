import { describe, it, expect } from 'vitest';
import { formatTs, formatRelative } from './datetime';

describe('formatTs', () => {
	it('SQLite 형식 (공백 구분자) 을 YYYY-MM-DD HH:mm 으로 변환', () => {
		// 로컬 시간대 해석이므로 month/day 만 검증 (시간은 TZ 영향).
		const out = formatTs('2026-05-15 16:13:09');
		expect(out).toMatch(/^2026-05-15 \d{2}:\d{2}$/);
	});

	it('ISO Z 형식도 처리', () => {
		const out = formatTs('2026-05-15T16:13:09Z');
		// UTC 16:13 → 로컬 TZ 따라 5/15 또는 5/16 으로 흔들림 (KST 는 +9 → 5/16 01:13).
		expect(out).toMatch(/^2026-05-1[456] \d{2}:\d{2}$/);
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
	const now = new Date('2026-05-20T12:00:00');

	it('< 60s → "방금"', () => {
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
		expect(out).toMatch(/^2026-05-01 \d{2}:\d{2}$/);
	});

	it('미래 시각 → 절대값 (음수 분 표시 안 함)', () => {
		const ts = '2026-05-21 12:00:00';
		const out = formatRelative(ts, now);
		expect(out).toMatch(/^2026-05-21/);
	});

	it('빈 / null → 빈 문자열', () => {
		expect(formatRelative(null)).toBe('');
		expect(formatRelative(undefined)).toBe('');
		expect(formatRelative('')).toBe('');
	});
});
