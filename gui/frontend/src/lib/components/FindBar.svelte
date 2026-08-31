<!--
  REQ-018: 페이지 내 검색 (Ctrl/Cmd+F).

  Tauri WebView 에는 브라우저의 기본 찾기가 아예 없다. 웹 모드에는 있지만,
  admin 결정으로 **양쪽 다 이 구현을 쓴다** — 환경마다 다른 UI 가 되는 것보다
  낫다.

  칠하는 방식은 CSS Custom Highlight API 다. `<mark>` 로 감싸는 흔한 방법은
  Svelte 가 소유한 DOM 에 남의 노드를 끼워 넣는 것이라, 그 부분이 재렌더되는
  순간 서로 어긋난다. 여기서는 `Range` 만 만들어 두고 칠하므로 문서는 그대로다.
  API 가 없는 환경에서는 **이동만** 하고 칠하지 않는다(찾기가 통째로 죽는
  것보다 낫다).

  찾기 바 자신은 `data-find-skip` 이라 자기 글자를 자기가 찾지 않는다.
-->
<script lang="ts">
	import { onDestroy } from 'svelte';
	import { locale, t } from '$lib/stores/locale';
	import {
		buildSegments,
		findMatches,
		matchToRange,
		stepIndex,
		supportsHighlightApi,
		type Segment,
		type Match
	} from '$lib/utils/find-in-page';

	let { root, onclose }: { root: HTMLElement | null; onclose: () => void } = $props();

	let query = $state('');
	let matches = $state<Match[]>([]);
	let current = $state(-1);
	let inputEl = $state<HTMLInputElement | undefined>(undefined);

	let segments: Segment[] = [];
	const canHighlight = supportsHighlightApi();

	/** 강조 이름 — `global.css` 의 `::highlight()` 와 짝이다. */
	const ALL = 'og-find';
	const CUR = 'og-find-current';

	function clearHighlights() {
		if (!canHighlight) return;
		try {
			CSS.highlights.delete(ALL);
			CSS.highlights.delete(CUR);
		} catch {
			/* 지원하지 않는 환경 — 어차피 칠하지 않았다. */
		}
	}

	function paint() {
		if (!canHighlight) return;
		try {
			const all: Range[] = [];
			let curRange: Range | null = null;
			for (let i = 0; i < matches.length; i++) {
				const r = matchToRange(segments, matches[i]);
				if (!r) continue;
				if (i === current) curRange = r;
				else all.push(r);
			}
			// 현재 항목은 따로 칠한다 — 나머지와 색이 달라야 어디 있는지 보인다.
			CSS.highlights.set(ALL, new Highlight(...all));
			if (curRange) CSS.highlights.set(CUR, new Highlight(curRange));
			else CSS.highlights.delete(CUR);
		} catch {
			/* Range 가 이미 무효 — 다음 재검색에서 낫는다. */
		}
	}

	/** 현재 항목이 화면 밖이면 스크롤. 칠하기가 안 되는 환경에서도 이건 된다. */
	function revealCurrent() {
		if (current < 0) return;
		const r = matchToRange(segments, matches[current]);
		if (!r) return;
		const rect = r.getBoundingClientRect();
		// 높이 0 이면 아직 레이아웃 전이거나 숨겨진 것 — 건드리지 않는다.
		if (rect.width === 0 && rect.height === 0) return;
		const pad = 80;
		const above = rect.top < pad;
		const below = rect.bottom > window.innerHeight - pad;
		if (!above && !below) return;
		// Range 는 scrollIntoView 가 없다 — 시작 노드의 부모 요소로 대신한다.
		const anchor =
			r.startContainer.nodeType === Node.ELEMENT_NODE
				? (r.startContainer as Element)
				: r.startContainer.parentElement;
		anchor?.scrollIntoView({ block: 'center', behavior: 'auto' });
	}

	function recompute(keepCurrent = false) {
		const target = root ?? document.querySelector('main');
		segments = target ? buildSegments(target) : [];
		const next = findMatches(segments, query);
		const prev = current;
		matches = next;
		if (next.length === 0) current = -1;
		else if (keepCurrent && prev >= 0) current = Math.min(prev, next.length - 1);
		else current = 0;
		paint();
		revealCurrent();
	}

	function step(delta: number) {
		if (matches.length === 0) return;
		current = stepIndex(current, matches.length, delta);
		paint();
		revealCurrent();
	}

	function close() {
		clearHighlights();
		onclose();
	}

	function onKeyDown(e: KeyboardEvent) {
		if (e.key === 'Escape') {
			e.preventDefault();
			close();
			return;
		}
		if (e.key === 'Enter') {
			e.preventDefault();
			step(e.shiftKey ? -1 : 1);
		}
	}

	// 질의가 바뀌면 다시 찾는다.
	//
	// **`$effect` 로 하면 안 된다.** `recompute()` 는 `matches`/`current` 를
	// 쓰는데, 그 안의 `paint()`/`revealCurrent()` 가 같은 값을 읽는다. 이펙트가
	// 자기가 쓴 상태를 의존성으로 잡아 무한히 다시 돈다
	// (`effect_update_depth_exceeded` 로 실제로 터졌다). 입력 이벤트에서 직접
	// 부르면 그런 되먹임이 없다.
	function onInput(e: Event) {
		query = (e.currentTarget as HTMLInputElement).value;
		recompute();
	}

	/**
	 * 내용이 바뀌면(댓글 로딩, 접기/펼치기 등) 잡아 둔 Range 가 낡는다.
	 * 자주 도는 것이 아니므로 디바운스로 충분하다.
	 */
	let reTimer: ReturnType<typeof setTimeout> | null = null;
	$effect(() => {
		const target = root ?? document.querySelector('main');
		if (!target || typeof MutationObserver === 'undefined') return;
		const mo = new MutationObserver(() => {
			if (reTimer) clearTimeout(reTimer);
			reTimer = setTimeout(() => recompute(true), 200);
		});
		mo.observe(target, { childList: true, subtree: true, characterData: true });
		return () => {
			mo.disconnect();
			if (reTimer) clearTimeout(reTimer);
		};
	});

	// 열리면 곧바로 입력할 수 있어야 한다.
	$effect(() => {
		inputEl?.focus();
		inputEl?.select();
	});

	onDestroy(clearHighlights);
