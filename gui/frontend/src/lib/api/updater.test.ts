// BUG-045: humanizeUpdaterError 의 케이스 분류.

import { describe, it, expect } from 'vitest';
import { humanizeUpdaterError } from './updater';

describe('humanizeUpdaterError', () => {
	it('404 → endpoint / release 안내', () => {
		expect(humanizeUpdaterError('Request failed with status 404')).toMatch(/릴리즈가 아직 없거나/);
	});
	it('not found 도 같은 안내', () => {
		expect(humanizeUpdaterError('endpoint not found')).toMatch(/릴리즈가 아직 없거나/);
	});
	it('network 키워드 → 네트워크 안내', () => {
		expect(humanizeUpdaterError('network error')).toMatch(/네트워크/);
		expect(humanizeUpdaterError('failed to fetch')).toMatch(/네트워크/);
		expect(humanizeUpdaterError('connection timed out')).toMatch(/네트워크/);
		expect(humanizeUpdaterError('DNS resolution failed')).toMatch(/네트워크/);
	});
	it('signature 키워드 → 서명 검증 실패', () => {
		expect(humanizeUpdaterError('Signature verification failed')).toMatch(/서명 검증/);
		expect(humanizeUpdaterError('invalid pubkey')).toMatch(/서명 검증/);
	});
	it('dev / debug → 개발 빌드 안내', () => {
		expect(humanizeUpdaterError('dev mode update not supported')).toMatch(/개발 빌드/);
	});
	it('매치 안 되는 raw → 기본 prefix + 원본', () => {
		const out = humanizeUpdaterError('mystery internal panic');
		expect(out).toMatch(/업데이트 확인 실패/);
		expect(out).toContain('mystery internal panic');
	});
});
