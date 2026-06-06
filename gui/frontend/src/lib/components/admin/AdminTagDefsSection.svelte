<script lang="ts">
	// DEV-068: `.guild/tags/{slug}.toml` — 사용자가 정의하는 태그.
	// quest_tag_defs 캐시 + 파일 진리원. quest 의 frontmatter 의 tag 사용 자체는
	// def 없어도 정상 (UI 가 fallback 색으로 표시).
	import { onMount } from 'svelte';
	import { adminApi } from '$lib/api/admin';
	import type { QuestTagDef } from '$lib/types';

	type Msg = { kind: 'info' | 'success' | 'error'; text: string } | null;
	let { onmessage }: { onmessage: (m: Msg) => void } = $props();

	let defs = $state<QuestTagDef[]>([]);
	let busy = $state(false);
	let editing: string | null = $state(null); // 원래 slug.
	let editColor = $state('');
	let editDescription = $state('');

	let creating = $state(false);
	let newSlug = $state('');
	let newColor = $state('#7bb87f');
	let newDescription = $state('');

	let confirmDelete: QuestTagDef | null = $state(null);

	onMount(refresh);

	export async function refresh() {
		try {
			defs = await adminApi.listTagDefs();
		} catch (e) {
			onmessage({ kind: 'error', text: `tag 정의 조회 실패: ${e}` });
		}
	}

	function startEdit(d: QuestTagDef) {
		editing = d.slug;
		editColor = d.color || '#7bb87f';
		editDescription = d.description;
	}

	function cancelEdit() {
		editing = null;
	}

	async function saveEdit() {
		if (!editing) return;
		busy = true;
		try {
			await adminApi.upsertTagDef({
				slug: editing,
				color: editColor,
				description: editDescription
			});
			onmessage({ kind: 'success', text: `'${editing}' 갱신됨` });
			editing = null;
			await refresh();
		} catch (e) {
			onmessage({ kind: 'error', text: `갱신 실패: ${e}` });
		} finally {
			busy = false;
		}
	}

	function openCreate() {
		newSlug = '';
		newColor = '#7bb87f';
		newDescription = '';
		creating = true;
	}

	async function doCreate() {
		const slug = newSlug.trim().toLowerCase();
		if (!slug) {
			onmessage({ kind: 'error', text: 'slug 는 필수.' });
			return;
		}
		if (!/^[a-z0-9_]+$/.test(slug)) {
			onmessage({ kind: 'error', text: 'slug 는 소문자/숫자/_ 만 (최대 32자).' });
			return;
		}
		busy = true;
		try {
			await adminApi.upsertTagDef({
				slug,
				color: newColor,
				description: newDescription
			});
			onmessage({ kind: 'success', text: `'${slug}' 추가됨` });
			creating = false;
			await refresh();
		} catch (e) {
			onmessage({ kind: 'error', text: `추가 실패: ${e}` });
		} finally {
			busy = false;
		}
	}

	function askDelete(d: QuestTagDef) {
		confirmDelete = d;
	}

	async function doDelete() {
		if (!confirmDelete) return;
		const target = confirmDelete;
		confirmDelete = null;
		busy = true;
		try {
			await adminApi.deleteTagDef(target.slug);
			onmessage({
				kind: 'success',
				text: `'${target.slug}' 정의 삭제됨 (quest 사용은 보존)`
			});
			await refresh();
		} catch (e) {
			onmessage({ kind: 'error', text: `삭제 실패: ${e}` });
		} finally {
			busy = false;
		}
	}
</script>

