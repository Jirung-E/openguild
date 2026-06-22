import { describe, it, expect } from 'vitest';
import { nextTabStopSpaces } from './editorSettings';

describe('DEV-130 nextTabStopSpaces (VSCode 식 탭 정지점)', () => {
	it('indentSize=4: 다음 4의 배수까지의 공백 수', () => {
		expect(nextTabStopSpaces('', 4)).toBe(4); // 열 0 → 4
		expect(nextTabStopSpaces('ab', 4)).toBe(2); // 열 2 → 4
		expect(nextTabStopSpaces('abc', 4)).toBe(1); // 열 3 → 4
		expect(nextTabStopSpaces('abcd', 4)).toBe(4); // 열 4 → 8 (가득)
		expect(nextTabStopSpaces('abcde', 4)).toBe(3); // 열 5 → 8
		expect(nextTabStopSpaces('abcdefgh', 4)).toBe(4); // 열 8 → 12
	});

	it('indentSize=2: 다음 2의 배수까지', () => {
		expect(nextTabStopSpaces('', 2)).toBe(2);
		expect(nextTabStopSpaces('a', 2)).toBe(1);
		expect(nextTabStopSpaces('ab', 2)).toBe(2);
	});

	it('앞에 탭 문자가 있으면 indentSize 폭으로 환산', () => {
		// '\t' → 열 4 (size=4). 다음 정지점까지 4.
		expect(nextTabStopSpaces('\t', 4)).toBe(4);
		// '\t' + 'a' → 열 5. 다음 정지점(8)까지 3.
		expect(nextTabStopSpaces('\ta', 4)).toBe(3);
	});
});
