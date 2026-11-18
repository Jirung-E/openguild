<script lang="ts">
	// DEV-484: 검색 pill(데스크탑 타이틀바 · 웹 메뉴바)에서 길드 이름 뒤에 "› 지금 문서 번호".
	// 문서 페이지(퀘스트 · 캠페인 · 규칙 · 도서관)에서만 보이고, 칩은 종류 색을 아주 옅게 쓴다(admin 이 시안 G2 로 정함).
	// 자리가 좁으면(창 폭 720px 미만) 숨겨 길드 이름만 남긴다 — 예전 모양 그대로.
	import { page } from '$app/stores';
	import { classifyDocRoute } from '$lib/stores/recentDocs';
	import Icon from '$lib/components/Icon.svelte';

	let doc = $derived(classifyDocRoute($page.url.pathname + $page.url.search));
</script>

{#if doc}
	<span class="pill-doc {doc.kind}">
		<span class="sep" aria-hidden="true"><Icon name="chevron-right" size="0.8em" /></span>
		<span class="chip">{doc.label}</span>
	</span>
{/if}

<style>
	.pill-doc {
		display: inline-flex;
		align-items: center;
		flex: none;
		min-width: 0;
	}
	.pill-doc.quest {
		--kc: var(--accent);
	}
	.pill-doc.campaign {
		--kc: var(--hl-pre);
	}
	.pill-doc.rule {
		--kc: var(--success);
	}
	.pill-doc.book {
		--kc: var(--warning);
	}
	.sep {
		display: inline-flex;
		margin: 0 var(--sp-6);
		opacity: 0.6;
	}
	.chip {
		display: inline-block;
		max-width: 9rem;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
		height: 1.05rem;
		/* 테두리 1px 씩을 뺀 높이 — 글자가 상자 가운데. */
		line-height: calc(1.05rem - 2px);
		padding: 0 var(--sp-6);
		box-sizing: border-box;
		border-radius: var(--r-pill);
		font-family: var(--font-mono);
		font-size: 0.7rem; /* token-ok: 식별자 칩 글자 — 검색 팔레트 · 보드의 번호 칩(--pill-fs 0.7~0.72rem)과 같게 */
		font-weight: var(--fw-semibold);
		color: color-mix(in srgb, var(--kc) 75%, var(--text));
		background: color-mix(in srgb, var(--kc) 6%, transparent);
		border: var(--bw) solid color-mix(in srgb, var(--kc) 14%, transparent);
	}
	@media (max-width: 720px) {
		.pill-doc {
			display: none;
		}
	}
</style>
