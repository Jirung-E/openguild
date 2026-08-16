import { describe, it, expect } from 'vitest';
import { isPointerDrivenHover } from './hover-guard';

describe('isPointerDrivenHover', () => {
	it('좌표가 같은 hover 는 휠/레이아웃이 만든 것 — 무시한다', () => {
		const at = (x: number, y: number) => ({ clientX: x, clientY: y }) as MouseEvent;
		expect(isPointerDrivenHover(at(10, 20))).toBe(true); // 사람이 움직여 들어옴
		expect(isPointerDrivenHover(at(10, 20))).toBe(false); // 커서 그대로
		expect(isPointerDrivenHover(at(10, 21))).toBe(true); // 1px 이라도 움직이면 진짜
		expect(isPointerDrivenHover(at(10, 21))).toBe(false);
	});
});
