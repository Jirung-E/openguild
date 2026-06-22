import { describe, it, expect } from 'vitest';
import { clampToStep, valueFromTrackPx, pixelsPerUnit } from './slider';

describe('clampToStep', () => {
	it('범위 내 값은 step 격자에 스냅', () => {
		expect(clampToStep(0.13, 0, 1, 0.05)).toBeCloseTo(0.15);
		expect(clampToStep(0.12, 0, 1, 0.05)).toBeCloseTo(0.1);
	});

	it('min 미만 → min', () => {
		expect(clampToStep(-5, 0, 1, 0.1)).toBe(0);
	});

	it('max 초과 → max', () => {
		expect(clampToStep(99, 0, 1, 0.1)).toBe(1);
	});

	it('step 의 소수 자릿수에 맞춰 부동소수 정리', () => {
		// 0.1 + 0.2 = 0.30000000000000004 인데 step 0.1 → 깔끔.
		const v = clampToStep(0.1 + 0.2, 0, 1, 0.1);
		expect(v).toBe(0.3);
		expect(v.toString()).toBe('0.3');
	});

	it('step 1 정수 격자', () => {
		expect(clampToStep(5.7, 0, 10, 1)).toBe(6);
		expect(clampToStep(5.4, 0, 10, 1)).toBe(5);
	});

	it('비균등 min (예: 50~200%)', () => {
		expect(clampToStep(127, 50, 200, 1)).toBe(127);
		expect(clampToStep(127.6, 50, 200, 1)).toBe(128);
	});

	it('step 0.01 정밀 (DEV-101 fix4 UI scale 1% 단위)', () => {
		expect(clampToStep(1.234, 0.5, 2.0, 0.01)).toBeCloseTo(1.23);
		expect(clampToStep(1.235, 0.5, 2.0, 0.01)).toBeCloseTo(1.24);
	});
});

describe('valueFromTrackPx', () => {
	it('트랙 좌측 = min', () => {
		expect(valueFromTrackPx(0, 200, 0, 1, 0.1)).toBe(0);
	});

	it('트랙 우측 = max', () => {
		expect(valueFromTrackPx(200, 200, 0, 1, 0.1)).toBe(1);
	});

	it('중앙 = 중간값', () => {
		expect(valueFromTrackPx(100, 200, 0, 1, 0.1)).toBeCloseTo(0.5);
	});

	it('트랙 밖 좌측 → min clamp', () => {
		expect(valueFromTrackPx(-50, 200, 0, 1, 0.1)).toBe(0);
	});

	it('트랙 밖 우측 → max clamp', () => {
		expect(valueFromTrackPx(500, 200, 0, 1, 0.1)).toBe(1);
	});

	it('trackWidth 0 → min (방어)', () => {
		expect(valueFromTrackPx(100, 0, 5, 10, 1)).toBe(5);
	});
});

describe('pixelsPerUnit', () => {
	it('트랙 200px / range 1 → 200', () => {
		expect(pixelsPerUnit(200, 0, 1)).toBe(200);
	});

	it('트랙 300px / range 100 (50~150) → 3', () => {
		expect(pixelsPerUnit(300, 50, 150)).toBe(3);
	});

	it('범위 0 → fallback 1 (방어)', () => {
		expect(pixelsPerUnit(200, 5, 5)).toBe(1);
	});
});
