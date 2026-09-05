// BUG-263: 파일명에 공백이 있으면 본문 첨부가 이미지로 안 나오고 마크다운
// 원문이 글자 그대로 보였다.
//
// 저장 파일명은 원본 이름을 살린다(DEV-324) — 목록에서 알아보라고. 그래서
// `sanitize_stem` 은 공백을 **일부러** 남긴다. 그런데 CommonMark 에서 꺾쇠
// 없는 링크 목적지에는 공백이 올 수 없다.
//
// 이 파일은 삽입부터 로드까지 **한 줄로 이어서** 고정한다. 셋 중 하나만
// 검증하면 통과하는데도 화면은 깨진다:
//
//   markdownFor  →  marked 가 파싱  →  decodeRelPath  →  파일 경로
//
// 특히 `decodeURI` 는 `%` 가 든 이름에서 **던진다**(marked 가 `%` 를 인코딩
// 하지 않는다). 거기서 던지면 공백 버그를 `%` 버그로 바꾸는 것뿐이다.
import { describe, it, expect } from 'vitest';
import { Marked } from 'marked';
import { markdownFor } from './editor-attach';
import { decodeRelPath, encodeRelPath } from './banner';

const md = new Marked();

/** 렌더된 HTML 에서 img src 를 뽑는다. 파싱 실패(원문 노출)면 null. */
function imgSrc(source: string): string | null {
	const html = md.parse(source, { async: false }) as string;
	return html.match(/<img[^>]+src="([^"]*)"/)?.[1] ?? null;
}

function linkHref(source: string): string | null {
	const html = md.parse(source, { async: false }) as string;
	return html.match(/<a[^>]+href="([^"]*)"/)?.[1] ?? null;
}

/** 저장 파일명 — core 의 `sanitize_stem` 은 공백을 남기고 `<>` 는 `_` 로 바꾼다. */
const NAMES = [
	'스크린샷 2026-09-04.png',
	'한글 파일 이름.png',
	'design mockup v2.png',
	'a(b).png',
	'a#b.png',
	'a&b.png',
	"it's.png",
	'a[b].png',
	'100%.png',
	'plain.png'
];

describe('markdownFor — 삽입한 마크다운이 실제로 이미지로 파싱되는가', () => {
	for (const name of NAMES) {
		it(`"${name}" 이 이미지로 렌더된다`, () => {
			const rel = `attachments/${name}`;
			expect(imgSrc(markdownFor(rel, 'png', name))).not.toBeNull();
		});
	}

	it('꺾쇠가 없으면 공백 있는 이름은 파싱 자체가 안 된다 — 이 버그의 정체', () => {
		const rel = 'attachments/스크린샷 2026.png';
		// 고치기 전 형식.
		expect(imgSrc(`![x](${rel})`)).toBeNull();
		// 고친 형식.
		expect(imgSrc(markdownFor(rel, 'png', 'x'))).not.toBeNull();
	});

	it('동영상은 HTML 속성이라 원래 안전했다 — 건드리지 않는다', () => {
		const out = markdownFor('attachments/영상 하나.mp4', 'mp4', 'v');
		expect(out).toBe('<video controls src="attachments/영상 하나.mp4"></video>');
	});

	it('비미디어 링크도 같은 규칙을 지킨다', () => {
		const rel = 'attachments/문서 파일.pdf';
		expect(linkHref(markdownFor(rel, 'pdf', 'doc'))).not.toBeNull();
	});
});

describe('decodeRelPath — marked 가 인코딩한 src 를 파일 경로로 되돌린다', () => {
	for (const name of NAMES) {
		it(`"${name}" 이 원래 경로로 왕복한다`, () => {
			const rel = `attachments/${name}`;
			const src = imgSrc(markdownFor(rel, 'png', name));
			expect(src).not.toBeNull();
			expect(decodeRelPath(src as string)).toBe(rel);
		});
	}

	it('`%` 가 든 이름에서 던지지 않는다 — decodeURI 는 여기서 URIError 를 낸다', () => {
		// marked 는 `%` 를 인코딩하지 않아 `%.p` 가 잘못된 이스케이프가 된다.
		expect(() => decodeURI('attachments/100%.png')).toThrow();
		expect(decodeRelPath('attachments/100%.png')).toBe('attachments/100%.png');
	});

	it('접두사 판정이 디코드 후에도 유지된다 — 재작성 대상에서 빠지면 안 된다', () => {
		const src = imgSrc(markdownFor('attachments/스크린샷 2026.png', 'png', 'x'));
		expect(decodeRelPath(src as string).startsWith('attachments/')).toBe(true);
	});
});

describe('encodeRelPath — HTTP(브라우저/원격)에서 요청 가능한 경로', () => {
	it('`#` 를 인코딩한다 — 안 하면 거기서 잘려 다른 경로를 요청한다', () => {
		expect(encodeRelPath('attachments/a#b.png')).toBe('attachments/a%23b.png');
	});

	it('공백과 `%` 도 인코딩한다', () => {
		expect(encodeRelPath('attachments/스크린샷 2026.png')).toContain('%20');
		expect(encodeRelPath('attachments/100%.png')).toBe('attachments/100%25.png');
	});

	it('경로 구분자는 남긴다 — 세그먼트가 뭉치면 다른 파일이 된다', () => {
		expect(encodeRelPath('attachments/sub/a b.png')).toBe('attachments/sub/a%20b.png');
	});

	it('인코딩된 경로를 디코드하면 원래 파일 경로로 돌아온다', () => {
		for (const name of NAMES) {
			const rel = `attachments/${name}`;
			expect(decodeURIComponent(encodeRelPath(rel))).toBe(rel);
		}
	});
});
