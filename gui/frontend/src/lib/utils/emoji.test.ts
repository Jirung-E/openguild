import { describe, it, expect } from 'vitest';
import { isSingleEmoji } from './emoji';

describe('isSingleEmoji', () => {
	it('accepts a plain single emoji', () => {
		expect(isSingleEmoji('😀')).toBe(true);
		expect(isSingleEmoji('👍')).toBe(true);
		expect(isSingleEmoji('🔥')).toBe(true);
	});

	it('accepts emoji with variation selector', () => {
		expect(isSingleEmoji('❤️')).toBe(true);
	});

	it('accepts skin-tone modified emoji', () => {
		expect(isSingleEmoji('👍🏽')).toBe(true);
	});

	it('accepts ZWJ sequences (e.g. family, couple)', () => {
		expect(isSingleEmoji('👨‍👩‍👧‍👦')).toBe(true);
		expect(isSingleEmoji('👨‍❤️‍👨')).toBe(true);
	});

	it('accepts flag emoji (regional indicator pair)', () => {
		expect(isSingleEmoji('🇰🇷')).toBe(true);
	});

	// DEV-132 후속(admin 보고): 이전엔 길이 제한 없이 임의 문자열이 그대로
	// 들어갈 수 있었다 — 텍스트/문장/여러 이모지는 거부.
	it('rejects plain text', () => {
		expect(isSingleEmoji('hello')).toBe(false);
		expect(isSingleEmoji('안녕')).toBe(false);
	});

	it('rejects empty string', () => {
		expect(isSingleEmoji('')).toBe(false);
	});

	it('rejects multiple independent emoji', () => {
		expect(isSingleEmoji('😀😀')).toBe(false);
		expect(isSingleEmoji('👍🔥')).toBe(false);
	});

	it('rejects emoji plus trailing text', () => {
		expect(isSingleEmoji('😀!')).toBe(false);
		expect(isSingleEmoji('a😀')).toBe(false);
	});

	it('rejects plain digits/ASCII symbols (not Extended_Pictographic)', () => {
		expect(isSingleEmoji('#')).toBe(false);
		expect(isSingleEmoji('1')).toBe(false);
	});
});
