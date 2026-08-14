import { describe, expect, it } from 'vitest';
import { isSaveShortcut } from './save-shortcut';

describe('isSaveShortcut', () => {
	it('accepts Ctrl+S and Cmd+S regardless of key casing', () => {
		expect(
			isSaveShortcut({ key: 's', ctrlKey: true, metaKey: false, shiftKey: false, altKey: false })
		).toBe(true);
		expect(
			isSaveShortcut({ key: 'S', ctrlKey: false, metaKey: true, shiftKey: false, altKey: false })
		).toBe(true);
	});

	it('rejects plain or modified S shortcuts', () => {
		expect(
			isSaveShortcut({ key: 's', ctrlKey: false, metaKey: false, shiftKey: false, altKey: false })
		).toBe(false);
		expect(
			isSaveShortcut({ key: 's', ctrlKey: true, metaKey: false, shiftKey: true, altKey: false })
		).toBe(false);
		expect(
			isSaveShortcut({ key: 's', ctrlKey: true, metaKey: false, shiftKey: false, altKey: true })
		).toBe(false);
	});
});
