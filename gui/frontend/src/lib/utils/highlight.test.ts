import { describe, it, expect } from 'vitest';
import { highlightSegments, searchTokens } from './highlight';

/** 조각 배열을 읽기 쉬운 문자열로 — 걸린 부분을 «» 로 감싼다. */
function render(text: string, query: string): string {
	return highlightSegments(text, query)
		.map((s) => (s.hit ? `«${s.text}»` : s.text))
		.join('');
}

describe('searchTokens', () => {
	it('공백으로 나누고 빈 토큰을 버린다', () => {
		expect(searchTokens('  수달   젤리곰 ')).toEqual(['수달', '젤리곰']);
	});
	it('검색어가 비면 빈 배열', () => {
		expect(searchTokens('   ')).toEqual([]);
	});
});

describe('highlightSegments', () => {
	it('한 토큰을 표시한다', () => {
		expect(render('이 댓글에만 젤리곰 이라는 단어가 있다', '젤리곰')).toBe(
			'이 댓글에만 «젤리곰» 이라는 단어가 있다'
		);
	});

	it('같은 토큰이 여러 번 나오면 전부 표시한다', () => {
		expect(render('수달 그리고 또 수달', '수달')).toBe('«수달» 그리고 또 «수달»');
	});

	/** core 의 AND 는 "문서가 나오는 조건" 이고, 표시는 보이는 일치 전부다. */
	it('토큰이 여러 개면 각각 표시한다', () => {
		expect(render('수달과 젤리곰', '수달 젤리곰')).toBe('«수달»과 «젤리곰»');
	});

	it('대소문자를 무시한다', () => {
		expect(render('Server --help leaks', 'server')).toBe('«Server» --help leaks');
		expect(render('server --HELP leaks', 'help')).toBe('server --«HELP» leaks');
	});

	/** 겹치는 일치는 하나로 합친다 — 조각이 쪼개지면 어색하다. */
	it('겹치는 토큰을 하나로 합친다', () => {
		expect(render('가나다', '가나 나다')).toBe('«가나다»');
	});

	it('맞닿은 일치도 합친다', () => {
		expect(render('abab', 'ab')).toBe('«abab»');
	});

	it('걸린 게 없으면 원문 한 조각', () => {
		const segs = highlightSegments('아무것도 없음', '수달');
		expect(segs).toEqual([{ text: '아무것도 없음', hit: false }]);
	});

	it('검색어가 비면 원문 한 조각', () => {
		expect(highlightSegments('본문', '  ')).toEqual([{ text: '본문', hit: false }]);
	});

	it('빈 본문은 빈 배열', () => {
		expect(highlightSegments('', '수달')).toEqual([]);
	});

	it('맨 앞 / 맨 뒤 일치도 처리한다', () => {
		expect(render('수달 보고서', '수달')).toBe('«수달» 보고서');
		expect(render('보고서 수달', '수달')).toBe('보고서 «수달»');
		expect(render('수달', '수달')).toBe('«수달»');
	});

	/**
	 * 발췌는 문서 본문에서 온 값이라 마크업이 섞일 수 있다. 조각 배열은 그냥
	 * 텍스트라 그대로 돌려주고, 렌더는 컴포넌트가 텍스트로 한다 — `{@html}` 을
	 * 쓰지 않는 이유.
	 */
	it('마크업처럼 보이는 문자열도 그냥 텍스트로 다룬다', () => {
		expect(render('<script>alert(1)</script> 수달', '수달')).toBe(
			'<script>alert(1)</script> «수달»'
		);
		const segs = highlightSegments('<b>수달</b>', '수달');
		expect(segs.map((s) => s.text).join('')).toBe('<b>수달</b>');
	});

	it('정규식 특수문자가 들어가도 문자 그대로 찾는다', () => {
		expect(render('a.b 그리고 axb', 'a.b')).toBe('«a.b» 그리고 axb');
		expect(render('비용 100% 절감', '100%')).toBe('비용 «100%» 절감');
		expect(render('경로 c:\\tmp', 'c:\\tmp')).toBe('경로 «c:\\tmp»');
	});

	it('조각을 이어 붙이면 항상 원문이다', () => {
		for (const [text, q] of [
			['이 댓글에만 젤리곰 이라는 단어', '젤리곰'],
			['가나다라마', '가나 라마'],
			['아무 일치 없음', 'zzz'],
			['수달', '수달']
		] as const) {
			expect(highlightSegments(text, q).map((s) => s.text).join('')).toBe(text);
		}
	});
});
