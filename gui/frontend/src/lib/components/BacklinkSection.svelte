<!--
  REQ-008: 문서 하단 "이 문서를 참조하는 문서" 섹션.

  cross-link 는 단방향이다 — A 가 `[[B]]` 를 걸면 A→B 는 본문만 보면 알 수
  있지만, B 를 보고 있을 때 누가 자신을 참조하는지는 알 수 없었다. 설계 결정이
  여러 퀘스트에 흩어져 있을 때 역방향 추적이 안 되면 맥락을 놓친다.

  색인(index.db `doc_links`)은 reindex 가 만든다. 여기서는 조회만 한다.
-->
<script lang="ts">
	import { backlinksApi, type Backlink, type BacklinkKind } from '$lib/api/backlinks';
	import { locale, t } from '$lib/stores/locale';
	import { KIND_LABEL, refHref } from '$lib/stores/questIndex';
	import { reindexBump } from '$lib/stores/reindex';

	let { kind, id }: { kind: BacklinkKind; id: string } = $props();

	let entries = $state<Backlink[]>([]);
	let loading = $state(true);
	let error = $state<string | null>(null);
	// REQ-008 후속: 접기 토글. 변경 이력 섹션(REQ-007)과 **같은 정책** — 기본
	// 접힘, 상태는 영속하지 않는다. 같은 상세 페이지의 형제 섹션이라 조작감이
	// 달라지면 안 된다.
	let collapsed = $state(true);
	function toggleCollapsed() {
		collapsed = !collapsed;
	}

	// REQ-004 에서 지적된 stale-async 문제를 여기서는 처음부터 막는다 —
	// 대상이 바뀐 뒤 이전 요청이 늦게 도착해 남의 목록을 덮어쓰지 않도록,
	// 결과를 반영하기 전에 "아직 내가 현재 대상인가" 를 다시 확인한다.
	// (`quests/[id]/+page.svelte` 의 slug 가드와 같은 방식.)
	$effect(() => {
		const wantKind = kind;
		const wantId = id;
		// reindex 후에는 색인이 바뀌므로 다시 읽는다.
		void $reindexBump;
		if (!wantId) return;
		loading = true;
		error = null;
		backlinksApi
			.list(wantKind, wantId)
			.then((list) => {
				if (wantKind !== kind || wantId !== id) return;
				entries = list;
			})
			.catch((e) => {
				if (wantKind !== kind || wantId !== id) return;
				error = e instanceof Error ? e.message : t('backlinks.loadFailed', $locale);
			})
			.finally(() => {
				if (wantKind !== kind || wantId !== id) return;
				loading = false;
			});
	});
</script>

