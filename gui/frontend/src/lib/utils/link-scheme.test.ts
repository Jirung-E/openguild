// DEV-485: 문서 링크 주소 종류.
import { describe, it, expect } from 'vitest';
import DOMPurify from 'dompurify';
import { ALLOWED_URI_REGEXP, isExternalHref } from './link-scheme';

const clean = (html: string) => DOMPurify.sanitize(html, { ALLOWED_URI_REGEXP });

describe('문서 속 링크 거르기', () => {
	it('다른 앱으로 가는 주소는 남긴다', () => {
		for (const href of [
			'vscode://file/C:/work/a.ts:10',
			'obsidian://open?vault=x',
			'https://example.com',
			'mailto:a@b.c',
			'/quests/DEV-1'
		]) {
			expect(clean(`<a href="${href}">x</a>`)).toContain(`href="${href}"`);
		}
	});
	it('앱 안에서 도는 주소는 지운다 — 대소문자 · 공백을 섞어도', () => {
		for (const href of [
			'javascript:alert(1)',
			'JaVaScRiPt:alert(1)',
			'java\tscript:alert(1)',
			'vbscript:x',
			'data:text/html,x'
		]) {
			expect(clean(`<a href="${href}">x</a>`)).not.toContain('href');
		}
	});
	it('이미지의 data: 는 그대로', () => {
		expect(clean('<img src="data:image/png;base64,AAAA">')).toContain('src="data:image/png');
	});
});

describe('누르면 바깥 앱으로 넘기나', () => {
	it('주소 종류가 있으면 넘긴다', () => {
		expect(isExternalHref('vscode://file/a')).toBe(true);
		expect(isExternalHref('https://example.com')).toBe(true);
		expect(isExternalHref('mailto:a@b.c')).toBe(true);
	});
	it('앱 안 이동 · 앱 안에서 도는 주소는 넘기지 않는다', () => {
		expect(isExternalHref('/quests/DEV-1')).toBe(false);
		expect(isExternalHref('#top')).toBe(false);
		expect(isExternalHref('javascript:alert(1)')).toBe(false);
		expect(isExternalHref('data:text/html,x')).toBe(false);
		expect(isExternalHref(`${location.origin}/quests/DEV-1`)).toBe(false);
	});
});
