import { describe, expect, it } from 'vitest';
import { guildCtxStamp, shouldRestoreGuildCtx } from './guildSession';

describe('BUG-399: 히스토리 항목의 길드 화면 표식', () => {
	it('welcome 은 언제나 길드 밖', () => {
		expect(guildCtxStamp('/welcome', true)).toBe(false);
		expect(shouldRestoreGuildCtx('/welcome', true)).toBe(false);
		expect(shouldRestoreGuildCtx('/welcome', undefined)).toBe(false);
	});
	it('보드는 플래그가 아직 꺼져 있어도 길드 안', () => {
		expect(guildCtxStamp('/', false)).toBe(true);
	});
	it('다른 화면은 그때 플래그를 따른다', () => {
		expect(guildCtxStamp('/quests/DEV-1', true)).toBe(true);
		expect(guildCtxStamp('/settings', false)).toBe(false);
	});
	it('남긴 표식이 있으면 그대로 되살린다', () => {
		expect(shouldRestoreGuildCtx('/quests/DEV-1', true)).toBe(true);
		expect(shouldRestoreGuildCtx('/settings', false)).toBe(false);
		expect(shouldRestoreGuildCtx('/settings', true)).toBe(true);
	});
	it('표식 없는 옛 항목은 설정만 빼고 길드 안으로 본다', () => {
		expect(shouldRestoreGuildCtx('/quests/DEV-1', undefined)).toBe(true);
		expect(shouldRestoreGuildCtx('/settings', undefined)).toBe(false);
	});
});
