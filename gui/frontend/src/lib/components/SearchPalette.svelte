<!--
  DEV-253: 전 문서 검색 팔레트 (vscode Quick Open 식).

  타이틀바 중앙의 길드 이름 pill 을 누르면 열린다. 길드의 모든 문서
  (퀘스트 / 캠페인 / 규칙 / 도서관)를 제목·식별자·태그로 검색한다.
  `#태그` 로 시작하면 태그 전용 검색.

  결과 선택(Enter / 클릭) 시 페이지 이동 없이 내용 미리보기 팝업을 띄우고,
  거기서 "페이지로 이동" 으로 실제 라우트 이동. 미리보기 본문은 아래
  가장자리 핸들로 세로 크기 조절 가능(기존 OverlayScrollbar 사용).

  검색은 클라이언트 사이드 — 각 타입의 기존 list API 를 병합해 필터.
  본문 미리보기는 선택 시점에 상세 API 로 지연 로드.
-->
<script lang="ts">
	import { onMount } from 'svelte';
	import { goto } from '$app/navigation';
	import { questsApi } from '$lib/api/quests';
	import { campaignsApi } from '$lib/api/campaigns';
	import { rulesApi } from '$lib/api/rules';
	import { libraryApi } from '$lib/api/library';
	import MarkdownView from './MarkdownView.svelte';
	// 크로스링크(`[[kind:ID]]`)와 동일한 네임스페이스 별칭 — 검색 범위 좁히기.
	import { KIND_ALIASES, KIND_LABEL } from '$lib/stores/questIndex';

	let { onclose }: { onclose: () => void } = $props();

	type Kind = 'quest' | 'campaign' | 'rule' | 'book';
	interface Item {
		kind: Kind;
		label: string; // "DEV-253" / "C-001" / rule slug / "BOOK-012"
		title: string;
		tags: string[];
		href: string;
		meta: string; // 상태 등 짧은 부가 정보
		load: () => Promise<string>; // 미리보기 본문(markdown) 지연 로더
	}

	let all = $state<Item[]>([]);
	let loading = $state(true);
	let query = $state('');
	let selIndex = $state(0);
	let inputEl = $state<HTMLInputElement | null>(null);

	// 미리보기 상태.
	let preview = $state<Item | null>(null);
	let previewBody = $state('');
	let previewLoading = $state(false);
	let previewH = $state(220);

	onMount(() => {
		inputEl?.focus();
		void loadAll();
	});

	async function loadAll() {
		loading = true;
		try {
			const [quests, camps, rules, books] = await Promise.all([
				questsApi.list().catch(() => []),
				campaignsApi.list().catch(() => []),
				rulesApi.list().catch(() => ({ entries: [] })),
				libraryApi.list().catch(() => [])
			]);
			const items: Item[] = [];
			for (const q of quests) {
				items.push({
					kind: 'quest',
					label: q.quest_id,
					title: q.title,
					tags: q.tags ?? [],
					href: `/quests/${q.id}`,
					meta: q.status_name_ko,
					load: async () => (await questsApi.get(q.id)).description ?? ''
				});
			}
			for (const c of camps) {
				items.push({
					kind: 'campaign',
					label: c.campaign_slug,
					title: c.title,
					tags: [],
					href: `/campaigns/${encodeURIComponent(c.campaign_slug)}`,
					meta: String(c.status),
					load: async () => (await campaignsApi.get(c.campaign_slug)).description ?? ''
				});
			}
			for (const r of rules.entries) {
				items.push({
					kind: 'rule',
					// 규칙은 slug 가 곧 식별자 — 별도 제목 없음(중복 표시 방지).
					label: r.slug,
					title: '',
					tags: r.tags ?? [],
					href: `/rules?slug=${encodeURIComponent(r.slug)}`,
					meta: '규칙',
					load: async () => r.content ?? ''
				});
			}
			for (const b of books) {
				items.push({
					kind: 'book',
					label: b.book_id,
					title: b.title,
					tags: b.tags ?? [],
					href: `/library?id=${encodeURIComponent(b.book_id)}`,
					meta: '도서관',
					load: async () => (await libraryApi.get(b.book_id)).body ?? ''
				});
			}
			all = items;
		} finally {
			loading = false;
		}
	}

	// 입력을 `namespace:` 접두 + 나머지 검색어로 분리. 크로스링크(`[[kind:ID]]`)와
	// 동일한 별칭 테이블 재사용: quest/q · campaign/c · rule/rules/r · book/library/lib.
	const parsed = $derived.by((): { kind: Kind | null; term: string } => {
		let raw = query.trim();
		const ci = raw.indexOf(':');
		if (ci > 0) {
			const prefix = raw.slice(0, ci).toLowerCase();
			const k = KIND_ALIASES[prefix];
			if (k) return { kind: k, term: raw.slice(ci + 1).trim() };
		}
		return { kind: null, term: raw };
	});

	// 활성 네임스페이스 범위 — 있으면 그 종류만 검색(범위 칩 표시용).
	const scopeKind = $derived(parsed.kind);

	const filtered = $derived.by(() => {
		const { kind, term } = parsed;
		const pool = kind ? all.filter((i) => i.kind === kind) : all;
		if (!term) return pool.slice(0, 50);
		if (term.startsWith('#')) {
			const tag = term.slice(1).toLowerCase();
			if (!tag) return pool.filter((i) => i.tags.length > 0).slice(0, 50);
			return pool.filter((i) => i.tags.some((t) => t.toLowerCase().includes(tag))).slice(0, 50);
		}
		const q = term.toLowerCase();
		return pool
			.filter(
				(i) =>
					i.title.toLowerCase().includes(q) ||
					i.label.toLowerCase().includes(q) ||
					i.tags.some((t) => t.toLowerCase().includes(q))
			)
			.slice(0, 50);
	});

	// 필터가 바뀌어 선택 index 가 범위를 벗어나면 리셋.
	$effect(() => {
		if (selIndex >= filtered.length) selIndex = 0;
	});

	async function openPreview(it: Item) {
		preview = it;
		previewLoading = true;
		previewBody = '';
		try {
			previewBody = await it.load();
			if (!previewBody.trim()) previewBody = '_(본문 없음)_';
		} catch {
			previewBody = '_미리보기를 불러오지 못했습니다._';
		} finally {
			previewLoading = false;
		}
	}

	function goItem(it: Item) {
		goto(it.href);
		onclose();
	}

	// 표시 이름 — 규칙처럼 title 이 비면 label 만(중복/후행 공백 방지).
	function displayName(it: Item): string {
		return it.title ? `${it.label} ${it.title}` : it.label;
	}

	function onKey(e: KeyboardEvent) {
		if (e.key === 'Escape') {
			e.preventDefault();
			if (preview) preview = null;
			else onclose();
			return;
		}
		if (preview) return;
		if (e.key === 'ArrowDown') {
			e.preventDefault();
			selIndex = Math.min(selIndex + 1, filtered.length - 1);
		} else if (e.key === 'ArrowUp') {
			e.preventDefault();
			selIndex = Math.max(selIndex - 1, 0);
		} else if (e.key === 'Enter') {
			e.preventDefault();
			const it = filtered[selIndex];
			if (it) void openPreview(it);
		}
	}

	// 미리보기 아래 가장자리 = 세로 크기 조절 핸들.
	function startResize(e: MouseEvent) {
		e.preventDefault();
		const startY = e.clientY;
		const startH = previewH;
		const onMove = (ev: MouseEvent) => {
			previewH = Math.max(90, Math.min(460, startH + (ev.clientY - startY)));
		};
		const onUp = () => {
			window.removeEventListener('mousemove', onMove);
			window.removeEventListener('mouseup', onUp);
			document.body.style.userSelect = '';
		};
		document.body.style.userSelect = 'none';
		window.addEventListener('mousemove', onMove);
		window.addEventListener('mouseup', onUp);
	}
