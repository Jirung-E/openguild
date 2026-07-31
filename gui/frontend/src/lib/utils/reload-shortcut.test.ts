// BUG-179: 새로고침 단축키 판정.
//
// 데스크탑에서는 이 판정이 미저장 경고의 유일한 트리거다(beforeunload 는
// BUG-075 때문에 못 쓴다) — 하나라도 새면 편집 내용이 그대로 날아간다.

import { describe, it, expect } from 'vitest';
import { isReloadShortcut } from './reload-shortcut';

describe('isReloadShortcut', () => {
	it('새로고침 단축키를 잡는다', () => {
		expect(isReloadShortcut({ key: 'F5' })).toBe(true);
		expect(isReloadShortcut({ key: 'r', ctrlKey: true })).toBe(true);
		expect(isReloadShortcut({ key: 'r', metaKey: true })).toBe(true); // macOS
		// 강제 새로고침 — Shift 가 붙어도 내용이 날아가는 건 같다.
		expect(isReloadShortcut({ key: 'R', ctrlKey: true })).toBe(true);
	});

	it('그 밖의 입력은 잡지 않는다', () => {
		expect(isReloadShortcut({ key: 'r' })).toBe(false); // 그냥 타이핑
		expect(isReloadShortcut({ key: 's', ctrlKey: true })).toBe(false); // 저장
		expect(isReloadShortcut({ key: 'F6' })).toBe(false);
		expect(isReloadShortcut({ key: 'Enter', ctrlKey: true })).toBe(false);
	});

	it('Alt 조합은 새로고침이 아니다 (창 관리 단축키)', () => {
		expect(isReloadShortcut({ key: 'F5', altKey: true })).toBe(false);
		expect(isReloadShortcut({ key: 'r', ctrlKey: true, altKey: true })).toBe(false);
	});
});
