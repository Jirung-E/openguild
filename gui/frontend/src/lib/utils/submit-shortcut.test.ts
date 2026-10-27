import { describe, expect, it } from 'vitest';
import { isSubmitShortcut, submitShortcutLabel } from './submit-shortcut';

const key = (o: Partial<KeyboardEvent>) =>
	({
		key: 'Enter',
		ctrlKey: false,
		metaKey: false,
		shiftKey: false,
		altKey: false,
		isComposing: false,
		keyCode: 13,
		...o
	}) as KeyboardEvent;

describe('isSubmitShortcut (DEV-424)', () => {
	it('Ctrl+Enter 와 ⌘Enter 는 보낸다', () => {
		expect(isSubmitShortcut(key({ ctrlKey: true }))).toBe(true);
		expect(isSubmitShortcut(key({ metaKey: true }))).toBe(true);
	});

	it('그냥 Enter · 다른 키 · Shift/Alt 를 섞은 것은 아니다', () => {
		expect(isSubmitShortcut(key({}))).toBe(false);
		expect(isSubmitShortcut(key({ key: 'a', ctrlKey: true }))).toBe(false);
		expect(isSubmitShortcut(key({ ctrlKey: true, shiftKey: true }))).toBe(false);
		expect(isSubmitShortcut(key({ ctrlKey: true, altKey: true }))).toBe(false);
	});

	it('한글 조합 중의 Enter 는 글자를 확정하는 키다 — 보내지 않는다', () => {
		expect(isSubmitShortcut(key({ ctrlKey: true, isComposing: true }))).toBe(false);
		expect(isSubmitShortcut(key({ ctrlKey: true, keyCode: 229 }))).toBe(false);
	});
});

describe('submitShortcutLabel', () => {
	it('맥은 ⌘Enter, 그 밖은 Ctrl+Enter', () => {
		expect(submitShortcutLabel('Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7)')).toBe('⌘Enter');
		expect(submitShortcutLabel('Mozilla/5.0 (Windows NT 10.0; Win64; x64)')).toBe('Ctrl+Enter');
	});
});
