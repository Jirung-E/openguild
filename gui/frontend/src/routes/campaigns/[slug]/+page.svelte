<!--
  DEV-011: Campaign 상세 페이지 (/campaigns/[slug]).
   - 제목 / 기간 / status (active|done 토글) / display_order
   - 본문 markdown 편집 (DEV-066 패턴 — DB sync)
   - 체크리스트 (본문의 GFM task list 와 자동 sync) — 토글 / 추가 / 삭제
   - 연결된 quest 표시 + 추가 / 제거
-->
<script lang="ts">
	import { onMount } from 'svelte';
	import { goto } from '$app/navigation';
	import { page } from '$app/stores';
	import { campaignsApi } from '$lib/api/campaigns';
	import { questsApi } from '$lib/api/quests';
	import type { CampaignDetail, Quest } from '$lib/types';
	import { marked } from 'marked';

	let slug = $derived($page.params.slug ?? '');
	let detail = $state<CampaignDetail | null>(null);
	let loading = $state(true);
	let error = $state<string | null>(null);

	// edit mode (메타 + 본문)
	let editMeta = $state(false);
	let editBody = $state(false);
	let titleEdit = $state('');
	let startedEdit = $state('');
	let endedEdit = $state('');
	let bodyEdit = $state('');
	let saving = $state(false);

	// 체크리스트 추가 입력
	let newChecklistText = $state('');

	// quest 연결
	let allQuests = $state<Quest[]>([]);
	let linkInput = $state('');

	async function load() {
		loading = true;
		try {
			const [d, qs] = await Promise.all([
				campaignsApi.get(slug),
				questsApi.list()
			]);
			detail = d;
			allQuests = qs;
		} catch (e) {
			error = e instanceof Error ? e.message : 'failed to load';
		} finally {
			loading = false;
		}
	}

	onMount(load);
	$effect(() => {
		// slug 가 바뀌면 (다른 캠페인으로 navigate) 재로드
		void slug;
		if (slug) load();
	});

	function fmtPeriod(): string {
		if (!detail) return '';
		const a = detail.started_at?.trim() || '';
		const b = detail.ended_at?.trim() || '';
		if (!a && !b) return '기간 미정';
		if (a && !b) return `${a} ~`;
		if (!a && b) return `~ ${b}`;
		return `${a} ~ ${b}`;
	}

	function startEditMeta() {
		if (!detail) return;
		titleEdit = detail.title;
		startedEdit = detail.started_at ?? '';
		endedEdit = detail.ended_at ?? '';
		editMeta = true;
	}
	async function saveMeta() {
		if (!detail) return;
		saving = true;
		try {
			await campaignsApi.update(detail.campaign_slug, {
				title: titleEdit.trim(),
				started_at: startedEdit,
				ended_at: endedEdit
			});
			editMeta = false;
			await load();
		} catch (e) {
			alert(e instanceof Error ? e.message : 'failed');
		} finally {
			saving = false;
		}
	}

	function startEditBody() {
		if (!detail) return;
		bodyEdit = detail.description ?? '';
		editBody = true;
	}
	async function saveBody() {
		if (!detail) return;
		saving = true;
		try {
			await campaignsApi.update(detail.campaign_slug, {
				description: bodyEdit
			});
			editBody = false;
			await load();
		} catch (e) {
			alert(e instanceof Error ? e.message : 'failed');
		} finally {
			saving = false;
		}
	}

	async function toggleStatus() {
		if (!detail) return;
		const next = detail.status === 'active' ? 'done' : 'active';
		try {
			await campaignsApi.update(detail.campaign_slug, { status: next });
			await load();
		} catch (e) {
			alert(e instanceof Error ? e.message : 'failed');
		}
	}

	// ── 체크리스트 ──
	async function addChecklist() {
		if (!detail) return;
		const t = newChecklistText.trim();
		if (!t) return;
		try {
			await campaignsApi.addChecklist(detail.campaign_slug, t);
			newChecklistText = '';
			await load();
		} catch (e) {
			alert(e instanceof Error ? e.message : 'failed');
		}
	}
	async function toggleChecklist(idx: number, currentlyChecked: boolean) {
		if (!detail) return;
		try {
			await campaignsApi.setChecklist(detail.campaign_slug, idx + 1, !currentlyChecked);
			await load();
		} catch (e) {
			alert(e instanceof Error ? e.message : 'failed');
		}
	}
	async function removeChecklist(idx: number) {
		if (!detail) return;
		if (!confirm('이 체크리스트 항목을 삭제할까요?')) return;
		try {
			await campaignsApi.removeChecklist(detail.campaign_slug, idx + 1);
			await load();
		} catch (e) {
			alert(e instanceof Error ? e.message : 'failed');
		}
	}

	// ── Quest 연결 ──
	async function linkQuest() {
		if (!detail) return;
		const qs = linkInput.trim().toUpperCase();
		if (!qs) return;
		try {
			await campaignsApi.linkQuest(detail.campaign_slug, qs);
			linkInput = '';
			await load();
		} catch (e) {
			alert(e instanceof Error ? e.message : 'failed');
		}
	}
	async function unlinkQuest(qSlug: string) {
		if (!detail) return;
		try {
			await campaignsApi.unlinkQuest(detail.campaign_slug, qSlug);
			await load();
		} catch (e) {
			alert(e instanceof Error ? e.message : 'failed');
		}
	}

	async function deleteCampaign() {
		if (!detail) return;
		if (!confirm(`캠페인 "${detail.title}" 삭제할까요? (soft delete)`)) return;
		try {
			await campaignsApi.delete(detail.campaign_slug);
			goto('/campaigns');
		} catch (e) {
			alert(e instanceof Error ? e.message : 'failed');
		}
	}

	function renderMd(s: string | null | undefined): string {
		if (!s) return '';
		return marked(s, { async: false }) as string;
	}
