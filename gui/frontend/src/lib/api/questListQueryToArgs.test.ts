/**
 * BUG-211: Tauri 경로의 `list_quests` args 타입 복원.
 *
 * HTTP 경로는 `Query<ListQuery>`(serde_urlencoded)가 `"true"` → bool,
 * `"10"` → i64 로 강제 변환해주지만, Tauri 경로는 args 객체가 JSON 으로
 * 역직렬화되므로 문자열이 그대로 가면 실패한다:
 *   invalid type: string "true", expected a boolean
 */
import { describe, it, expect } from 'vitest';
import { questListQueryToArgs } from './transport';

const args = (qs: string) => questListQueryToArgs(new URLSearchParams(qs));

describe('questListQueryToArgs', () => {
	it('bool 필드를 실제 boolean 으로 바꾼다', () => {
		expect(args('slim=true').slim).toBe(true);
		expect(args('title_only=true').title_only).toBe(true);
		expect(args('reverse=true&no_parent=true')).toEqual({ reverse: true, no_parent: true });
	});

	it('serde_urlencoded 가 받아주는 다른 표기도 동일하게 처리', () => {
		expect(args('slim=1').slim).toBe(true);
		expect(args('slim=TRUE').slim).toBe(true);
		expect(args('slim=false').slim).toBe(false);
		expect(args('slim=0').slim).toBe(false);
	});

	it('정수 필드를 number 로 바꾼다', () => {
		expect(args('limit=10&offset=20')).toEqual({ limit: 10, offset: 20 });
	});

	it('문자열 필드는 건드리지 않는다', () => {
		expect(args('sort=updated&type=DEV,BUG&search=%ED%95%9C%EA%B8%80')).toEqual({
			sort: 'updated',
			type: 'DEV,BUG',
			search: '한글'
		});
	});

	it('빈 쿼리는 빈 객체 — 호출부가 args 자체를 생략할 수 있게', () => {
		expect(args('')).toEqual({});
	});

	it('실제 호출 조합 — slim 목록 + 정렬', () => {
		expect(args('sort=updated&slim=true')).toEqual({ sort: 'updated', slim: true });
	});
});
