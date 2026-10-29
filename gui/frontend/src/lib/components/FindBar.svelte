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
	// DEV-454: 글자 아이콘 대신 SVG.
	import Icon from './Icon.svelte';
	import { onDestroy, onMount } from 'svelte';
	import { locale, t } from '$lib/stores/locale';
	import { findQuery } from '$lib/stores/findQuery';
	import {
		buildSegments,
		findMatches,
		matchToRange,
		stepIndex,
		supportsHighlightApi,
		type Segment,
		type Match
	} from '$lib/utils/find-in-page';

	// DEV-467: `initial` — 열자마자 찾을 검색어(검색 팔레트에서 연 문서). 문서가 아직 그려지는 중이어도
	// 아래 MutationObserver 가 다시 찾고, 처음 찾았을 때 그리로 간다.
	let {
		root,
		onclose,
		initial = ''
	}: { root: HTMLElement | null; onclose: () => void; initial?: string } = $props();

	let query = $state('');
	let matches = $state<Match[]>([]);
	let current = $state(-1);
	let inputEl = $state<HTMLInputElement | undefined>(undefined);

	let segments: Segment[] = [];
	const canHighlight = supportsHighlightApi();

	/** 강조 이름 — `global.css` 의 `::highlight()` 와 짝이다. */
	const ALL = 'og-find';
	const CUR = 'og-find-current';

	/**
	 * 등록해 둔 Highlight 객체. 지울 때 **레지스트리에서 빼는 것만으로는
	 * 부족한** 환경이 있어서 참조를 들고 있는다(BUG-268).
	 */
	let painted: { name: string; hl: Highlight }[] = [];

	/**
	 * BUG-268: 찾기 창을 닫아도 강조가 남았다(admin 보고 — 노란색·파란색 둘 다).
	 *
	 * `CSS.highlights.delete(name)` 는 분명히 불리고 있었다. jsdom 대역으로
	 * 확인해도 닫기·Escape·언마운트 세 경로 모두 delete 가 나간다. 그런데도
	 * 화면에 남는다면 **레지스트리에서 빠져도 다시 그리지 않는** 것이다
	 * (WebKit 계열에서 알려진 무효화 문제).
	 *
	 * 그래서 지우는 순서를 바꾼다: 먼저 Highlight 자체를 **비우고**(Range 를
	 * 다 빼면 그 자리는 확실히 다시 그려진다) 그다음 레지스트리에서 뺀다.
	 * 어느 쪽이 듣든 결과는 같고, 남아 있던 Range 참조도 함께 놓아준다.
	 */
	function clearHighlights() {
		if (!canHighlight) return;
		for (const { hl } of painted) {
			try {
				hl.clear();
			} catch {
				/* clear 가 없는 구형 구현 — 아래 delete 로 간다. */
			}
		}
		painted = [];
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
			// BUG-268: **새로 칠하기 전에 옛 것을 비운다.**
			//
			// 닫을 때만이 아니라 **질의를 바꿀 때도** 옛 강조가 남았다(admin 재확인).
			// `CSS.highlights.set(name, 새것)` 으로 덮어써도 WebKit 은 옛 Range 가
			// 있던 자리를 다시 그리지 않는다. 레지스트리 교체가 아니라 Highlight
			// 자체를 비우는 것이 확실한 무효화다.
			for (const { hl } of painted) {
				try {
					hl.clear();
				} catch {
					/* clear 가 없는 구형 구현 — set 으로 덮어쓰는 데 기댄다. */
				}
			}
			// 현재 항목은 따로 칠한다 — 나머지와 색이 달라야 어디 있는지 보인다.
			painted = [];
			const allHl = new Highlight(...all);
			CSS.highlights.set(ALL, allHl);
			painted.push({ name: ALL, hl: allHl });
			if (curRange) {
				const curHl = new Highlight(curRange);
				CSS.highlights.set(CUR, curHl);
				painted.push({ name: CUR, hl: curHl });
			} else {
				CSS.highlights.delete(CUR);
			}
		} catch {
			/* Range 가 이미 무효 — 다음 재검색에서 낫는다. */
		}
	}

	/**
	 * 현재 항목이 화면 밖이면 스크롤. 칠하기가 안 되는 환경에서도 이건 된다.
	 *
	 * `paint()` 와 같은 이유로 **던지지 않는다** — 낡은 Range 의 기하를 묻는
	 * 것은 실패할 수 있고, 스크롤이 한 번 안 되는 것보다 예외가 새는 쪽이
	 * 나쁘다(`Range.getBoundingClientRect` 가 없는 환경도 있다 — jsdom).
	 */
	function revealCurrent() {
		try {
			revealCurrentInner();
		} catch {
			/* 기하를 못 구했다 — 이동만 건너뛴다. */
		}
	}

	function revealCurrentInner() {
		if (current < 0) return;
		const r = matchToRange(segments, matches[current]);
		if (!r) return;
		if (typeof r.getBoundingClientRect !== 'function') return;
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

	/**
	 * `keepCurrent` 는 내용이 바뀌어 다시 찾는 경로(아래 MutationObserver)다 — 그때는 **칠하기만 하고
	 * 스크롤하지 않는다**(BUG-381). 화면에 따라 스크롤만 해도 DOM 이 바뀌는데(지연 렌더 등), 그때마다
	 * 찾은 곳으로 다시 끌고 가서 사용자가 스크롤할 수 없었다. 스크롤은 질의를 바꾸거나 넘길 때만.
	 */
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
		// 보이는 곳에 없다가 접힌 댓글이 펼쳐져 처음 찾았을 때(BUG-382)는 그리로 간다.
		if (!keepCurrent || prev < 0) revealCurrent();
	}

	function step(delta: number) {
		if (matches.length === 0) return;
		current = stepIndex(current, matches.length, delta);
		paint();
		revealCurrent();
	}

	function close() {
		clearHighlights();
		findQuery.set('');
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

	/**
	 * BUG-385: **찾기 칸 밖에서도 Esc 로 닫는다**(admin: 찾기 칸에 포커스가 있을 때만 먹었다).
	 * 본문을 눌러 읽다 보면 포커스는 이미 찾기 칸을 떠나 있다.
	 *
	 * 다른 것이 먼저 Esc 를 쓴 경우는 건드리지 않는다 — 이미 처리한 Esc(`defaultPrevented`,
	 * 찾기 칸 자신도 여기 해당), 한글 조합 중인 Esc, 모달 · 팔레트가 떠 있을 때(그 창을 닫는
	 * Esc 다).
	 */
	function onWindowKeyDown(e: KeyboardEvent) {
		if (e.key !== 'Escape' || e.defaultPrevented || e.isComposing) return;
		if (document.querySelector('[role="dialog"], [role="alertdialog"]')) return;
		e.preventDefault();
		close();
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
		// BUG-382: 접힌 댓글이 검색어가 든 것을 펼친다 — 펼쳐지면 아래 MutationObserver 가 다시 찾는다.
		findQuery.set(query);
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

	onMount(() => {
		if (!initial) return;
		query = initial;
		findQuery.set(initial);
		recompute();
	});

	// 열리면 곧바로 입력할 수 있어야 한다.
	$effect(() => {
		inputEl?.focus();
		inputEl?.select();
	});

	onDestroy(() => {
		clearHighlights();
		findQuery.set('');
	});
</script>

<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<svelte:window onkeydown={onWindowKeyDown} />

<div class="find-bar" role="search" {...{ 'data-find-skip': '' }} onkeydown={onKeyDown}>
	<input
		bind:this={inputEl}
		value={query}
		oninput={onInput}
		type="text"
		class="input find-input"
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
		aria-label={t('find.prev', $locale)}><Icon name="up" size="1em" /></button
	>
	<button
		class="find-btn"
		onclick={() => step(1)}
		disabled={matches.length === 0}
		title={t('find.next', $locale)}
		aria-label={t('find.next', $locale)}><Icon name="down" size="1em" /></button
	>
	<button
		class="find-btn"
		onclick={close}
		title={t('find.close', $locale)}
		aria-label={t('find.close', $locale)}><Icon name="close" size="1em" /></button
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
		gap: var(--sp-6);
		padding: var(--sp-6) var(--sp-8);
		background: var(--bg-elevated);
		border: var(--bw) solid var(--border);
		border-radius: var(--r-lg);
		box-shadow: var(--shadow-sm);
	}
	.find-input {
		width: 12rem;
		font: inherit;
	}
	.find-count {
		min-width: 4.5ch;
		text-align: center;
		color: var(--text-muted);
		font-size: var(--fs-sm);
		font-variant-numeric: tabular-nums;
	}
	.find-btn {
		padding: var(--sp-2) var(--sp-6);
		background: transparent;
		border: var(--bw) solid transparent;
		border-radius: var(--r-sm);
		color: var(--text-muted);
		font: inherit;
		font-size: var(--fs-md);
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