</script>

<div class="page">
	<div class="top">
		<button class="back" onclick={() => history.back()}>← 뒤로</button>
		{#if detail}
			<button class="status-badge status-{detail.status}" onclick={toggleStatus} title="클릭하여 상태 토글">
				{detail.status}
			</button>
			<button class="btn-delete" onclick={deleteCampaign}>🗑 삭제</button>
		{/if}
	</div>

	{#if loading}
		<div class="state">Loading…</div>
	{:else if error || !detail}
		<div class="state error">{error ?? '캠페인 없음'}</div>
	{:else}
		<!-- 메타 -->
		<section class="meta">
			{#if editMeta}
				<input class="title-input" bind:value={titleEdit} disabled={saving} />
				<div class="period-row">
					<input type="date" bind:value={startedEdit} disabled={saving} />
					<span>~</span>
					<input type="date" bind:value={endedEdit} disabled={saving} />
				</div>
				<div class="actions">
					<button class="btn-save" onclick={saveMeta} disabled={saving || !titleEdit.trim()}>
						{saving ? '저장…' : '저장'}
					</button>
					<button class="btn-cancel" onclick={() => (editMeta = false)} disabled={saving}>취소</button>
				</div>
			{:else}
				<div class="title-row">
					<span class="slug">{detail.campaign_slug}</span>
					<h1>{detail.title}</h1>
					<button class="btn-edit" onclick={startEditMeta}>✎ 편집</button>
				</div>
				<div class="period">{fmtPeriod()}</div>
			{/if}
		</section>

		<!-- 본문 markdown -->
		<section class="body">
			<div class="section-head">
				<h2>본문</h2>
				{#if !editBody}
					<button class="btn-edit" onclick={startEditBody}>✎ 편집</button>
				{/if}
			</div>
			{#if editBody}
				<textarea bind:value={bodyEdit} rows="12" disabled={saving}></textarea>
				<div class="actions">
					<button class="btn-save" onclick={saveBody} disabled={saving}>
						{saving ? '저장…' : '저장'}
					</button>
					<button class="btn-cancel" onclick={() => (editBody = false)} disabled={saving}>취소</button>
				</div>
			{:else if detail.description && detail.description.trim()}
				<div class="md">{@html renderMd(detail.description)}</div>
			{:else}
				<div class="empty">본문 없음. <button class="link" onclick={startEditBody}>본문 추가</button></div>
			{/if}
		</section>

		<!-- 체크리스트 -->
		<section>
			<h2>체크리스트 ({detail.checklists.filter((c) => c.checked).length}/{detail.checklists.length})</h2>
			{#if detail.checklists.length === 0}
				<p class="empty">항목 없음.</p>
			{:else}
				<ul class="checklist">
					{#each detail.checklists as item, idx (item.id)}
						<li>
							<label>
								<input
									type="checkbox"
									checked={item.checked}
									onchange={() => toggleChecklist(idx, item.checked)}
								/>
								<span class:checked={item.checked}>{item.text}</span>
							</label>
							<button class="rm" title="삭제" onclick={() => removeChecklist(idx)}>×</button>
						</li>
					{/each}
				</ul>
			{/if}
			<div class="add-row">
				<input
					type="text"
					bind:value={newChecklistText}
					placeholder="새 체크리스트 항목..."
					onkeydown={(e) => e.key === 'Enter' && addChecklist()}
				/>
				<button onclick={addChecklist} disabled={!newChecklistText.trim()}>추가</button>
			</div>
		</section>

		<!-- 연결된 Quest -->
		<section>
			<h2>연결된 퀘스트 ({detail.linked_quests.length})</h2>
			{#if detail.linked_quests.length === 0}
				<p class="empty">연결된 퀘스트 없음.</p>
			{:else}
				<ul class="linked">
					{#each detail.linked_quests as q (q.id)}
						<li>
							<a href={`/quests/${encodeURIComponent(q.quest_id)}?from=campaign:${detail.campaign_slug}`}>
								<span class="badge type" style:--c={q.type_color}>{q.quest_id}</span>
								<span class="qtitle">{q.title}</span>
								<span class="badge status" style:--c={q.status_color}>{q.status_name_en}</span>
							</a>
							<button class="rm" title="연결 해제" onclick={() => unlinkQuest(q.quest_id)}>×</button>
						</li>
					{/each}
				</ul>
			{/if}
			<div class="add-row">
				<input
					type="text"
					bind:value={linkInput}
					placeholder="quest slug (예: DEV-001)"
					list="quest-options"
					onkeydown={(e) => e.key === 'Enter' && linkQuest()}
				/>
				<datalist id="quest-options">
					{#each allQuests as q (q.id)}
						<option value={q.quest_id}>{q.title}</option>
					{/each}
				</datalist>
				<button onclick={linkQuest} disabled={!linkInput.trim()}>연결</button>
			</div>
		</section>
	{/if}
</div>

<style>
	.page { padding: 1.25rem 1.5rem; max-width: 880px; margin: 0 auto; }
	.top {
		display: flex;
		align-items: center;
		gap: 0.5rem;
		margin-bottom: 1rem;
	}
	.back, .btn-delete, .status-badge, .btn-edit {
		font-size: 0.825rem;
		padding: 0.3rem 0.7rem;
		border-radius: 6px;
		cursor: pointer;
		background: transparent;
		border: 1px solid #30363d;
		color: #c9d1d9;
		font-family: inherit;
	}
	.back:hover, .btn-edit:hover { background: #21262d; }
	.btn-delete { margin-left: auto; color: #f85149; border-color: #5a2424; }
	.btn-delete:hover { background: #2d0f0f; }

	.status-badge { text-transform: uppercase; font-weight: 600; }
	.status-badge.status-active { background: #102a18; color: #56d364; border-color: #2ea043; }
	.status-badge.status-done { background: #2a2a2a; color: #8b949e; }

	.state { color: #6e7681; padding: 1.5rem 0; font-size: 0.875rem; }
	.state.error { color: #f85149; }

	section { margin-bottom: 1.75rem; }
	.section-head { display: flex; align-items: baseline; gap: 0.75rem; margin-bottom: 0.4rem; }
	h1 { font-size: 1.4rem; color: #c9d1d9; margin: 0; }
	h2 { font-size: 1rem; color: #c9d1d9; margin: 0 0 0.4rem 0; }

	.title-row { display: flex; align-items: baseline; gap: 0.75rem; }
	.title-row .btn-edit { margin-left: auto; }
	.slug {
		font-size: 0.8rem;
		font-family: 'JetBrains Mono', ui-monospace, monospace;
		color: #8b949e;
	}
	.period { color: #8b949e; font-size: 0.875rem; }

	.title-input {
		background: #0d1117;
		border: 1px solid #30363d;
		color: #c9d1d9;
		border-radius: 6px;
		padding: 0.4rem 0.6rem;
		font-size: 1.2rem;
		width: 100%;
		margin-bottom: 0.5rem;
	}
	.period-row { display: flex; align-items: center; gap: 0.5rem; margin-bottom: 0.5rem; }
	.period-row input {
		background: #0d1117;
		border: 1px solid #30363d;
		color: #c9d1d9;
		border-radius: 6px;
		padding: 0.3rem 0.5rem;
	}

	.actions { display: flex; gap: 0.4rem; margin-top: 0.5rem; }
	.btn-save {
		padding: 0.35rem 0.85rem;
		background: #238636;
		border: 1px solid #2ea043;
		color: #fff;
		border-radius: 6px;
		cursor: pointer;
		font-size: 0.825rem;
	}
	.btn-save:disabled { opacity: 0.5; cursor: not-allowed; }
	.btn-cancel {
		padding: 0.35rem 0.85rem;
		background: transparent;
		border: 1px solid #30363d;
		color: #c9d1d9;
		border-radius: 6px;
		cursor: pointer;
		font-size: 0.825rem;
	}

	textarea {
		background: #0d1117;
		border: 1px solid #30363d;
		color: #c9d1d9;
		border-radius: 6px;
		padding: 0.5rem 0.7rem;
		font-family: 'JetBrains Mono', ui-monospace, monospace;
		font-size: 0.875rem;
		width: 100%;
		resize: vertical;
	}

	.md {
		background: #0d1117;
		border: 1px solid #21262d;
		border-radius: 6px;
		padding: 0.85rem 1rem;
		color: #c9d1d9;
		font-size: 0.9rem;
		line-height: 1.55;
	}
	.md :global(h1), .md :global(h2), .md :global(h3) { color: #c9d1d9; }
	.md :global(code) { background: #161b22; padding: 0.1rem 0.3rem; border-radius: 3px; }
	.md :global(input[type="checkbox"]) { margin-right: 0.4rem; }

	.empty { color: #6e7681; font-size: 0.875rem; }
	.link { background: none; border: none; color: #58a6ff; cursor: pointer; padding: 0; }

	.checklist, .linked { list-style: none; padding: 0; margin: 0; display: flex; flex-direction: column; gap: 2px; }
	.checklist li, .linked li {
		display: flex;
		align-items: center;
		gap: 0.5rem;
		padding: 0.45rem 0.7rem;
		background: #161b22;
		border: 1px solid #21262d;
		border-radius: 6px;
	}
	.checklist li label { display: flex; align-items: center; gap: 0.5rem; flex: 1; cursor: pointer; }
	.checklist li span.checked { text-decoration: line-through; color: #8b949e; }

	.linked li a { display: flex; align-items: center; gap: 0.5rem; flex: 1; text-decoration: none; color: inherit; }
	.qtitle { color: #c9d1d9; flex: 1; }

	.rm {
		background: transparent;
		border: 1px solid transparent;
		color: #8b949e;
		cursor: pointer;
		border-radius: 4px;
		width: 1.5rem;
		height: 1.5rem;
	}
	.rm:hover { color: #f85149; border-color: #5a2424; }

	.add-row {
		display: flex;
		gap: 0.4rem;
		margin-top: 0.5rem;
	}
	.add-row input {
		flex: 1;
		background: #0d1117;
		border: 1px solid #30363d;
		color: #c9d1d9;
		border-radius: 6px;
		padding: 0.35rem 0.6rem;
		font-size: 0.875rem;
	}
	.add-row button {
		padding: 0.35rem 0.85rem;
		background: #21262d;
		border: 1px solid #30363d;
		color: #c9d1d9;
		border-radius: 6px;
		cursor: pointer;
		font-size: 0.825rem;
	}
	.add-row button:disabled { opacity: 0.5; cursor: not-allowed; }
	.add-row button:hover:not(:disabled) { background: #2a2a4a; }

	.badge {
		font-size: 0.7rem;
		padding: 0.1rem 0.45rem;
		border-radius: 4px;
		color: var(--c);
		border: 1px solid var(--c);
		font-family: 'JetBrains Mono', ui-monospace, monospace;
	}
	.badge.status { font-family: inherit; }
</style>
