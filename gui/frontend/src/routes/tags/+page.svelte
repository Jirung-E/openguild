<!--
  DEV-253: 태그 목록 페이지 — /tags.

  타이틀바 ☰ 메뉴의 진입점. 길드의 모든 문서(퀘스트 / 규칙 / 도서관)에서
  사용 중인 태그를 집계해 사용 횟수와 함께 나열한다. 태그를 펼치면 그 태그가
  달린 문서 목록이 나오고, 각 항목은 해당 페이지로 이동한다.

  태그 정의(/api/tag-defs)가 있으면 색을 따르고, 정의만 있고 사용처가 없는
  태그도 함께 보여준다(0 회).

  집계는 클라이언트 사이드 — 검색 팔레트와 동일하게 각 타입의 list API 병합.
-->
<script lang="ts">
	import { onMount } from 'svelte';
	import { questsApi } from '$lib/api/quests';
	import { rulesApi } from '$lib/api/rules';
	import { libraryApi } from '$lib/api/library';
	import { adminApi } from '$lib/api/admin';

	type Kind = 'quest' | 'rule' | 'book';
	interface Doc {
		kind: Kind;
		label: string;
		title: string;
		href: string;
	}
	interface TagRow {
		tag: string;
		color: string | null;
		docs: Doc[];
	}

	const KIND_LABEL: Record<Kind, string> = { quest: '퀘스트', rule: '규칙', book: '도서관' };

	let rows = $state<TagRow[]>([]);
	let loading = $state(true);
	let filter = $state('');
	let expanded = $state<string | null>(null);

	onMount(() => void load());

	async function load() {
		loading = true;
		try {
			const [quests, rules, books, defs] = await Promise.all([
				questsApi.list().catch(() => []),
				rulesApi.list().catch(() => ({ entries: [] })),
				libraryApi.list().catch(() => []),
				adminApi.listTagDefs().catch(() => [])
			]);

			const map = new Map<string, TagRow>();
			const ensure = (tag: string): TagRow => {
				let r = map.get(tag);
				if (!r) {
					r = { tag, color: null, docs: [] };
					map.set(tag, r);
				}
				return r;
			};

			for (const d of defs) ensure(d.slug).color = d.color;

			for (const q of quests) {
				for (const t of q.tags ?? []) {
					ensure(t).docs.push({
						kind: 'quest',
						label: q.quest_id,
						title: q.title,
						href: `/quests/${q.id}`
					});
				}
			}
			for (const r of rules.entries) {
				for (const t of r.tags ?? []) {
					ensure(t).docs.push({
						kind: 'rule',
						label: r.slug,
						title: r.slug,
						href: `/rules?slug=${encodeURIComponent(r.slug)}`
					});
				}
			}
			for (const b of books) {
				for (const t of b.tags ?? []) {
					ensure(t).docs.push({
						kind: 'book',
						label: b.book_id,
						title: b.title,
						href: `/library?id=${encodeURIComponent(b.book_id)}`
					});
				}
			}

			// 사용 많은 순 → 같은 수면 이름순.
			rows = [...map.values()].sort(
				(a, b) => b.docs.length - a.docs.length || a.tag.localeCompare(b.tag)
			);
		} finally {
			loading = false;
		}
	}

	const shown = $derived.by(() => {
		const q = filter.trim().toLowerCase();
		if (!q) return rows;
		return rows.filter((r) => r.tag.toLowerCase().includes(q));
	});

	function toggle(tag: string) {
		expanded = expanded === tag ? null : tag;
	}
</script>

