<!--
  BUG-021 fix1: Markdown 본문 렌더링 공유 컴포넌트.

  Quest Detail 과 Campaign Detail 의 본문 프리뷰가 같은 스타일을 쓰도록
  통일. 캠페인 스타일 base (헤더 사이즈 = 브라우저 기본 = 명확히 구분,
  배경 #0d1117) + Quest 의 확장 (pre / blockquote / table / hr / a) 머지.
-->
<script lang="ts">
	import { marked } from 'marked';

	let { source }: { source: string } = $props();

	let html = $derived(marked(source ?? '', { async: false }) as string);
</script>

<div class="md">{@html html}</div>

<style>
	.md {
		background: #0d1117;
		border: 1px solid #21262d;
		border-radius: 6px;
		padding: 0.85rem 1rem;
		color: #c9d1d9;
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
		color: #e6edf3;
		margin: 1em 0 0.4em;
	}

	.md :global(p) { margin: 0.5em 0; }
	.md :global(ul), .md :global(ol) { padding-left: 1.5rem; margin: 0.4em 0; }
	.md :global(input[type='checkbox']) { margin-right: 0.4rem; }

	.md :global(code) {
		background: #161b22;
		padding: 0.1rem 0.3rem;
		border-radius: 3px;
	}
	.md :global(pre) {
		background: #161b22;
		border: 1px solid #21262d;
		border-radius: 6px;
		padding: 0.75rem 1rem;
		overflow-x: auto;
	}
	.md :global(pre code) { background: none; padding: 0; color: #c9d1d9; }
	.md :global(blockquote) {
		border-left: 3px solid #30363d;
		margin: 0.5em 0;
		padding: 0.25em 0.75em;
		color: #8b949e;
	}
	/* BUG-039: 긴 URL 이 본문 폭을 넘어 가로 스크롤 발생하던 문제 — anywhere
	   으로 break (`word-break: break-word` 는 deprecated 대안 — 둘 다 적용해서
	   브라우저 지원 폭 확보). */
	.md :global(a) {
		color: #58a6ff;
		overflow-wrap: anywhere;
		word-break: break-word;
	}
	.md :global(hr) {
		border: none;
		border-top: 1px solid #21262d;
		margin: 1em 0;
	}
	.md :global(table) {
		border-collapse: collapse;
		width: 100%;
		font-size: 0.875rem;
	}
	.md :global(th),
	.md :global(td) {
		border: 1px solid #21262d;
		padding: 0.35rem 0.6rem;
	}
	.md :global(th) { background: #161b22; }
</style>
