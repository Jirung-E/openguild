// BUG-259: 작업기록의 펼침·기간 상태가 페이지를 옮기면 사라지던 문제.
//
// 저장이 새면 증상이 그대로 돌아온다. 그리고 **깨진 값에 던지면 안 된다** —
// 표시 상태 하나 때문에 페이지 전체가 안 뜨면 원래 버그보다 나쁘다.
import { describe, it, expect, beforeEach } from 'vitest';
import {
	loadWorklogUnit,
	saveWorklogUnit,
	loadExpandedDocs,
	saveExpandedDocs,
	worklogDocKey,
	DEFAULT_UNIT,
	EXPANDED_MAX
} from './worklogView';

describe('worklogView — 기간 단위', () => {
	beforeEach(() => {
		sessionStorage.clear();
	});

	it('저장된 것이 없으면 기본값(일)', () => {
		expect(loadWorklogUnit()).toBe(DEFAULT_UNIT);
	});

	it('저장한 값을 그대로 돌려준다', () => {
		saveWorklogUnit('week');
		expect(loadWorklogUnit()).toBe('week');
		saveWorklogUnit('range');
		expect(loadWorklogUnit()).toBe('range');
	});

	it('모르는 값이면 기본값 — 손으로 고쳤거나 예전 버전이 남긴 값일 수 있다', () => {
		sessionStorage.setItem('openguild.worklogUnit', 'fortnight');
		expect(loadWorklogUnit()).toBe(DEFAULT_UNIT);
	});
});

describe('worklogView — 펼친 그룹', () => {
	beforeEach(() => {
		sessionStorage.clear();
	});

	it('저장된 것이 없으면 빈 Set', () => {
		expect(loadExpandedDocs().size).toBe(0);
	});

	it('왕복한다 — 이게 깨지면 버그가 그대로 돌아온다', () => {
		saveExpandedDocs(new Set(['2026-08-30|DEV-001', '2026-08-30|DEV-003']));
		const back = loadExpandedDocs();
		expect(back.has('2026-08-30|DEV-001')).toBe(true);
		expect(back.has('2026-08-30|DEV-003')).toBe(true);
		expect(back.size).toBe(2);
	});

	it('빈 Set 을 저장하면 빈 Set 으로 돌아온다 — 전부 접은 상태도 상태다', () => {
		saveExpandedDocs(new Set(['a|b']));
		saveExpandedDocs(new Set());
		expect(loadExpandedDocs().size).toBe(0);
	});

	it('JSON 이 깨져 있어도 던지지 않는다', () => {
		sessionStorage.setItem('openguild.worklogExpanded', '{짤림');
		expect(() => loadExpandedDocs()).not.toThrow();
		expect(loadExpandedDocs().size).toBe(0);
	});

	it('배열이 아니면 빈 Set', () => {
		sessionStorage.setItem('openguild.worklogExpanded', '{"a":1}');
		expect(loadExpandedDocs().size).toBe(0);
	});

	it('배열 안의 문자열 아닌 것은 걸러낸다', () => {
		sessionStorage.setItem('openguild.worklogExpanded', '["ok",1,null,{"x":1},"ok2"]');
		const back = loadExpandedDocs();
		expect([...back]).toEqual(['ok', 'ok2']);
	});

	it('상한을 넘으면 오래된 쪽부터 버린다 — 날짜를 옮겨 다니면 무한히 쌓인다', () => {
		const keys = Array.from({ length: EXPANDED_MAX + 50 }, (_, i) => `d|${i}`);
		saveExpandedDocs(new Set(keys));
		const back = loadExpandedDocs();
		expect(back.size).toBe(EXPANDED_MAX);
		// 최근 것은 남고 가장 오래된 것은 없다.
		expect(back.has(`d|${EXPANDED_MAX + 49}`)).toBe(true);
		expect(back.has('d|0')).toBe(false);
	});
});

describe('worklogDocKey', () => {
	it('날짜가 다르면 같은 문서라도 다른 키다', () => {
		expect(worklogDocKey('2026-08-30', 'DEV-001')).not.toBe(
			worklogDocKey('2026-08-29', 'DEV-001')
		);
	});
});