</script>

<!-- 바깥 클릭 / Esc 로 닫기. backdrop 은 투명 — 콘텐츠를 어둡게 덮지 않음. -->
<div
	class="backdrop"
	role="button"
	tabindex="-1"
	aria-label="검색 닫기"
	onclick={onclose}
	onkeydown={(e) => e.key === 'Escape' && onclose()}
></div>

<div class="palette" role="dialog" aria-label="문서 검색">
	{#if !preview}
		<div class="input-wrap">
			{#if scopeKind}
				<span class="scope-chip {scopeKind}">{KIND_LABEL[scopeKind]}만</span>
			{/if}
			<input
				bind:this={inputEl}
				bind:value={query}
				onkeydown={onKey}
				placeholder="검색어 · 범위(rules: · quest: …) · #태그"
				spellcheck="false"
			/>
		</div>
		<div class="rows">
			{#if loading}
				<div class="empty">불러오는 중…</div>
			{:else if filtered.length === 0}
				<div class="empty">검색 결과 없음</div>
			{:else}
				{#each filtered as it, i (it.kind + it.label)}
					<button
						class="row"
						class:sel={i === selIndex}
						onmouseenter={() => (selIndex = i)}
						onclick={() => openPreview(it)}
						title={displayName(it) + (it.tags.length ? '  ' + it.tags.map((t) => '#' + t).join(' ') : '')}
					>
						<span class="ptype {it.kind}">{KIND_LABEL[it.kind]}</span>
						<span class="ptitle">{displayName(it)}</span>
						{#if it.tags.length}
							<span class="ptags">{it.tags.map((t) => '#' + t).join(' ')}</span>
						{/if}
					</button>
				{/each}
			{/if}
		</div>
	{:else}
		<div class="dp-head">
			<span class="ptype {preview.kind}">{KIND_LABEL[preview.kind]}</span>
			<span class="dp-title" title={displayName(preview)}>{displayName(preview)}</span>
			<button class="dp-x" onclick={() => (preview = null)} title="목록으로 (Esc)">✕</button>
		</div>
		<div class="dp-meta">
			<span>{preview.meta}</span>
			{#if preview.tags.length}
				<span class="tag">{preview.tags.map((t) => '#' + t).join(' ')}</span>
			{/if}
		</div>
		<div class="dp-body" style="height:{previewH}px">
			{#if previewLoading}
				<div class="empty">불러오는 중…</div>
			{:else}
				<MarkdownView source={previewBody} />
			{/if}
		</div>
		<div class="dp-foot">
			<button class="dp-btn" onclick={() => (preview = null)}>← 목록</button>
			<button class="dp-btn primary" onclick={() => goItem(preview)}>페이지로 이동 →</button>
		</div>
		<!-- svelte-ignore a11y_no_static_element_interactions -->
		<div class="dp-resize" onmousedown={startResize} title="아래로 드래그해 크기 조절"></div>
	{/if}
</div>

<style>
	.backdrop {
		position: fixed;
		inset: var(--titlebar-h, 0px) 0 0 0;
		z-index: 1190;
		background: transparent;
		border: none;
		cursor: default;
	}
	.palette {
		position: fixed;
		top: calc(var(--titlebar-h, 32px) + 2px);
		left: 50%;
		transform: translateX(-50%);
		width: min(560px, 62vw);
		z-index: 1200;
		background: var(--bg-elevated);
		border: 1px solid var(--border);
		border-radius: 8px;
		box-shadow: 0 10px 34px rgba(0, 0, 0, 0.45);
		overflow: hidden;
	}
	.input-wrap {
		display: flex;
		align-items: center;
		gap: 0.5rem;
		padding: 0 0.8rem;
		background: var(--bg-subtle);
		border-bottom: 1px solid var(--border);
	}
	.scope-chip {
		flex: none;
		font-size: 0.68rem;
		font-weight: 600;
		border-radius: 4px;
		padding: 0.1rem 0.4rem;
		color: var(--accent);
		background: color-mix(in srgb, var(--accent) 14%, transparent);
	}
	.scope-chip.campaign {
		color: #d2a8ff;
		background: color-mix(in srgb, #d2a8ff 14%, transparent);
	}
	.scope-chip.rule {
		color: #7ee787;
		background: color-mix(in srgb, #7ee787 14%, transparent);
	}
	.scope-chip.book {
		color: var(--warning);
		background: color-mix(in srgb, var(--warning) 14%, transparent);
	}
	input {
		flex: 1;
		width: 100%;
		padding: 0.55rem 0;
		font-size: 0.9rem;
		border: none;
		outline: none;
		background: transparent;
		color: var(--text-strong);
	}
	input::placeholder {
		color: var(--text-faint);
	}
	.rows {
		max-height: 340px;
		overflow-y: auto;
	}
	.empty {
		padding: 0.9rem;
		text-align: center;
		font-size: 0.82rem;
		color: var(--text-faint);
	}
	.row {
		display: flex;
		align-items: center;
		gap: 0.6rem;
		width: 100%;
		padding: 0.45rem 0.8rem;
		font-size: 0.85rem;
		background: transparent;
		border: none;
		cursor: pointer;
		text-align: left;
	}
	.row.sel {
		background: var(--nav-hover-bg);
	}
	.ptype {
		flex: none;
		min-width: 3.6rem;
		text-align: center;
		font-size: 0.68rem;
		font-weight: 600;
		border-radius: 4px;
		padding: 0.1rem 0.35rem;
	}
	/* 타입별 색 — QuestBoard / 문서 톤과 맞춤. */
	.ptype.quest {
		color: var(--accent);
		background: color-mix(in srgb, var(--accent) 14%, transparent);
	}
	.ptype.campaign {
		color: #d2a8ff;
		background: color-mix(in srgb, #d2a8ff 14%, transparent);
	}
	.ptype.rule {
		color: #7ee787;
		background: color-mix(in srgb, #7ee787 14%, transparent);
	}
	.ptype.book {
		color: var(--warning);
		background: color-mix(in srgb, var(--warning) 14%, transparent);
	}
	.ptitle {
		flex: 1;
		color: var(--text);
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.ptags {
		flex: none;
		font-size: 0.7rem;
		color: var(--text-faint);
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
		max-width: 40%;
	}
	/* ── 미리보기 ── */
	.dp-head {
		display: flex;
		align-items: center;
		gap: 0.6rem;
		padding: 0.55rem 0.8rem;
		border-bottom: 1px solid var(--border);
	}
	.dp-title {
		flex: 1;
		font-size: 0.9rem;
		font-weight: 600;
		color: var(--text-strong);
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.dp-x {
		flex: none;
		color: var(--text-muted);
		font-size: 0.8rem;
		background: none;
		border: none;
		cursor: pointer;
		padding: 0.2rem 0.35rem;
		border-radius: 4px;
	}
	.dp-x:hover {
		background: var(--nav-hover-bg);
		color: var(--text);
	}
	.dp-meta {
		display: flex;
		gap: 0.9rem;
		padding: 0.4rem 0.8rem;
		font-size: 0.72rem;
		color: var(--text-muted);
		border-bottom: 1px solid var(--border);
	}
	.dp-meta .tag {
		color: var(--accent);
	}
	.dp-body {
		padding: 0.5rem 0.8rem;
		overflow-y: auto;
		/* 스크롤바는 global.css 의 기존 커스텀 ::-webkit-scrollbar 규칙이
		   컨테이너 안쪽 오른쪽에 그린다(별도 처리 불필요). */
	}
	.dp-foot {
		display: flex;
		justify-content: flex-end;
		gap: 0.5rem;
		padding: 0.5rem 0.8rem;
		border-top: 1px solid var(--border);
	}
	.dp-btn {
		font-size: 0.78rem;
		padding: 0.3rem 0.7rem;
		border-radius: 6px;
		border: 1px solid var(--border);
		background: transparent;
		color: var(--text);
		cursor: pointer;
	}
	.dp-btn:hover {
		background: var(--nav-hover-bg);
	}
	.dp-btn.primary {
		background: var(--success-strong, #238636);
		border-color: transparent;
		color: #fff;
	}
	/* 아래 가장자리 = 세로 크기 조절 핸들. */
	.dp-resize {
		height: 7px;
		cursor: ns-resize;
		background: var(--bg-subtle);
		border-top: 1px solid var(--border);
		display: flex;
		align-items: center;
		justify-content: center;
	}
	.dp-resize::before {
		content: '';
		width: 34px;
		height: 3px;
		border-radius: 2px;
		background: var(--text-faint);
	}
	.dp-resize:hover::before {
		background: var(--text-muted);
	}
</style>
