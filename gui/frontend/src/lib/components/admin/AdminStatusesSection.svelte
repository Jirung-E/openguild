<script lang="ts">
	import { onMount } from 'svelte';
	import { adminApi, type QuestStatusWithCount } from '$lib/api/admin';

	type Msg = { kind: 'info' | 'success' | 'error'; text: string } | null;
	let { onmessage }: { onmessage: (m: Msg) => void } = $props();

	let statuses = $state<QuestStatusWithCount[]>([]);
	let busy = $state(false);
	let editing: string | null = $state(null); // slug.
	let editNameEn = $state('');
	let editNameKo = $state('');
	let editColor = $state('');
	let editSortOrder = $state(0);

	let creating = $state(false);
	let newNameEn = $state('');
	let newNameKo = $state('');
	let newColor = $state('#8b95a1');
	// sort_order 는 backend 가 max+1 로 자동.

	let confirmDelete: QuestStatusWithCount | null = $state(null);

	onMount(refresh);

	export async function refresh() {
		try {
			statuses = await adminApi.listStatuses();
		} catch (e) {
			onmessage({ kind: 'error', text: `status 목록 조회 실패: ${e}` });
		}
	}

	function startEdit(s: QuestStatusWithCount) {
		editing = s.slug;
		editNameEn = s.name_en;
		editNameKo = s.name_ko;
		editColor = s.color;
		editSortOrder = s.sort_order;
	}
	function cancelEdit() {
		editing = null;
	}

	async function saveEdit() {
		if (!editing) return;
		busy = true;
		try {
			await adminApi.updateStatus(editing, {
				name_en: editNameEn,
				name_ko: editNameKo,
				color: editColor,
				sort_order: editSortOrder
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
		newNameEn = '';
		newNameKo = '';
		newColor = '#8b95a1';
		creating = true;
	}

	async function doCreate() {
		const en = newNameEn.trim();
		const ko = newNameKo.trim();
		if (!en || !ko) {
			onmessage({ kind: 'error', text: 'name_en / name_ko 둘 다 필수.' });
			return;
		}
		busy = true;
		try {
			await adminApi.createStatus({
				name_en: en,
				name_ko: ko,
				color: newColor
			});
			onmessage({ kind: 'success', text: `'${en}' 추가됨` });
			creating = false;
			await refresh();
		} catch (e) {
			onmessage({ kind: 'error', text: `추가 실패: ${e}` });
		} finally {
			busy = false;
		}
	}

	function askDelete(s: QuestStatusWithCount) {
		confirmDelete = s;
	}

	async function doDelete() {
		if (!confirmDelete) return;
		const target = confirmDelete;
		confirmDelete = null;
		busy = true;
		try {
			await adminApi.deleteStatus(target.slug);
			onmessage({ kind: 'success', text: `'${target.slug}' 삭제됨` });
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
		<h2>Quest Statuses</h2>
		<div class="actions">
			<button onclick={openCreate} disabled={busy}>+ 새 status</button>
			<button onclick={refresh} disabled={busy}>새로고침</button>
		</div>
	</div>

	{#if statuses.length === 0}
		<p class="empty">status 없음.</p>
	{:else}
		<table>
			<thead>
				<tr>
					<th style="width: 4ch">#</th>
					<th style="width: 12ch">slug</th>
					<th>name_en</th>
					<th>name_ko</th>
					<th style="width: 5ch">색</th>
					<th style="width: 8ch">사용 중</th>
					<th style="width: 14ch"></th>
				</tr>
			</thead>
			<tbody>
				{#each statuses as s (s.id)}
					<tr>
						{#if editing === s.slug}
							<td>
								<input
									type="number"
									bind:value={editSortOrder}
									disabled={busy}
									class="num"
								/>
							</td>
							<td><code class="frozen">{s.slug}</code></td>
							<td><input type="text" bind:value={editNameEn} disabled={busy} /></td>
							<td><input type="text" bind:value={editNameKo} disabled={busy} /></td>
							<td><input type="color" bind:value={editColor} disabled={busy} /></td>
							<td class="count">{s.quest_count}</td>
							<td class="row-actions">
								<button class="save" onclick={saveEdit} disabled={busy}>저장</button>
								<button onclick={cancelEdit} disabled={busy}>취소</button>
							</td>
						{:else}
							<td class="num">{s.sort_order}</td>
							<td><code>{s.slug}</code></td>
							<td>{s.name_en}</td>
							<td>{s.name_ko}</td>
							<td>
								<span class="swatch" style="background: {s.color}"></span>
								<code class="hex">{s.color}</code>
							</td>
							<td class="count">{s.quest_count}</td>
							<td class="row-actions">
								<button onclick={() => startEdit(s)} disabled={busy}>수정</button>
								<button
									class="danger"
									onclick={() => askDelete(s)}
									disabled={busy || s.quest_count > 0}
									title={s.quest_count > 0
										? `사용 중 quest ${s.quest_count}개 — 먼저 이동`
										: '삭제'}
								>
									삭제
								</button>
							</td>
						{/if}
					</tr>
				{/each}
			</tbody>
		</table>
		<p class="hint">
			slug 는 frozen — history / 파일 frontmatter 가 참조하므로 rename 안 됨.
			sort_order 변경 시 파일명도 자동 rename.
		</p>
	{/if}
</section>

{#if creating}
	<div class="ov" role="presentation">
		<div class="modal" role="dialog" aria-modal="true" tabindex="-1">
			<h3 class="modal-title">새 quest status</h3>
			<div class="form">
				<label>
					<span>name_en</span>
					<input
						type="text"
						bind:value={newNameEn}
						placeholder="Blocked / In Review 등"
						disabled={busy}
					/>
				</label>
				<label>
					<span>name_ko</span>
					<input
						type="text"
						bind:value={newNameKo}
						placeholder="막힘 / 리뷰 중 등"
						disabled={busy}
					/>
				</label>
				<label>
					<span>색</span>
					<input type="color" bind:value={newColor} disabled={busy} />
					<code class="hex">{newColor}</code>
				</label>
			</div>
			<p class="form-note">
				sort_order 는 현재 max + 1 로 자동 부여. 만든 뒤 수정 가능.
			</p>
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
			<h3 class="modal-title">Status 삭제</h3>
			<p class="modal-msg">
				<strong>{confirmDelete.name_en}</strong> (<code>{confirmDelete.slug}</code>) 을 삭제할까요?
				<br />
				디스크의 <code>.guild/statuses/{confirmDelete.sort_order}-{confirmDelete.slug}.toml</code>
				파일도 함께 제거됩니다.
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
		background: #1a1a2e;
		border: 1px solid #2a2a4a;
		border-radius: 8px;
	}
	.section-header {
		display: flex;
		align-items: center;
		justify-content: space-between;
		margin-bottom: 1rem;
	}
	h2 {
		margin: 0;
		font-size: 1.1rem;
	}
	.actions {
		display: flex;
		gap: 0.5rem;
	}
	button {
		padding: 0.35rem 0.85rem;
		background: #2a2a4a;
		border: 1px solid #3a3a6a;
		border-radius: 4px;
		color: #c9d1d9;
		font-size: 0.85rem;
		cursor: pointer;
	}
	button:hover:not(:disabled) {
		background: #3a3a6a;
	}
	button:disabled {
		opacity: 0.5;
		cursor: not-allowed;
	}
	button.danger {
		color: #e94f4f;
		border-color: #5a2a2a;
	}
	button.danger:hover:not(:disabled) {
		background: rgba(233, 79, 79, 0.18);
	}
	button.save {
		background: #1f6feb;
		border-color: #2f81f7;
		color: #fff;
	}
	button.save:hover:not(:disabled) {
		background: #2f81f7;
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
		border-bottom: 1px solid #2a2a4a;
		vertical-align: middle;
	}
	th {
		color: #8b949e;
		font-weight: 500;
		font-size: 0.8rem;
	}
	code {
		font-family: 'SFMono-Regular', Consolas, monospace;
		font-size: 0.85em;
		background: #0d1117;
		padding: 0.05rem 0.3rem;
		border-radius: 3px;
	}
	code.frozen {
		color: #8b949e;
	}
	.hex {
		color: #8b949e;
		margin-left: 0.4rem;
	}
	.swatch {
		display: inline-block;
		width: 1rem;
		height: 1rem;
		border-radius: 3px;
		vertical-align: middle;
		border: 1px solid #2a2a4a;
	}
	.num {
		text-align: center;
		color: #8b949e;
	}
	.count {
		color: #8b949e;
	}
	.row-actions {
		display: flex;
		gap: 0.3rem;
		justify-content: flex-end;
	}
	input[type='text'],
	input[type='number'] {
		width: 100%;
		padding: 0.3rem 0.5rem;
		background: #0d1117;
		border: 1px solid #30363d;
		border-radius: 4px;
		color: #c9d1d9;
		font: inherit;
	}
	input[type='number'].num {
		width: 4ch;
		padding: 0.3rem 0.3rem;
		text-align: center;
	}
	input[type='text']:focus,
	input[type='number']:focus {
		outline: none;
		border-color: #58a6ff;
	}
	input[type='color'] {
		width: 2.2rem;
		height: 1.6rem;
		padding: 0;
		border: 1px solid #30363d;
		border-radius: 4px;
		background: #0d1117;
		cursor: pointer;
	}
	.empty {
		color: #8b95a1;
		font-size: 0.875rem;
	}
	.hint {
		margin-top: 0.75rem;
		color: #8b949e;
		font-size: 0.8rem;
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
		background: #161b22;
		border: 1px solid #30363d;
		border-radius: 10px;
		width: 100%;
		max-width: 460px;
		padding: 1.2rem 1.4rem;
		box-shadow: 0 12px 36px rgba(0, 0, 0, 0.6);
		color: #c9d1d9;
	}
	.modal-title {
		margin: 0 0 0.85rem;
		font-size: 1rem;
		font-weight: 600;
		color: #e6edf3;
	}
	.modal-msg {
		margin: 0 0 1rem;
		font-size: 0.875rem;
		color: #c9d1d9;
	}
	.modal-msg strong {
		color: #e6edf3;
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
		color: #8b949e;
	}
	.form-note {
		margin: 0 0 1rem;
		font-size: 0.8rem;
		color: #8b949e;
	}
	.modal-actions {
		display: flex;
		gap: 0.5rem;
		justify-content: flex-end;
	}
	.btn-yes {
		padding: 0.4rem 1.1rem;
		background: rgba(31, 111, 235, 0.18);
		border: 1px solid #2f81f7;
		border-radius: 6px;
		color: #58a6ff;
		font-size: 0.875rem;
		cursor: pointer;
	}
	.btn-yes:hover:not(:disabled) {
		background: rgba(31, 111, 235, 0.32);
	}
	.btn-yes.danger {
		background: rgba(233, 79, 79, 0.18);
		border-color: #e94f4f;
		color: #e94f4f;
	}
	.btn-yes.danger:hover:not(:disabled) {
		background: rgba(233, 79, 79, 0.32);
	}
	.btn-no {
		padding: 0.4rem 1rem;
		background: transparent;
		border: 1px solid #30363d;
		border-radius: 6px;
		color: #8b949e;
		font-size: 0.875rem;
		cursor: pointer;
	}
	.btn-no:hover:not(:disabled) {
		background: #21262d;
	}
</style>