<section>
	<div class="section-header">
		<h2>Quest Tag 정의</h2>
		<div class="actions">
			<button onclick={openCreate} disabled={busy}>+ 새 tag 정의</button>
			<button onclick={refresh} disabled={busy}>새로고침</button>
		</div>
	</div>
	<p class="intro">
		<code>.guild/tags/&lt;slug&gt;.toml</code> 의 색 / 설명. 정의가 없는 tag 도 quest 가 사용 가능
		(UI 기본 색으로 표시).
	</p>

	{#if defs.length === 0}
		<p class="empty">정의된 tag 없음.</p>
	{:else}
		<table>
			<thead>
				<tr>
					<th style="width: 16ch">slug</th>
					<th style="width: 5ch">색</th>
					<th>설명</th>
					<th style="width: 14ch"></th>
				</tr>
			</thead>
			<tbody>
				{#each defs as d (d.slug)}
					<tr>
						{#if editing === d.slug}
							<td><code>{d.slug}</code></td>
							<td><input type="color" bind:value={editColor} disabled={busy} /></td>
							<td>
								<input type="text" bind:value={editDescription} maxlength="200" disabled={busy} />
							</td>
							<td class="row-actions">
								<button class="save" onclick={saveEdit} disabled={busy}>저장</button>
								<button onclick={cancelEdit} disabled={busy}>취소</button>
							</td>
						{:else}
							<td><code>{d.slug}</code></td>
							<td>
								{#if d.color}
									<span class="swatch" style="background: {d.color}"></span>
									<code class="hex">{d.color}</code>
								{:else}
									<span class="dim">—</span>
								{/if}
							</td>
							<td>{d.description || '—'}</td>
							<td class="row-actions">
								<button onclick={() => startEdit(d)} disabled={busy}>수정</button>
								<button class="danger" onclick={() => askDelete(d)} disabled={busy}>삭제</button>
							</td>
						{/if}
					</tr>
				{/each}
			</tbody>
		</table>
	{/if}
</section>

{#if creating}
	<div class="ov" role="presentation">
		<div class="modal" role="dialog" aria-modal="true" tabindex="-1">
			<h3 class="modal-title">새 tag 정의</h3>
			<div class="form">
				<label>
					<span>slug</span>
					<input
						type="text"
						bind:value={newSlug}
						placeholder="frontend / urgent 등"
						maxlength="32"
						pattern="[a-z0-9_]+"
						title="소문자 / 숫자 / '_' 만, 최대 32자"
						disabled={busy}
					/>
				</label>
				<label>
					<span>색</span>
					<input type="color" bind:value={newColor} disabled={busy} />
					<code class="hex">{newColor}</code>
				</label>
				<label>
					<span>설명</span>
					<input
						type="text"
						bind:value={newDescription}
						placeholder="(선택) 이 tag 의 용도"
						maxlength="200"
						disabled={busy}
					/>
				</label>
			</div>
			<p class="form-note">파일 <code>.guild/tags/&lt;slug&gt;.toml</code> 로 저장됩니다.</p>
			<div class="modal-actions">
				<button class="btn-yes" onclick={doCreate} disabled={busy}>추가</button>
				<button class="btn-no" onclick={() => (creating = false)} disabled={busy}>취소</button>
			</div>
		</div>
	</div>
{/if}

{#if confirmDelete}
	<div class="ov" role="presentation">
		<div class="modal" role="dialog" aria-modal="true" tabindex="-1">
			<h3 class="modal-title">Tag 정의 삭제</h3>
			<p class="modal-msg">
				<code>{confirmDelete.slug}</code> 정의를 삭제할까요? <br />
				기존 quest 의 tag 사용은 그대로 (fallback 색).
			</p>
			<div class="modal-actions">
				<button class="btn-yes danger" onclick={doDelete} disabled={busy}>삭제</button>
				<button class="btn-no" onclick={() => (confirmDelete = null)} disabled={busy}>취소</button>
			</div>
		</div>
	</div>
{/if}

<style>
	section {
		margin-bottom: 2.5rem;
		padding: 1.25rem;
		background: var(--bg-elevated);
		border: 1px solid var(--border);
		border-radius: 8px;
	}
	.section-header {
		display: flex;
		align-items: center;
		justify-content: space-between;
		margin-bottom: 0.5rem;
	}
	h2 {
		margin: 0;
		font-size: 1.1rem;
	}
	.intro {
		margin: 0 0 1rem;
		color: var(--text-muted);
		font-size: 0.8rem;
	}
	.actions {
		display: flex;
		gap: 0.5rem;
	}
	button {
		padding: 0.35rem 0.85rem;
		background: var(--bg-subtle);
		border: 1px solid var(--border);
		border-radius: 4px;
		color: var(--text);
		font-size: 0.85rem;
		cursor: pointer;
	}
	button:hover:not(:disabled) {
		background: var(--border);
	}
	button:disabled {
		opacity: 0.5;
		cursor: not-allowed;
	}
	button.danger {
		color: var(--danger);
		border-color: var(--danger);
	}
	button.danger:hover:not(:disabled) {
		background: rgba(233, 79, 79, 0.18);
	}
	button.save {
		background: var(--accent-strong);
		border-color: var(--accent);
		color: var(--text-strong);
	}
	button.save:hover:not(:disabled) {
		background: var(--accent);
	}
	table {
		width: 100%;
		border-collapse: collapse;
		font-size: 0.85rem;
	}
	th,
	td {
		text-align: left;
		padding: 0.5rem 0.6rem;
		border-bottom: 1px solid var(--border);
		vertical-align: middle;
	}
	th {
		color: var(--text-muted);
		font-weight: 500;
		font-size: 0.8rem;
	}
	code {
		font-family: 'SFMono-Regular', Consolas, monospace;
		font-size: 0.85em;
		background: var(--bg);
		padding: 0.05rem 0.3rem;
		border-radius: 3px;
	}
	.hex {
		color: var(--text-muted);
		margin-left: 0.4rem;
	}
	.swatch {
		display: inline-block;
		width: 1rem;
		height: 1rem;
		border-radius: 3px;
		vertical-align: middle;
		border: 1px solid var(--border);
	}
	.dim {
		color: var(--text-muted);
	}
	.row-actions {
		display: flex;
		gap: 0.3rem;
		justify-content: flex-end;
	}
	input[type='text'] {
		width: 100%;
		padding: 0.3rem 0.5rem;
		background: var(--bg);
		border: 1px solid var(--border);
		border-radius: 4px;
		color: var(--text);
		font: inherit;
	}
	input[type='text']:focus {
		outline: none;
		border-color: var(--accent);
	}
	input[type='color'] {
		width: 2.2rem;
		height: 1.6rem;
		padding: 0;
		border: 1px solid var(--border);
		border-radius: 4px;
		background: var(--bg);
		cursor: pointer;
	}
	.empty {
		color: var(--text-muted);
		font-size: 0.875rem;
	}

	/* modal */
	.ov {
		position: fixed;
		inset: 0;
		background: rgba(0, 0, 0, 0.6);
		z-index: 100;
		display: flex;
		align-items: center;
		justify-content: center;
		padding: 1rem;
	}
	.modal {
		background: var(--bg-elevated);
		border: 1px solid var(--border);
		border-radius: 10px;
		width: 100%;
		max-width: 460px;
		padding: 1.2rem 1.4rem;
		box-shadow: 0 12px 36px rgba(0, 0, 0, 0.6);
		color: var(--text);
	}
	.modal-title {
		margin: 0 0 0.85rem;
		font-size: 1rem;
		font-weight: 600;
		color: var(--text-strong);
	}
	.modal-msg {
		margin: 0 0 1rem;
		font-size: 0.875rem;
		color: var(--text);
	}
	.form {
		display: flex;
		flex-direction: column;
		gap: 0.75rem;
		margin-bottom: 0.5rem;
	}
	.form label {
		display: flex;
		align-items: center;
		gap: 0.6rem;
		font-size: 0.875rem;
	}
	.form label > span {
		flex: 0 0 5rem;
		color: var(--text-muted);
	}
	.form-note {
		margin: 0 0 1rem;
		font-size: 0.8rem;
		color: var(--text-muted);
	}
	.modal-actions {
		display: flex;
		gap: 0.5rem;
		justify-content: flex-end;
	}
	.btn-yes {
		padding: 0.4rem 1.1rem;
		background: rgba(31, 111, 235, 0.18);
		border: 1px solid var(--accent);
		border-radius: 6px;
		color: var(--accent);
		font-size: 0.875rem;
		cursor: pointer;
	}
	.btn-yes:hover:not(:disabled) {
		background: rgba(31, 111, 235, 0.32);
	}
	.btn-yes.danger {
		background: rgba(233, 79, 79, 0.18);
		border-color: var(--danger);
		color: var(--danger);
	}
	.btn-yes.danger:hover:not(:disabled) {
		background: rgba(233, 79, 79, 0.32);
	}
	.btn-no {
		padding: 0.4rem 1rem;
		background: transparent;
		border: 1px solid var(--border);
		border-radius: 6px;
		color: var(--text-muted);
		font-size: 0.875rem;
		cursor: pointer;
	}
	.btn-no:hover:not(:disabled) {
		background: var(--bg-subtle);
	}
</style>