</script>

<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<div class="find-bar" role="search" {...{ 'data-find-skip': '' }} onkeydown={onKeyDown}>
	<input
		bind:this={inputEl}
		value={query}
		oninput={onInput}
		type="text"
		class="find-input"
		placeholder={t('find.placeholder', $locale)}
		aria-label={t('find.placeholder', $locale)}
		spellcheck="false"
		autocomplete="off"
	/>
	<span class="find-count" aria-live="polite">
		{#if query.length === 0}
			&nbsp;
		{:else if matches.length === 0}
			{t('find.none', $locale)}
		{:else}
			{current + 1}/{matches.length}
		{/if}
	</span>
	<button
		class="find-btn"
		onclick={() => step(-1)}
		disabled={matches.length === 0}
		title={t('find.prev', $locale)}
		aria-label={t('find.prev', $locale)}>↑</button
	>
	<button
		class="find-btn"
		onclick={() => step(1)}
		disabled={matches.length === 0}
		title={t('find.next', $locale)}
		aria-label={t('find.next', $locale)}>↓</button
	>
	<button
		class="find-btn"
		onclick={close}
		title={t('find.close', $locale)}
		aria-label={t('find.close', $locale)}>✕</button
	>
</div>

<style>
	.find-bar {
		position: fixed;
		top: calc(var(--titlebar-h, 0px) + var(--nav-h, 3.25rem) + 0.5rem);
		right: 1rem;
		z-index: 60;
		display: flex;
		align-items: center;
		gap: 0.35rem;
		padding: 0.35rem 0.5rem;
		background: var(--bg-elevated);
		border: var(--bw) solid var(--border);
		border-radius: var(--r-lg);
		box-shadow: 0 4px 16px rgb(0 0 0 / 18%);
	}
	.find-input {
		width: 12rem;
		padding: 0.2rem 0.4rem;
		background: var(--bg);
		border: var(--bw) solid var(--border);
		border-radius: var(--r-sm);
		color: var(--text);
		font: inherit;
		font-size: 0.85rem;
		outline: none;
	}
	.find-input:focus-visible {
		border-color: var(--accent);
	}
	.find-count {
		min-width: 4.5ch;
		text-align: center;
		color: var(--text-muted);
		font-size: 0.78rem;
		font-variant-numeric: tabular-nums;
	}
	.find-btn {
		padding: 0.15rem 0.4rem;
		background: transparent;
		border: var(--bw) solid transparent;
		border-radius: var(--r-sm);
		color: var(--text-muted);
		font: inherit;
		font-size: 0.85rem;
		cursor: pointer;
	}
	.find-btn:hover:not(:disabled) {
		background: var(--bg-subtle);
		color: var(--text);
	}
	.find-btn:disabled {
		opacity: 0.4;
		cursor: default;
	}
</style>
