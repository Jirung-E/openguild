import { describe, it, expect } from 'vitest';
import { normalizeRemoteUrl } from './remoteServer';

// BUG-098(사용자 보고: "127.0.0.1:3000 만 입력하면 '서버가 응답했지만
// 예상한 형식이 아니다'") — 스킴 누락 입력이 fetch() 에서 상대 경로로
// 오인되던 회귀 방지.
describe('normalizeRemoteUrl', () => {
	it('스킴 없으면 http:// 를 붙인다', () => {
		expect(normalizeRemoteUrl('127.0.0.1:3000')).toBe('http://127.0.0.1:3000');
	});

	it('이미 http:// 가 있으면 그대로(중복 방지)', () => {
		expect(normalizeRemoteUrl('http://127.0.0.1:3000')).toBe('http://127.0.0.1:3000');
	});

	it('https:// 도 보존', () => {
		expect(normalizeRemoteUrl('https://example.com')).toBe('https://example.com');
	});

	it('trailing slash 제거', () => {
		expect(normalizeRemoteUrl('http://127.0.0.1:3000/')).toBe('http://127.0.0.1:3000');
		expect(normalizeRemoteUrl('127.0.0.1:3000///')).toBe('http://127.0.0.1:3000');
	});

	it('앞뒤 공백 정리', () => {
		expect(normalizeRemoteUrl('  192.168.1.10:3000  ')).toBe('http://192.168.1.10:3000');
	});

	it('빈 문자열은 빈 문자열 그대로', () => {
		expect(normalizeRemoteUrl('   ')).toBe('');
	});
});
