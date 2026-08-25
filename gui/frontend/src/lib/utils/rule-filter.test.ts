import { describe, it, expect } from 'vitest';
import type { RuleEntry } from '$lib/api/rules';
import { filterRules, excerptAround } from './rule-filter';

function rule(slug: string, content = '', tags: string[] = []): RuleEntry {
	return { slug, content, tags, created_at: '', updated_at: '' };
}

const RULES = [
	rule('release-process', '태그를 밀기 전에 CHANGELOG 절이 반드시 있어야 한다.', ['ops']),
	rule('cli-command-naming', '명령 이름은 동사-명사 순서로.', ['cli']),
	rule('file-truth-db-cache', 'index.db 는 파일에서 다시 만들 수 있는 파생물이다.', ['core', 'ops'])
];

describe('filterRules — 검색어', () => {
	it('slug 로 찾는다', () => {
		const r = filterRules(RULES, 'release');
		expect(r.map((x) => x.entry.slug)).toEqual(['release-process']);
		expect(r[0].matchedIn).toContain('slug');
	});

	/** 핵심: 본문에만 있는 단어도 찾아야 한다 — 이게 REQ-013 의 목적. */
	it('본문에만 있는 단어로 찾는다', () => {
		const r = filterRules(RULES, '파생물');
		expect(r.map((x) => x.entry.slug)).toEqual(['file-truth-db-cache']);
		expect(r[0].matchedIn).toEqual(['body']);
	});

	it('태그로도 찾는다', () => {
		const r = filterRules(RULES, 'cli');
		// slug 에 'cli' 가 있는 것과 태그가 'cli' 인 것 — 같은 문서다.
		expect(r.map((x) => x.entry.slug)).toEqual(['cli-command-naming']);
		expect(r[0].matchedIn).toEqual(expect.arrayContaining(['slug', 'tag']));
	});

	it('대소문자를 무시한다', () => {
		expect(filterRules(RULES, 'RELEASE')).toHaveLength(1);
		expect(filterRules([rule('x', 'CHANGELOG')], 'changelog')).toHaveLength(1);
	});

	it('맞는 게 없으면 빈 배열', () => {
		expect(filterRules(RULES, '없는단어')).toEqual([]);
	});

	it('검색어가 비면 전부 통과하고 matchedIn 은 빈다', () => {
		const r = filterRules(RULES, '   ');
		expect(r).toHaveLength(3);
		expect(r[0].matchedIn).toEqual([]);
	});

	it('입력 순서를 유지한다', () => {
		const r = filterRules(RULES, '다');
		const idx = r.map((x) => RULES.findIndex((e) => e.slug === x.entry.slug));
		expect(idx).toEqual([...idx].sort((a, b) => a - b));
	});
});

describe('filterRules — 태그 필터 (기존 동작 유지)', () => {
	it('선택한 태그를 전부 가져야 통과한다 (AND)', () => {
		expect(filterRules(RULES, '', new Set(['ops'])).map((x) => x.entry.slug)).toEqual([
			'release-process',
			'file-truth-db-cache'
		]);
		expect(filterRules(RULES, '', new Set(['ops', 'core'])).map((x) => x.entry.slug)).toEqual([
			'file-truth-db-cache'
		]);
	});

	it('태그 필터와 검색어는 AND 로 걸린다', () => {
		expect(filterRules(RULES, '파생물', new Set(['cli']))).toEqual([]);
		expect(filterRules(RULES, '파생물', new Set(['core']))).toHaveLength(1);
	});
});

describe('excerptAround', () => {
	it('앞뒤를 잘라내고 생략 부호를 붙인다', () => {
		const text = 'ㄱ'.repeat(100) + '표적' + 'ㄴ'.repeat(100);
		const out = excerptAround(text, 100, 10);
		expect(out.startsWith('…')).toBe(true);
		expect(out.endsWith('…')).toBe(true);
		expect(out).toContain('표적');
	});

	it('짧은 본문은 생략 부호 없이 그대로', () => {
		expect(excerptAround('짧은 본문', 0, 40)).toBe('짧은 본문');
	});

	/** 한글이 문자 단위로 잘려야 한다 — 바이트로 자르면 깨진다. */
	it('한글이 깨지지 않는다', () => {
		const out = excerptAround('가나다라마바사아자차카타파하', 7, 3);
		expect(out).toMatch(/^[…가-힣\s]+…?$/);
		expect(out).not.toContain('�');
	});
});
