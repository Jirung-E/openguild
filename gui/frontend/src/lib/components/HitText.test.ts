// 검색 결과의 제목 · 번호에도 걸린 글자를 칠한다.
import { describe, it, expect } from 'vitest';
import { render } from '@testing-library/svelte';
import HitText from './HitText.svelte';

describe('HitText', () => {
	it('걸린 글자만 mark 로, 대소문자 무시 · 낱말마다', () => {
		const { container } = render(HitText, {
			props: { text: 'Login 코끼리 화면', query: 'login 화면' }
		});
		expect([...container.querySelectorAll('mark')].map((m) => m.textContent)).toEqual([
			'Login',
			'화면'
		]);
		expect(container.textContent).toBe('Login 코끼리 화면');
	});

	it('검색어가 없으면 글만', () => {
		const { container } = render(HitText, { props: { text: '<b>제목</b>', query: '  ' } });
		expect(container.querySelector('mark')).toBeNull();
		expect(container.querySelector('b')).toBeNull();
		expect(container.textContent).toBe('<b>제목</b>');
	});
});
