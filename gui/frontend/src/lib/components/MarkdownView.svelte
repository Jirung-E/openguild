<!--
  BUG-021 fix1: Markdown 본문 렌더링 공유 컴포넌트.

  Quest Detail 과 Campaign Detail 의 본문 프리뷰가 같은 스타일을 쓰도록
  통일. 캠페인 스타일 base (헤더 사이즈 = 브라우저 기본 = 명확히 구분,
  배경 var(--bg)) + Quest 의 확장 (pre / blockquote / table / hr / a) 머지.
-->
<script lang="ts">
	import { marked } from 'marked';
	import { tick } from 'svelte';
	// DEV-111: mermaid 다이어그램 렌더링 — lazy import (~700KB), 블록 있을 때만.
	import { theme, resolveTheme } from '$lib/stores/theme';

	let { source }: { source: string } = $props();

	let html = $derived(marked(source ?? '', { async: false }) as string);
	let container: HTMLDivElement | undefined = $state(undefined);

	// 매 effect 실행 시 mermaid 블록 탐지 + 렌더. id 충돌 방지용 counter.
	let renderCounter = 0;

	// DEV-111 fix1: mermaid.render() 는 임시 컨테이너를 body 에 만들어 SVG 를
	// 그리는데, syntax error 시 그 임시 div 에 bomb 아이콘 + "Syntax error in
	// text mermaid version X.Y.Z" 를 그려놓고 throw 한다. 우리 catch 는 throw
	// 만 잡고 댓글 안의 <pre> 만 교체하므로 body 끝의 leftover 가 남아
	// 페이지 최하단에 폭탄이 계속 보임. SPA 라우트 전환에도 사라지지 않아
	// markdown preview 없는 페이지에서도 보였음.
	//
	// 수정:
	//   (A) mermaid.parse(code, { suppressErrors: true }) 로 render 전 검증.
	//       false 면 render 자체를 부르지 않음 — DOM 오염 X.
	//   (B) 안전망: render 실패 시 mermaid 가 만들었을 수 있는 임시 노드를
	//       id 기준으로 직접 제거.
	async function renderMermaidBlocks() {
		if (!container) return;
		const blocks = container.querySelectorAll<HTMLElement>('pre > code.language-mermaid');
		if (blocks.length === 0) return;
		const { default: mermaid } = await import('mermaid');
		const eff = resolveTheme($theme);
		mermaid.initialize({
			startOnLoad: false,
			theme: eff === 'dark' ? 'dark' : 'default',
			securityLevel: 'loose',
			fontFamily: 'inherit'
		});
		for (const block of Array.from(blocks)) {
			const pre = block.parentElement;
			if (!pre) continue;
			const code = block.innerText;
			const id = `mm-${++renderCounter}-${Math.random().toString(36).slice(2, 8)}`;
			// (A) parse pre-check — syntax error 시 render 안 부름.
			let parseOk = true;
			let parseErr: unknown = null;
			try {
				const res = await mermaid.parse(code, { suppressErrors: true });
				parseOk = res !== false;
			} catch (e) {
				// suppressErrors:true 면 throw 안 하지만 방어.
				parseOk = false;
				parseErr = e;
			}
			if (!parseOk) {
				showInlineError(pre, parseErr ?? new Error('syntax error'));
				continue;
			}
			try {
				const { svg } = await mermaid.render(id, code);
				const wrap = document.createElement('div');
				wrap.className = 'mermaid-rendered';
				wrap.innerHTML = svg;
				pre.replaceWith(wrap);
			} catch (e) {
				showInlineError(pre, e);
				// (B) 혹시 모를 leftover 정리. mermaid 가 만드는 임시 컨테이너 후보:
				//   - `<svg id="${id}">` (render 호출 시 전달한 id)
				//   - `<div id="d${id}">` (mermaid v11 의 hidden 임시 div)
				document.getElementById(id)?.remove();
				document.getElementById(`d${id}`)?.remove();
			}
		}
	}

	function showInlineError(pre: HTMLElement, e: unknown) {
		const err = document.createElement('div');
		err.className = 'mermaid-error';
		err.textContent = `mermaid 렌더 실패: ${(e as Error)?.message ?? String(e)}`;
		pre.replaceWith(err);
	}

	$effect(() => {
		// html / theme 변경 시 재렌더.
		void html;
		void $theme;
		tick().then(() => {
			renderMermaidBlocks();
			rewriteLocalMedia();
		});
	});

	// DEV-069: markdown 본문의 로컬 이미지 / 동영상 참조 해석.
	//
	// WebView 는 raw 파일 경로 / file:// 로드를 차단 (frontend origin 이
	// http://tauri.localhost) — `![](attachments/foo.png)` 같은 `.guild/` 상대
	// 참조를 asset URL (Tauri convertFileSrc) / 서버 endpoint (브라우저) 로
	// 재작성. 웹 URL (http/https/data/asset) 은 그대로 통과.
	import { guildFileUrl } from '$lib/utils/banner';
	function isExternalSrc(src: string): boolean {
		return /^(https?:|data:|asset:|blob:|\/api\/)/i.test(src) || src.startsWith('//');
	}
	async function rewriteLocalMedia() {
		if (!container) return;
		const targets = container.querySelectorAll<HTMLElement>('img, video, source');
		for (const el of Array.from(targets)) {
			const src = el.getAttribute('src');
			if (!src || isExternalSrc(src) || el.dataset.ogRewritten) continue;
			// 절대 OS 경로 (`C:\...` / `/home/...`) 는 보안상 미지원 — `.guild/`
			// 상대 (attachments/ / assets/) 만. 그 외는 그대로 (깨진 이미지 표시).
			const rel = src.replace(/^\.\//, '');
			if (!rel.startsWith('attachments/') && !rel.startsWith('assets/')) continue;
			try {
				const url = await guildFileUrl(rel);
				el.setAttribute('src', url);
				el.dataset.ogRewritten = '1';
			} catch {
				/* 해석 실패 — 원본 유지 */
			}
		}
	}
</script>

<div class="md" bind:this={container}>{@html html}</div>

<style>
	.md {
		background: var(--bg);
		border: 1px solid var(--bg-subtle);
		border-radius: 6px;
		padding: 0.85rem 1rem;
		color: var(--text);
		font-size: 0.9rem;
		line-height: 1.55;
		/* BUG-039: 자식 (긴 링크 / inline code 등) 이 컨테이너 폭을 못 넘게. */
		max-width: 100%;
		overflow-wrap: anywhere;
	}

	/* 헤더 — 캠페인 spirit (브라우저 기본 사이즈로 명확 구분, 컬러만 통일). */
	.md :global(h1),
	.md :global(h2),
	.md :global(h3),
	.md :global(h4),
	.md :global(h5),
	.md :global(h6) {
		color: var(--text-strong);
		margin: 1em 0 0.4em;
	}

	.md :global(p) { margin: 0.5em 0; }
	.md :global(ul), .md :global(ol) { padding-left: 1.5rem; margin: 0.4em 0; }
	.md :global(input[type='checkbox']) { margin-right: 0.4rem; }

	.md :global(code) {
		background: var(--bg-elevated);
		padding: 0.1rem 0.3rem;
		border-radius: 3px;
	}
	.md :global(pre) {
		background: var(--bg-elevated);
		border: 1px solid var(--bg-subtle);
		border-radius: 6px;
		padding: 0.75rem 1rem;
		overflow-x: auto;
	}
	.md :global(pre code) { background: none; padding: 0; color: var(--text); }
	.md :global(blockquote) {
		border-left: 3px solid var(--border);
		margin: 0.5em 0;
		padding: 0.25em 0.75em;
		color: var(--text-muted);
	}
	/* BUG-039: 긴 URL 이 본문 폭을 넘어 가로 스크롤 발생하던 문제 — anywhere
	   으로 break (`word-break: break-word` 는 deprecated 대안 — 둘 다 적용해서
	   브라우저 지원 폭 확보). */
	.md :global(a) {
		color: var(--accent);
		overflow-wrap: anywhere;
		word-break: break-word;
	}
	.md :global(hr) {
		border: none;
		border-top: 1px solid var(--bg-subtle);
		margin: 1em 0;
	}
	.md :global(table) {
		border-collapse: collapse;
		width: 100%;
		font-size: 0.875rem;
	}
	.md :global(th),
	.md :global(td) {
		border: 1px solid var(--bg-subtle);
		padding: 0.35rem 0.6rem;
	}
	.md :global(th) { background: var(--bg-elevated); }
	/* DEV-111: mermaid 다이어그램 렌더 영역. */
	.md :global(.mermaid-rendered) {
		display: flex;
		justify-content: center;
		padding: 0.5rem 0;
		margin: 0.5em 0;
	}
	.md :global(.mermaid-rendered svg) {
		max-width: 100%;
		height: auto;
	}
	.md :global(.mermaid-error) {
		padding: 0.5rem 0.75rem;
		background: color-mix(in srgb, var(--danger) 12%, transparent);
		border: 1px solid color-mix(in srgb, var(--danger) 40%, transparent);
		border-radius: 6px;
		color: var(--danger);
		font-size: 0.8rem;
		font-family: 'SFMono-Regular', Consolas, monospace;
		white-space: pre-wrap;
	}
</style>