<!-- 참조가 없으면 섹션 자체를 그리지 않는다 — 대부분의 문서에는 backlink 가
     없고, 빈 섹션이 상세 페이지마다 쌓이면 소음이 된다. -->
{#if loading}
	<!-- 로딩 중에도 자리를 잡지 않는다(대개 곧 비어 있음이 판명된다). -->
{:else if error}
	<section class="bl">
		<h2 class="bl-title">{t('backlinks.title', $locale)}</h2>
		<p class="bl-state error">{error}</p>
	</section>
{:else if entries.length > 0}
	<section class="bl">
		<div class="section-head">
			<button
				type="button"
				class="section-toggle"
				onclick={toggleCollapsed}
				aria-expanded={!collapsed}
				title={collapsed
					? t('backlinks.expand', $locale)
					: t('backlinks.collapse', $locale)}
			>
				<span class="toggle-icon" class:collapsed>▼</span>
				<h2 class="bl-title">{t('backlinks.title', $locale)}</h2>
			</button>
			<span class="bl-count">{entries.length}</span>
		</div>
		{#if !collapsed}
		<ul class="bl-list">
			{#each entries as e (e.kind + ':' + e.id)}
				<li class="bl-item {e.kind}">
					<a href={refHref(e.id, e.kind, e.id)}>
						<span class="bl-kind {e.kind}">{KIND_LABEL[e.kind]}</span>
						<span class="bl-id">{e.id}</span>
						{#if e.title && e.title !== e.id}
							<span class="bl-t">{e.title}</span>
						{/if}
					</a>
				</li>
			{/each}
		</ul>
		{/if}
	</section>
{/if}

<style>
	.bl {
		margin-bottom: 1.5rem;
	}
	/* REQ-008 후속: 헤더 구조를 변경 이력 섹션과 맞춘다 — 토글 버튼(아이콘+제목)
	   + 카운트 뱃지. 같은 상세 페이지의 형제 섹션이라 모양이 달라지면 안 된다. */
	.section-head {
		display: flex;
		align-items: center;
		gap: 0.5rem;
		margin-bottom: 0.5rem;
	}
	.section-toggle {
		display: flex;
		align-items: center;
		gap: 0.4rem;
		background: none;
		border: none;
		padding: 0;
		cursor: pointer;
		color: inherit;
		font: inherit;
	}
	.toggle-icon {
		font-size: 0.65rem;
		color: var(--text-muted);
		transition: transform 0.12s;
		display: inline-block;
	}
	.toggle-icon.collapsed {
		transform: rotate(-90deg);
	}
	.bl-title {
		font-size: 0.8rem;
		font-weight: 600;
		color: var(--text-muted);
		text-transform: uppercase;
		letter-spacing: 0.05em;
	}
	/* 변경 이력 섹션의 `.qh-count` 와 **같은 공식** — 두 섹션이 나란히 있어
	   카운트 모양이 다르면 바로 눈에 띈다. */
	.bl-count {
		font-size: 0.72rem;
		color: var(--text-faint);
		padding: 0.05rem 0.4rem;
		border-radius: 10px;
		background: var(--bg-subtle);
	}
	.bl-list {
		list-style: none;
		margin: 0;
		padding: 0;
	}
	.bl-item a {
		display: flex;
		gap: 0.6rem;
		align-items: baseline;
		padding: 0.35rem 0;
		border-bottom: 1px solid var(--border);
		text-decoration: none;
		color: var(--text);
		font-size: 0.83rem;
		min-width: 0;
	}
	.bl-item a:hover .bl-t {
		text-decoration: underline;
	}
	.bl-kind {
		flex: none;
		font-size: 0.68rem;
		font-weight: 600;
		border-radius: 4px;
		padding: 0.1rem 0.35rem;
		color: var(--text-muted);
		background: color-mix(in srgb, var(--text-muted) 12%, transparent);
	}
	/* 종류별 색 — 검색 팔레트(DEV-362)와 같은 토큰을 쓴다. */
	/* 종류 색을 항목 단위로 한 번만 정하고, 종류 칩과 slug pill 이 함께 쓴다 —
	   같은 색을 두 곳에 따로 적으면 갈라진다(DEV-362 에서 겪은 문제). */
	.bl-item.quest {
		--bl-c: var(--accent);
	}
	.bl-item.campaign {
		--bl-c: var(--hl-pre);
	}
	.bl-item.rule {
		--bl-c: var(--success);
	}
	.bl-item.book {
		--bl-c: var(--warning);
	}
	.bl-kind.quest {
		color: var(--accent);
		background: color-mix(in srgb, var(--accent) 14%, transparent);
	}
	.bl-kind.campaign {
		color: var(--hl-pre);
		background: color-mix(in srgb, var(--hl-pre) 14%, transparent);
	}
	.bl-kind.rule {
		color: var(--success);
		background: color-mix(in srgb, var(--success) 14%, transparent);
	}
	.bl-kind.book {
		color: var(--warning);
		background: color-mix(in srgb, var(--warning) 14%, transparent);
	}
	/* slug 은 **pill** 이어야 한다 — 보드 노드(`.node-pill.mono`)와 같은 공식.
	   맨 텍스트로 두면 같은 식별자가 화면마다 다른 것으로 읽힌다(DEV-362 와
	   같은 이유). 종류별 색은 `.bl-kind` 가 이미 쓰므로 여기선 `--bl-c` 로
	   받아 같은 색을 공유한다. */
	.bl-id {
		flex: none;
		display: inline-flex;
		align-items: center;
		height: 17px;
		padding: 0 7px;
		box-sizing: border-box;
		border-radius: 9px;
		font-family: 'SFMono-Regular', Consolas, monospace;
		font-size: 10px;
		font-weight: 600;
		line-height: 1;
		white-space: nowrap;
		color: var(--bl-c, var(--accent));
		background: color-mix(in srgb, var(--bl-c, var(--accent)) 16%, transparent);
		border: 1px solid color-mix(in srgb, var(--bl-c, var(--accent)) 55%, transparent);
	}
	.bl-t {
		color: var(--text);
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.bl-state {
		font-size: 0.83rem;
		color: var(--text-muted);
	}
	.bl-state.error {
		color: var(--danger);
	}
</style>