<div class="wrap">
	<header>
		<h1>태그 목록</h1>
		<span class="count">{rows.length}개 태그</span>
		<input class="filter" bind:value={filter} placeholder="태그 검색…" spellcheck="false" />
	</header>

	{#if loading}
		<p class="empty">불러오는 중…</p>
	{:else if shown.length === 0}
		<p class="empty">태그 없음</p>
	{:else}
		<ul class="tags">
			{#each shown as r (r.tag)}
				<li class="tag-item">
					<button class="tag-head" onclick={() => toggle(r.tag)} aria-expanded={expanded === r.tag}>
						<span
							class="chip"
							style={r.color ? `--chip:${r.color}` : ''}
							class:defined={!!r.color}>#{r.tag}</span
						>
						<span class="usage">{r.docs.length}</span>
						<span class="chev" class:open={expanded === r.tag}>›</span>
					</button>
					{#if expanded === r.tag}
						<ul class="docs">
							{#if r.docs.length === 0}
								<li class="doc-empty">사용처 없음 (정의만 존재)</li>
							{:else}
								{#each r.docs as d (d.kind + d.label)}
									<li>
										<a class="doc" href={d.href}>
											<span class="dtype {d.kind}">{KIND_LABEL[d.kind]}</span>
											<span class="dtitle">{d.label} {d.title}</span>
										</a>
									</li>
								{/each}
							{/if}
						</ul>
					{/if}
				</li>
			{/each}
		</ul>
	{/if}
</div>

<style>
	.wrap {
		max-width: var(--content-max-width, 1100px);
		margin: 0 auto;
		padding: 1.5rem;
	}
	header {
		display: flex;
		align-items: center;
		gap: 0.9rem;
		margin-bottom: 1.2rem;
	}
	h1 {
		font-size: 1.4rem;
		font-weight: 700;
		color: var(--text-strong);
	}
	.count {
		font-size: 0.8rem;
		color: var(--text-muted);
	}
	.filter {
		margin-left: auto;
		width: 14rem;
		padding: 0.4rem 0.7rem;
		font-size: 0.85rem;
		border: 1px solid var(--border);
		border-radius: 6px;
		background: var(--bg-subtle);
		color: var(--text-strong);
		outline: none;
	}
	.filter:focus {
		border-color: var(--accent);
	}
	.empty {
		padding: 2rem;
		text-align: center;
		color: var(--text-faint);
	}
	.tags {
		list-style: none;
		display: flex;
		flex-direction: column;
		gap: 0.35rem;
	}
	.tag-item {
		border: 1px solid var(--border);
		border-radius: 8px;
		overflow: hidden;
		background: var(--bg-elevated);
	}
	.tag-head {
		display: flex;
		align-items: center;
		gap: 0.75rem;
		width: 100%;
		padding: 0.6rem 0.9rem;
		background: transparent;
		border: none;
		cursor: pointer;
		text-align: left;
	}
	.tag-head:hover {
		background: var(--nav-hover-bg);
	}
	.chip {
		font-size: 0.85rem;
		font-weight: 600;
		color: var(--text-muted);
		padding: 0.15rem 0.5rem;
		border-radius: 5px;
		background: var(--bg-subtle);
	}
	.chip.defined {
		color: var(--chip);
		background: color-mix(in srgb, var(--chip) 14%, transparent);
		border: 1px solid color-mix(in srgb, var(--chip) 35%, transparent);
	}
	.usage {
		font-size: 0.78rem;
		color: var(--text-muted);
		min-width: 1.5rem;
		text-align: center;
	}
	.chev {
		margin-left: auto;
		color: var(--text-faint);
		transition: transform 0.15s;
	}
	.chev.open {
		transform: rotate(90deg);
	}
	.docs {
		list-style: none;
		border-top: 1px solid var(--border);
		padding: 0.3rem;
	}
	.doc-empty {
		padding: 0.5rem 0.7rem;
		font-size: 0.78rem;
		color: var(--text-faint);
	}
	.doc {
		display: flex;
		align-items: center;
		gap: 0.6rem;
		padding: 0.4rem 0.6rem;
		border-radius: 5px;
		font-size: 0.83rem;
		text-decoration: none;
		color: var(--text);
	}
	.doc:hover {
		background: var(--nav-hover-bg);
	}
	.dtype {
		flex: none;
		min-width: 3.4rem;
		text-align: center;
		font-size: 0.68rem;
		font-weight: 600;
		border-radius: 4px;
		padding: 0.1rem 0.35rem;
	}
	.dtype.quest {
		color: var(--accent);
		background: color-mix(in srgb, var(--accent) 14%, transparent);
	}
	.dtype.rule {
		color: var(--success);
		background: color-mix(in srgb, var(--success) 14%, transparent);
	}
	.dtype.book {
		color: var(--warning);
		background: color-mix(in srgb, var(--warning) 14%, transparent);
	}
	.dtitle {
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
</style>
