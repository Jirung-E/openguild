/**
 * BUG-176: 히스토리 항목의 길드 표식 비교.
 *
 * 이 판정이 틀리면 둘 중 하나가 난다:
 *  - 너무 헐거우면 → 다른 길드 화면을 그대로 보여준다(원래 버그).
 *  - 너무 빡빡하면 → 같은 길드인데도 웰컴으로 튕겨 정상 이동이 막힌다.
 * 그래서 `sameGuild` 의 경계를 고정해 둔다.
 */
import { describe, it, expect, vi, beforeEach } from 'vitest';

vi.mock('$lib/api/transport', () => ({ detectEnvironment: () => detectEnv() }));
vi.mock('$lib/stores/remoteServer', () => ({
	getRemoteServerUrl: () => remoteUrl,
	isRemoteSessionActive: () => remoteActive
}));

let detectEnv = () => 'tauri' as string;
let remoteUrl: string | null = null;
let remoteActive = false;

beforeEach(() => {
	detectEnv = () => 'tauri';
	remoteUrl = null;
	remoteActive = false;
	vi.resetModules();
});

describe('BUG-176 sameGuild', () => {
	it('같은 로컬 경로면 같은 길드', async () => {
		const m = await import('./guildIdentity');
		expect(m.sameGuild({ kind: 'local', path: 'C:/a' }, { kind: 'local', path: 'C:/a' })).toBe(true);
	});

	it('다른 로컬 경로면 다른 길드 — 이 경우가 원래 버그(A 항목에 B 가 그려짐)', async () => {
		const m = await import('./guildIdentity');
		expect(m.sameGuild({ kind: 'local', path: 'C:/a' }, { kind: 'local', path: 'C:/b' })).toBe(false);
	});

	it('같은 원격 URL 이면 같은 길드', async () => {
		const m = await import('./guildIdentity');
		expect(m.sameGuild({ kind: 'remote', url: 'http://h:3000' }, { kind: 'remote', url: 'http://h:3000' })).toBe(true);
	});

	it('로컬과 원격은 경로/URL 이 비슷해도 항상 다른 길드', async () => {
		const m = await import('./guildIdentity');
		expect(m.sameGuild({ kind: 'local', path: 'x' }, { kind: 'remote', url: 'x' })).toBe(false);
	});

	it('한쪽이 없으면 "같다"고 하지 않는다 — 표식 없는 항목을 통과시키는 판단은 호출부 몫', async () => {
		const m = await import('./guildIdentity');
		expect(m.sameGuild(null, { kind: 'local', path: 'C:/a' })).toBe(false);
		expect(m.sameGuild({ kind: 'local', path: 'C:/a' }, undefined)).toBe(false);
	});
});

describe('BUG-176 currentGuildId', () => {
	it('브라우저 모드는 null — 길드 전환 개념이 없어 가드를 걸지 않는다', async () => {
		detectEnv = () => 'browser';
		const m = await import('./guildIdentity');
		expect(await m.currentGuildId()).toBeNull();
	});

	it('원격 세션이 활성이면 원격 식별자', async () => {
		remoteUrl = 'http://host:3000';
		remoteActive = true;
		const m = await import('./guildIdentity');
		expect(await m.currentGuildId()).toEqual({ kind: 'remote', url: 'http://host:3000' });
	});

	it('원격 URL 이 남아 있어도 이번 세션에 연결한 게 아니면 원격으로 보지 않는다', async () => {
		// BUG-095/099 와 같은 기준 — 잔존값과 실제 연결을 구분해야 한다.
		remoteUrl = 'http://host:3000';
		remoteActive = false;
		const m = await import('./guildIdentity');
		// 로컬 경로 조회(invoke)는 이 환경에서 실패 → null. 원격으로 오인하지 않는 것이 핵심.
		expect(await m.currentGuildId()).toBeNull();
	});
});
