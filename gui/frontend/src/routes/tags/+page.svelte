<!--
  DEV-253: 태그 목록 페이지 — /tags.

  타이틀바 ☰ 메뉴의 진입점. 길드의 모든 문서(퀘스트 / 규칙 / 도서관)에서
  사용 중인 태그를 집계해 사용 횟수와 함께 나열한다. 태그를 펼치면 그 태그가
  달린 문서 목록이 나오고, 각 항목은 해당 페이지로 이동한다.

  태그 정의(/api/tag-defs)가 있으면 색을 따르고, 정의만 있고 사용처가 없는
  태그도 함께 보여준다(0 회).

  집계는 클라이언트 사이드 — 검색 팔레트와 동일하게 각 타입의 list API 병합.

  DEV-392: 태그 **정의 편집**(만들기 · 색 · 설명 · 삭제)도 여기서 한다. 예전엔 관리 페이지에
  따로 있어서, 여기서 본 색을 바꾸려면 관리 페이지로 가야 했다. 같은 대상을 두 화면이
  나눠 갖지 않게 합쳤다.
-->
<script lang="ts">
	import { onMount } from 'svelte';
	import { questsApi } from '$lib/api/quests';
	import { rulesApi } from '$lib/api/rules';
	import { libraryApi } from '$lib/api/library';
	import { adminApi } from '$lib/api/admin';
	import { locale, t } from '$lib/stores/locale';
	import { showToast } from '$lib/stores/toast';
	import ConfirmDialog from '$lib/components/ConfirmDialog.svelte';

	type Kind = 'quest' | 'rule' | 'book';
	interface Doc {
		kind: Kind;
		label: string;
		title: string;
		href: string;
	}
	interface TagRow {
		tag: string;
		/** 정의 파일이 있나 — 색이 비어 있어도 정의일 수 있다. */
		defined: boolean;
		color: string | null;
		description: string;
		docs: Doc[];
	}

	const KIND_KEY: Record<Kind, string> = {
		quest: 'tags.kindQuest',
		rule: 'tags.kindRule',
		book: 'tags.kindBook'
	};
	const DEFAULT_COLOR = '#7bb87f';
	/** 코어 `validate_tag_slug` 와 같은 규칙. 예전 관리 화면은 `-` 를 빼먹어 코어가 받는 이름을 거절했다. */
	const SLUG_RE = /^[a-z0-9_-]{1,32}$/;

	let rows = $state<TagRow[]>([]);
	let loading = $state(true);
	let filter = $state('');
	let expanded = $state<string | null>(null);

	// 편집 — 한 번에 한 줄. `null` 이면 닫힘, `''` 이면 새 정의.
	let editing = $state<string | null>(null);
	let editSlug = $state('');
	let editColor = $state(DEFAULT_COLOR);
	let editDescription = $state('');
	let busy = $state(false);
	let confirmDelete = $state<TagRow | null>(null);

	onMount(() => void load());

	async function load() {
		loading = true;
		try {
			const [quests, rules, books, defs] = await Promise.all([
				questsApi.list(true).catch(() => []),
				rulesApi.list().catch(() => ({ entries: [] })),
				libraryApi.list().catch(() => []),
				adminApi.listTagDefs().catch(() => [])
			]);

			const map = new Map<string, TagRow>();
			const ensure = (tag: string): TagRow => {
				let r = map.get(tag);
				if (!r) {
					r = { tag, defined: false, color: null, description: '', docs: [] };
					map.set(tag, r);
				}
				return r;
			};

			for (const d of defs) {
				const r = ensure(d.slug);
				r.defined = true;
				r.color = d.color || null;
				r.description = d.description ?? '';
			}

			for (const q of quests) {
				for (const tag of q.tags ?? []) {
					ensure(tag).docs.push({
						kind: 'quest',
						label: q.quest_id,
						title: q.title,
						// REQ-004: `/quests/[id]` 라우트는 param 을 **slug 로** 받아
						// `getBySlug` 를 호출한다(+page.svelte 의 `$page.params.id`).
						// 숫자 PK 를 넘기면 조회가 실패해 에러 화면이 뜬다 —
						// 앱에서 유일하게 어긋나 있던 링크다.
						href: `/quests/${encodeURIComponent(q.quest_id)}`
					});
				}
			}
			for (const r of rules.entries) {
				for (const tag of r.tags ?? []) {
					ensure(tag).docs.push({
						kind: 'rule',
						label: r.slug,
						title: r.slug,
						href: `/rules?slug=${encodeURIComponent(r.slug)}`
					});
				}
			}
			for (const b of books) {
				for (const tag of b.tags ?? []) {
					ensure(tag).docs.push({
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
		return rows.filter(
			(r) => r.tag.toLowerCase().includes(q) || r.description.toLowerCase().includes(q)
		);
	});

	function toggle(tag: string) {
		expanded = expanded === tag ? null : tag;
	}

	function startCreate() {
		editing = '';
		editSlug = '';
		editColor = DEFAULT_COLOR;
		editDescription = '';
	}

	/** 정의가 있으면 고치고, 쓰이기만 하는 태그면 그 이름으로 정의를 만든다. */
	function startEdit(r: TagRow) {
		editing = r.tag;
		editSlug = r.tag;
		editColor = r.color || DEFAULT_COLOR;
		editDescription = r.description;
	}

	async function save() {
		const slug = (editing === '' ? editSlug.trim().toLowerCase() : editing) ?? '';
		if (!SLUG_RE.test(slug)) {
			showToast(t('tags.slugPattern', $locale), 'error');
			return;
		}
		busy = true;
		try {
			await adminApi.upsertTagDef({ slug, color: editColor, description: editDescription.trim() });
			editing = null;
			await load();
			showToast(t('tags.saved', $locale), 'success');
		} catch (e) {
			showToast(`${t('tags.saveFailed', $locale)}: ${e instanceof Error ? e.message : e}`, 'error');
		} finally {
			busy = false;
		}
	}

	async function doDelete() {
		const target = confirmDelete;
		confirmDelete = null;
		if (!target) return;
		busy = true;
		try {
			await adminApi.deleteTagDef(target.tag);
			await load();
			showToast(t('tags.deleted', $locale), 'success');
		} catch (e) {
			showToast(`${t('tags.deleteFailed', $locale)}: ${e instanceof Error ? e.message : e}`, 'error');
		} finally {
			busy = false;
		}
	}
</script>

{#snippet editor()}
	<div class="editor">
		{#if editing === ''}
			<input
				class="slug"
				type="text"
				bind:value={editSlug}
				placeholder={t('tags.slugPlaceholder', $locale)}
				aria-label={t('tags.slug', $locale)}
				maxlength="32"
				spellcheck="false"
				disabled={busy}
			/>
		{/if}
		<input
			type="color"
			bind:value={editColor}
			aria-label={t('tags.color', $locale)}
			disabled={busy}
		/>
		<input
			class="desc-input"
			type="text"
			bind:value={editDescription}
			placeholder={t('tags.descPlaceholder', $locale)}
			aria-label={t('tags.description', $locale)}
			maxlength="200"
			disabled={busy}
		/>
		<button class="btn save" onclick={save} disabled={busy}>{t('common.save', $locale)}</button>
		<button class="btn" onclick={() => (editing = null)} disabled={busy}
			>{t('common.cancel', $locale)}</button
		>
	</div>
{/snippet}

<div class="wrap">
	<header>
		<h1>{t('tags.title', $locale)}</h1>
		<span class="count">{t('tags.count', $locale).replace('{n}', String(rows.length))}</span>
		<input
			class="filter"
			bind:value={filter}
			placeholder={t('tags.filterPlaceholder', $locale)}
			spellcheck="false"
		/>
		<button class="btn" onclick={startCreate} disabled={busy || editing === ''}
			>{t('tags.newDef', $locale)}</button
		>
	</header>
	<p class="intro">{t('tags.intro', $locale)}</p>
	{#if editing === ''}
		<div class="tag-item new">{@render editor()}</div>
	{/if}
	{#if loading}
		<p class="empty">{t('tags.loading', $locale)}</p>
	{:else if shown.length === 0}
		<p class="empty">{t('tags.empty', $locale)}</p>
	{:else}
		<ul class="tags">
			{#each shown as r (r.tag)}
				<li class="tag-item">
					<div class="tag-row">
						<button class="tag-head" onclick={() => toggle(r.tag)} aria-expanded={expanded === r.tag}>
							<span
								class="chip"
								style={r.color ? `--chip:${r.color}` : ''}
								class:defined={!!r.color}>#{r.tag}</span
							>
							<span class="usage">{r.docs.length}</span>
							{#if r.description}
								<span class="tag-desc">{r.description}</span>
							{/if}
							<span class="chev" class:open={expanded === r.tag}>›</span>
						</button>
						<div class="row-actions">
							{#if r.defined}
								<button class="btn" onclick={() => startEdit(r)} disabled={busy}
									>{t('tags.edit', $locale)}</button
								>
								<button class="btn danger" onclick={() => (confirmDelete = r)} disabled={busy}
									>{t('tags.delete', $locale)}</button
								>
							{:else if SLUG_RE.test(r.tag)}
								<!-- 색을 주려면 정의가 있어야 한다 — 쓰이기만 하는 태그에서 바로 만든다. -->
								<button class="btn" onclick={() => startEdit(r)} disabled={busy}
									>{t('tags.define', $locale)}</button
								>
							{/if}
						</div>
					</div>
					{#if editing === r.tag}
						{@render editor()}
					{/if}
					{#if expanded === r.tag}
						<ul class="docs">
							{#if r.docs.length === 0}
								<li class="doc-empty">{t('tags.noUsage', $locale)}</li>
							{:else}
								{#each r.docs as d (d.kind + d.label)}
									<li>
										<a class="doc" href={d.href}>
											<span class="dtype {d.kind}">{t(KIND_KEY[d.kind], $locale)}</span>
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

<ConfirmDialog
	open={confirmDelete !== null}
	title={t('tags.deleteTitle', $locale)}
	message={t('tags.deleteMsg', $locale).replace('{tag}', confirmDelete?.tag ?? '')}
	confirmLabel={t('tags.delete', $locale)}
	danger
	onconfirm={doDelete}
	oncancel={() => (confirmDelete = null)}
/>

<style>
	.wrap {
		max-width: var(--content-max-width, 1100px);
		margin: 0 auto;
		padding: 1.5rem;
	}
	header {
		display: flex;
		flex-wrap: wrap;
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
		border: var(--bw) solid var(--border);
		border-radius: var(--r-md);
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
		border: var(--bw) solid var(--border);
		border-radius: var(--r-lg);
		overflow: hidden;
		background: var(--bg-elevated);
	}
	.tag-row {
		display: flex;
		align-items: center;
	}
	.tag-head {
		display: flex;
		align-items: center;
		gap: 0.75rem;
		flex: 1;
		min-width: 0;
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
		border-radius: var(--r-sm);
		background: var(--bg-subtle);
	}
	.chip.defined {
		color: var(--chip);
		background: color-mix(in srgb, var(--chip) 14%, transparent);
		border: var(--bw) solid color-mix(in srgb, var(--chip) 35%, transparent);
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
	.tag-desc {
		font-size: 0.8rem;
		color: var(--text-muted);
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
		min-width: 0;
	}
	.row-actions {
		display: flex;
		gap: 0.3rem;
		padding-right: 0.6rem;
		flex: none;
	}
	.btn {
		padding: 0.25rem 0.7rem;
		background: var(--bg-subtle);
		border: var(--bw) solid var(--border);
		border-radius: var(--r-sm);
		color: var(--text);
		font-size: 0.8rem;
		cursor: pointer;
		white-space: nowrap;
	}
	.btn:hover:not(:disabled) {
		background: var(--border);
	}
	.btn:disabled {
		opacity: 0.5;
		cursor: not-allowed;
	}
	.btn.danger {
		color: var(--danger);
		border-color: var(--danger);
	}
	.btn.save {
		background: var(--btn-primary-bg);
		border-color: var(--btn-primary-border);
		color: var(--btn-primary-text);
	}
	.intro {
		margin: -0.6rem 0 1rem;
		font-size: 0.8rem;
		color: var(--text-muted);
	}
	.editor {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 0.5rem;
		padding: 0.5rem 0.9rem 0.7rem;
		border-top: var(--bw) solid var(--border);
	}
	.tag-item.new {
		margin-bottom: 0.35rem;
	}
	.tag-item.new .editor {
		border-top: none;
		padding-top: 0.7rem;
	}
	.editor input[type='text'] {
		padding: 0.3rem 0.5rem;
		background: var(--bg);
		border: var(--bw) solid var(--border);
		border-radius: var(--r-sm);
		color: var(--text);
		font: inherit;
		font-size: 0.85rem;
	}
	.editor .slug {
		width: 12rem;
	}
	.editor .desc-input {
		flex: 1;
		min-width: 12rem;
	}
	.editor input[type='color'] {
		width: 2.2rem;
		height: 1.7rem;
		padding: 0;
		border: var(--bw) solid var(--border);
		border-radius: var(--r-sm);
		background: var(--bg);
		cursor: pointer;
	}
	.docs {
		list-style: none;
		border-top: var(--bw) solid var(--border);
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
		border-radius: var(--r-sm);
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
		border-radius: var(--r-sm);
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
