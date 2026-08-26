import { describe, it, expect } from 'vitest';
import { Generation } from './latest-only';

describe('Generation', () => {
	it('가장 최근 토큰만 현재다', () => {
		const g = new Generation();
		const a = g.next();
		const b = g.next();
		expect(g.isCurrent(a)).toBe(false);
		expect(g.isCurrent(b)).toBe(true);
	});

	/**
	 * 핵심 시나리오: A 를 열고 응답 전에 B 로 이동. 늦게 온 A 응답이 화면을
	 * 덮으면 "나머지는 B 인데 이력만 A" 가 된다.
	 */
	it('늦게 도착한 이전 응답을 걸러낸다', () => {
		const g = new Generation();
		let shown = '';
		const load = (name: string) => {
			const mine = g.next();
			return (value: string) => {
				if (g.isCurrent(mine)) shown = value;
			};
		};
		const applyA = load('A'); // 먼저 시작
		const applyB = load('B'); // 나중에 시작 → 최신
		applyB('B의 이력');
		applyA('A의 이력'); // 늦게 도착 — 무시돼야 한다
		expect(shown).toBe('B의 이력');
	});

	it('cancel 후에는 아무 토큰도 현재가 아니다', () => {
		const g = new Generation();
		const t = g.next();
		g.cancel();
		expect(g.isCurrent(t)).toBe(false);
	});

	it('첫 세대 전에는 0 이 현재가 아니다', () => {
		expect(new Generation().isCurrent(0)).toBe(true); // 초기 #n = 0
		const g = new Generation();
		g.next();
		expect(g.isCurrent(0)).toBe(false);
	});
});
