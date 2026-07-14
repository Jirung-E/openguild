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
	import { goto, afterNavigate } from '$app/navigation';
	import { questsApi } from '$lib/api/quests';
	import { campaignsApi } from '$lib/api/campaigns';
	import { rulesApi } from '$lib/api/rules';
	import { libraryApi } from '$lib/api/library';
	import MarkdownView from './MarkdownView.svelte';
	// 크로스링크(`[[kind:ID]]`)와 동일한 네임스페이스 별칭 — 검색 범위 좁히기.
	import { KIND_ALIASES, KIND_LABEL } from '$lib/stores/questIndex';
	// DEV-255: 결과 열기 방식(미리보기/자식윈도우/페이지이동) 선택 — 공용 헬퍼.
	import { openInWindow, openInPage } from '$lib/utils/open-item';

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
	// DEV-255 버그 수정: 방향키로 선택이 화면 밖으로 나가도 스크롤 안 되던 문제
	// — 선택 행을 scrollIntoView 하기 위한 목록 컨테이너 참조.
	let rowsEl = $state<HTMLDivElement | null>(null);

	// 미리보기 상태.
	let preview = $state<Item | null>(null);
	let previewBody = $state('');
	let previewLoading = $state(false);
	let previewH = $state(220);

	onMount(() => {
		inputEl?.focus();
		void loadAll();
	});

	// DEV-255 회귀 수정(2차): 위 onWindowClick 을 `<svelte:window onclick>` 으로
	// 붙이면 컴포넌트 생성 즉시(=같은 tick) 리스너가 등록된다. 그런데 팔레트를
	// 여는 그 클릭(타이틀바의 검색 pill) 자체가 아직 window 까지 버블링 중이던
	// 이벤트라, 새로 등록된 리스너가 그 "여는 클릭"에도 반응해 열리자마자 다시
	// 닫혀버렸다(afterNavigate 'enter' 버그와 동일 계열 — "검색 팔레트 안
	// 열림" 재현). 리스너 등록을 한 tick 미뤄(setTimeout 0) 여는 클릭의 버블링이
	// 다 끝난 뒤에만 붙는다.
	onMount(() => {
		const id = setTimeout(() => {
			window.addEventListener('click', onWindowClick);
		}, 0);
		return () => {
			clearTimeout(id);
			window.removeEventListener('click', onWindowClick);
		};
	});

	// DEV-255 버그 수정: selIndex 가 바뀔 때마다(방향키 이동) 해당 행이 보이도록
	// 스크롤. rowsEl.children 순서는 filtered 순서와 항상 일치.
	$effect(() => {
		if (preview || !rowsEl) return;
		const idx = selIndex;
		const el = rowsEl.children[idx] as HTMLElement | undefined;
		el?.scrollIntoView({ block: 'nearest' });
	});

	// DEV-255 버그 수정: 타이틀바(메뉴 버튼 포함)를 눌러도 팔레트가 안 꺼지던
	// 문제 — 기존 backdrop 은 titlebar 영역을 제외(inset: titlebar-h)해서
	// 그 위 클릭이 안 잡혔다. window 레벨로 팔레트 바깥 클릭을 감지해 닫는다.
	function onWindowClick(e: MouseEvent) {
		const t = e.target as HTMLElement;
		if (!t.closest('.palette')) onclose();
	}

	// DEV-255 버그 수정: 팔레트가 열린 채로 뒤로/앞으로가기(타이틀바 버튼·
	// 마우스 사이드버튼·단축키 등 어떤 경로든)가 되면 팔레트가 남아있던 문제
	// — 어떤 이유로든 라우트가 바뀌면 팔레트를 닫는다.
	// DEV-255 회귀 수정: `afterNavigate` 는 "컴포넌트가 mount 될 때도" 한 번
	// 호출된다(SvelteKit 문서: "runs ... when the current component mounts,
	// and also whenever we navigate") — 그 첫 호출(type === 'enter')까지
	// onclose() 를 태워서 팔레트가 열리자마자 닫혀버렸다(검색 팔레트 안 열림
	// 버그). 실제 라우트 전환(mount 이후)만 걸러서 닫는다.
	afterNavigate((nav) => {
		if (nav.type === 'enter') return;
		onclose();
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
					// BUG(발견 2026-07-14): 이전엔 q.id(숫자 row id) 사용 — [id] 라우트는
					// getBySlug(quest_id 문자열) 로 조회해 "이동"/"자식창" 모두 빈 화면.
					href: `/quests/${q.quest_id}`,
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
		// 결과 개수 상한 없음 — 단순 행이라 문서가 많아도 렌더 부담 미미, 영역 스크롤.
		if (!term) return pool;
		if (term.startsWith('#')) {
			const tag = term.slice(1).toLowerCase();
			if (!tag) return pool.filter((i) => i.tags.length > 0);
			return pool.filter((i) => i.tags.some((t) => t.toLowerCase().includes(tag)));
		}
		const q = term.toLowerCase();
		return pool.filter(
			(i) =>
				i.title.toLowerCase().includes(q) ||
				i.label.toLowerCase().includes(q) ||
				i.tags.some((t) => t.toLowerCase().includes(q))
		);
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

	// DEV-255: 페이지로 이동 — 현재 창 라우팅 + 팔레트 닫기(기존 동작 유지).
	function goItem(it: Item) {
		openInPage(it.href);
		onclose();
	}

	// DEV-255: 항목별 새 창 — 팔레트는 열어둔 채 유지(여러 개 동시에 띄우고
	// 비교하는 사용 흐름 지원, AskUserQuestion 결정).
	function windowItem(it: Item) {
		void openInWindow(it.href, displayName(it));
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

<!-- DEV-255 버그 수정: 팔레트 바깥(타이틀바 포함) 아무 데나 클릭해도 닫힘.
     리스너는 위 onMount 에서 한 tick 지연 등록(여는 클릭과의 충돌 방지). -->

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
		<div class="rows" bind:this={rowsEl}>
			{#if loading}
				<div class="empty">불러오는 중…</div>
			{:else if filtered.length === 0}
				<div class="empty">검색 결과 없음</div>
			{:else}
				{#each filtered as it, i (it.kind + it.label)}
					<!-- DEV-255: 행 = 라벨(기본 클릭 = 미리보기) + 열기 방식 아이콘 3개(항상 노출). -->
					<!-- svelte-ignore a11y_no_static_element_interactions -->
					<div class="row" class:sel={i === selIndex} onmouseenter={() => (selIndex = i)}>
						<button
							class="row-main"
							onclick={() => openPreview(it)}
							title={displayName(it) + (it.tags.length ? '  ' + it.tags.map((t) => '#' + t).join(' ') : '')}
						>
							<span class="ptype {it.kind}">{KIND_LABEL[it.kind]}</span>
							<span class="ptitle">{displayName(it)}</span>
							{#if it.tags.length}
								<span class="ptags">{it.tags.map((t) => '#' + t).join(' ')}</span>
							{/if}
						</button>
						<div class="row-actions">
							<button class="row-act" onclick={() => openPreview(it)} title="미리보기" aria-label="미리보기">
								<svg width="13" height="13" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.3" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
									<path d="M1.5 8S4 3.5 8 3.5 14.5 8 14.5 8 12 12.5 8 12.5 1.5 8 1.5 8Z" />
									<circle cx="8" cy="8" r="1.7" />
								</svg>
							</button>
							<button class="row-act" onclick={() => windowItem(it)} title="새 창으로 열기" aria-label="새 창으로 열기">
								<svg width="13" height="13" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.3" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
									<path d="M6 3H3.3a.8.8 0 0 0-.8.8v8.4a.8.8 0 0 0 .8.8h8.4a.8.8 0 0 0 .8-.8V10" />
									<path d="M9 2.5h4.5V7" />
									<path d="M13.5 2.5 7.2 8.8" />
								</svg>
							</button>
							<button class="row-act" onclick={() => goItem(it)} title="페이지로 이동" aria-label="페이지로 이동">
								<svg width="13" height="13" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
									<path d="M2.5 8h11" />
									<path d="M9 4.2 12.8 8 9 11.8" />
								</svg>
							</button>
						</div>
					</div>
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
			<!-- DEV-255: 미리보기에서도 자식윈도우/페이지이동으로 전환 가능. -->
			<button class="dp-btn" onclick={() => preview && windowItem(preview)}>새 창으로 열기</button>
			<button class="dp-btn primary" onclick={() => preview && goItem(preview)}>페이지로 이동 →</button>
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
		color: var(--hl-pre);
		background: color-mix(in srgb, var(--hl-pre) 14%, transparent);
	}
	.scope-chip.rule {
		color: var(--success);
		background: color-mix(in srgb, var(--success) 14%, transparent);
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
	/* DEV-255: 행 = row-main(라벨, 기본 클릭 = 미리보기) + row-actions(열기 방식
	   아이콘 3개). 이전엔 행 전체가 하나의 <button> 이었으나 중첩 버튼이
	   필요해져 컨테이너를 div 로 변경. */
	.row {
		display: flex;
		align-items: stretch;
		width: 100%;
	}
	.row.sel {
		background: var(--nav-hover-bg);
	}
	.row-main {
		display: flex;
		align-items: center;
		gap: 0.6rem;
		flex: 1;
		min-width: 0;
		padding: 0.45rem 0.8rem;
		font-size: 0.85rem;
		background: transparent;
		border: none;
		cursor: pointer;
		text-align: left;
	}
	.row-actions {
		flex: none;
		display: flex;
		align-items: center;
		gap: 0.1rem;
		padding: 0 0.5rem 0 0;
	}
	.row-act {
		display: inline-flex;
		align-items: center;
		justify-content: center;
		width: 22px;
		height: 22px;
		color: var(--text-faint);
		background: transparent;
		border: none;
		border-radius: 4px;
		cursor: pointer;
	}
	.row-act:hover {
		background: var(--nav-hover-bg);
		color: var(--text);
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
		color: var(--hl-pre);
		background: color-mix(in srgb, var(--hl-pre) 14%, transparent);
	}
	.ptype.rule {
		color: var(--success);
		background: color-mix(in srgb, var(--success) 14%, transparent);
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
		background: var(--btn-primary-bg);
		border-color: transparent;
		color: var(--btn-primary-text);
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
