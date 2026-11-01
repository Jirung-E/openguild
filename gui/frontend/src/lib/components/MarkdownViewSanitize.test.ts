// BUG-387: 본문 속 HTML 은 **거른 뒤** 넣는다 — 남이 쓴 본문이 git · 서버로 들어온다.
// 막는 것(스크립트 · on… 속성 · javascript: 주소 · iframe · style)과 지키는 것(첨부 동영상 · 이미지 · 링크 ·
// 코드블록 하이라이트 · 체크박스 · 표 · cross-link)을 실제 렌더로 본다.

import { describe, it, expect, vi } from 'vitest';
import { render } from '@testing-library/svelte';
import { tick } from 'svelte';
import MarkdownView from './MarkdownView.svelte';
import { questIndex, questIndexNs, type IndexedRef } from '$lib/stores/questIndex';

vi.mock('$lib/api/client', () => ({ api: { get: vi.fn(async () => []) } }));
vi.mock('$lib/utils/open-item', () => ({ openInPage: vi.fn(), openInWindow: vi.fn() }));

async function body(source: string) {
	const { container } = render(MarkdownView, { source });
	await tick();
	await tick();
	return container.querySelector('.md') as HTMLElement;
}

describe('BUG-387 본문 HTML 거르기', () => {
	it('스크립트 · on… 속성 · javascript: 주소 · iframe 은 지운다', async () => {
		const md = await body(
			'<img src="x" onerror="alert(1)">\n\n<script>alert(2)</script>\n\n' +
				'<svg onload="alert(3)"><circle r="1"/></svg>\n\n<iframe srcdoc="<script>alert(4)</script>"></iframe>\n\n' +
				'[눌러](javascript:alert(5)) <a href="jav&#x61;script:alert(6)">숨긴</a>\n\n<details open ontoggle="alert(7)"><summary>접기</summary>안</details>'
		);
		expect(md.querySelector('img')?.getAttribute('onerror')).toBeNull();
		expect(md.querySelector('script')).toBeNull();
		expect(md.querySelector('iframe')).toBeNull();
		// 남은 요소 어디에도 on… 속성이 없다(svg · iframe 은 목록 밖이라 글자로 보인다).
		for (const el of md.querySelectorAll('*'))
			for (const at of el.attributes) expect(at.name.startsWith('on'), at.name).toBe(false);
		expect(md.querySelector('svg')).toBeNull();
		expect(md.textContent).toContain('<script>alert(2)</script>');
		for (const a of md.querySelectorAll('a'))
			expect(a.getAttribute('href') ?? '').not.toMatch(/script:/i);
		// 안전한 HTML 은 남는다(접기).
		expect(md.querySelector('details summary')?.textContent).toBe('접기');
		// 속성값 속 꺾쇠에서 끊기지 않는다 — 끊기면 닫히지 않은 iframe 이 뒤를 삼킨다.
		expect(md.textContent).toContain('<iframe srcdoc=');
	});

	it('style 태그 · style 속성 · form 류는 지운다 — 앱 화면을 덮어 꾸밀 수 있다', async () => {
		const md = await body(
			'<style>body{display:none}</style>\n\n<div style="position:fixed;inset:0">덮개</div>\n\n<form action="https://x"><button>보내기</button></form>'
		);
		expect(md.querySelector('style')).toBeNull();
		expect(md.querySelector('[style]')).toBeNull();
		expect(md.querySelector('form, button')).toBeNull();
		expect(md.textContent).toContain('덮개');
	});

	it('첨부 동영상 · 이미지 · 링크는 그대로', async () => {
		const md = await body(
			'설명\n\n<video controls src="attachments/a b.mp4"></video>\n\n문장 중간 <video controls src="attachments/w.webm"></video> 끝\n\n' +
				'![사진](https://example.com/x.png)\n\n[문서](https://example.com/d) · [첨부](attachments/f.pdf) · <https://auto.link> · [절](#h)'
		);
		const videos = [...md.querySelectorAll('video')];
		expect(videos).toHaveLength(2);
		expect(videos.every((v) => v.hasAttribute('controls'))).toBe(true);
		// 첨부 경로는 기존 처리(rewriteLocalMedia)가 앱 · 서버 주소로 바꾼다 — 거른 뒤에도 그 처리를 탄다.
		await vi.waitFor(() =>
			expect(videos[0].getAttribute('src')).toBe('/api/guild-files/attachments/a%20b.mp4')
		);
		expect(md.querySelector('img')?.getAttribute('src')).toBe('https://example.com/x.png');
		expect([...md.querySelectorAll('a')].map((a) => a.getAttribute('href'))).toEqual([
			'https://example.com/d',
			'attachments/f.pdf',
			'https://auto.link',
			'#h'
		]);
	});

	it('코드블록 · 인라인 코드 속 HTML 은 글자 그대로, 하이라이트도 그대로', async () => {
		const md = await body(
			'```html\n<script>alert(1)</script>\n<video src="x"></video>\n```\n\n`<img src=x onerror=alert(1)>`'
		);
		const pre = md.querySelector('pre code') as HTMLElement;
		expect(pre.className).toContain('hljs');
		expect(pre.querySelector('span[class^="hljs-"]')).not.toBeNull();
		expect(pre.textContent).toContain('<script>alert(1)</script>');
		expect(md.querySelector('p code')?.textContent).toBe('<img src=x onerror=alert(1)>');
		expect(md.querySelector('script, video')).toBeNull();
	});

	it('글 속 자리표시 · 태그 언급은 글자로 보인다 — 예전엔 사라지거나 뒤 문단을 삼켰다', async () => {
		// 실제 길드의 모양(DEV-185 · DEV-183): 백틱 없이 쓴 `<slug>`, 문장 중간의 `<style>`.
		const md = await body(
			'openguild quest show <slug> --full 과 Vec<String>\n\n' +
				'전역 CSS(컴포넌트 <style> 밖, no-hex 검사 비대상)로 둔다. 다음 문장도 보여야 한다.'
		);
		expect(md.textContent).toContain('quest show <slug> --full');
		expect(md.textContent).toContain('Vec<String>');
		expect(md.textContent).toContain('컴포넌트 <style> 밖, no-hex 검사 비대상');
		expect(md.textContent).toContain('다음 문장도 보여야 한다');
		expect(md.querySelector('style, slug')).toBeNull();
	});

	it('체크박스 · 표 · cross-link 는 그대로', async () => {
		questIndex.set(new Map([['DEV-001', { kind: 'quest', title: '있음' } as IndexedRef]]));
		questIndexNs.set(new Map([['quest:DEV-001', { kind: 'quest', title: '있음' } as IndexedRef]]));
		const md = await body(
			'- [x] 됨\n- [ ] 안 됨\n\n| a | b |\n|---|---|\n| 1 | 2 |\n\n[[DEV-001]]'
		);
		expect(md.querySelectorAll('input[type="checkbox"]')).toHaveLength(2);
		expect((md.querySelector('input[type="checkbox"]') as HTMLInputElement).checked).toBe(true);
		expect(md.querySelector('table td')?.textContent).toBe('1');
		expect(md.querySelector('a.xlink')?.getAttribute('href')).toBe('/quests/DEV-001');
	});
});
