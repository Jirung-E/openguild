<script lang="ts">
	import { onMount } from 'svelte';
	import { adminApi, type QuestTypeWithCount } from '$lib/api/admin';

	type Msg = { kind: 'info' | 'success' | 'error'; text: string } | null;
	let { onmessage }: { onmessage: (m: Msg) => void } = $props();

	let types = $state<QuestTypeWithCount[]>([]);
	let busy = $state(false);
	let editing: string | null = $state(null); // 원래 prefix (key).
	let editPrefix = $state(''); // BUG-018: rename 도 inline.
	let editColor = $state('');
	let editDesc = $state('');

	// 추가 모달.
	let creating = $state(false);
	let newPrefix = $state('');
	let newColor = $state('#4a90d9');
	let newDesc = $state('');

	// DEV-014 후속 (fix5): 추가 시 기본 색을 매번 다르게.
	// 기존 사용 중인 색을 피한 다음 palette 색을 고름.
	const COLOR_PALETTE = [
		'#4a90d9', '#e94f4f', '#7bb87f', '#f5a623',
		'#8e4ec6', '#1abc9c', '#e91e63', '#34495e',
		'#16a085', '#d35400', '#2c3e50', '#c0392b'
	];
	function pickNextColor(): string {
		const used = new Set(types.map((t) => t.color.toLowerCase()));
		for (const c of COLOR_PALETTE) {
			if (!used.has(c.toLowerCase())) return c;
		}
		// 모두 사용 중이면 길이 기반 cycle.
		return COLOR_PALETTE[types.length % COLOR_PALETTE.length];
	}

	// 삭제 확인 모달.
	let confirmDelete: QuestTypeWithCount | null = $state(null);


	onMount(refresh);

	export async function refresh() {
		try {
			types = await adminApi.listTypes();
		} catch (e) {
			onmessage({ kind: 'error', text: `type 목록 조회 실패: ${e}` });
		}
	}

	function startEdit(t: QuestTypeWithCount) {
		editing = t.prefix;
		editPrefix = t.prefix;
		editColor = t.color;
		editDesc = t.description ?? '';
	}
	function cancelEdit() {
		editing = null;
	}

	async function saveEdit() {
		if (!editing) return;
		const newPrefix = editPrefix.trim().toUpperCase();
		const renaming = newPrefix && newPrefix !== editing;
		// BUG-018: prefix 변경 시 cascade 확인.
		if (renaming) {
			const count = types.find((t) => t.prefix === editing)?.quest_count ?? 0;
			const ok = window.confirm(
				`'${editing}' → '${newPrefix}' 로 이름 변경.\n\n` +
					`이 type 의 모든 quest (${count}개) 의 slug 가 cascade 됩니다 ` +
					`(파일명 / frontmatter / DB history).\n\n` +
					`다른 quest 본문 안의 '${editing}-NNN' mention 은 자동 갱신되지 않습니다 ` +
					`— 직접 검색/수정 필요.\n\n계속할까요?`
			);
			if (!ok) return;
		}
		busy = true;
		try {
			await adminApi.updateType(editing, {
				new_prefix: renaming ? newPrefix : undefined,
				color: editColor,
				description: editDesc.trim() === '' ? null : editDesc
			});
			onmessage({
				kind: 'success',
				text: renaming
					? `'${editing}' → '${newPrefix}' 갱신 완료 (cascade)`
					: `'${editing}' 갱신됨`
			});
			editing = null;
			await refresh();
		} catch (e) {
			onmessage({ kind: 'error', text: `갱신 실패: ${e}` });
		} finally {
			busy = false;
		}
	}

	function openCreate() {
		newPrefix = '';
		newColor = pickNextColor();
		newDesc = '';
		creating = true;
	}

	async function doCreate() {
		const prefix = newPrefix.trim().toUpperCase();
		if (!prefix) {
			onmessage({ kind: 'error', text: 'prefix 를 입력하세요.' });
			return;
		}
		busy = true;
		try {
			await adminApi.createType({
				prefix,
				color: newColor,
				description: newDesc.trim() || null
			});
			onmessage({ kind: 'success', text: `'${prefix}' 추가됨` });
			creating = false;
			await refresh();
		} catch (e) {
			onmessage({ kind: 'error', text: `추가 실패: ${e}` });
		} finally {
			busy = false;
		}
	}

	function askDelete(t: QuestTypeWithCount) {
		confirmDelete = t;
	}

	async function doDelete() {
		if (!confirmDelete) return;
		const target = confirmDelete;
		confirmDelete = null;
		busy = true;
		try {
			await adminApi.deleteType(target.prefix);
			onmessage({ kind: 'success', text: `'${target.prefix}' 삭제됨` });
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
		<h2>Quest Types</h2>
		<div class="actions">
			<button onclick={openCreate} disabled={busy}>+ 새 type</button>
			<button onclick={refresh} disabled={busy}>새로고침</button>
		</div>
	</div>

	{#if types.length === 0}
		<p class="empty">type 없음.</p>
	{:else}
		<table>
			<thead>
				<tr>
					<th style="width: 6ch">prefix</th>
					<th style="width: 5ch">색</th>
					<th>설명</th>
					<th style="width: 8ch">사용 중</th>
					<th style="width: 14ch"></th>
				</tr>
			</thead>
			<tbody>
				{#each types as t (t.id)}
					<tr>
						{#if editing === t.prefix}
							<!-- BUG-018: prefix 도 inline 편집 가능. 변경 시 cascade confirm. -->
							<td>
								<input
									type="text"
									bind:value={editPrefix}
									maxlength="6"
									pattern="[A-Z0-9]+"
									title="대문자/숫자 1~6자"
									disabled={busy}
									class="prefix-input"
								/>
							</td>
							<td>
								<input type="color" bind:value={editColor} disabled={busy} />
							</td>
							<td>
								<input
									type="text"
									bind:value={editDesc}
									placeholder="(없음)"
									disabled={busy}
								/>
							</td>
							<td class="count">{t.quest_count}</td>
							<td class="row-actions">
								<button class="save" onclick={saveEdit} disabled={busy}>저장</button>
								<button onclick={cancelEdit} disabled={busy}>취소</button>
							</td>
						{:else}
							<td><code>{t.prefix}</code></td>
							<td>
								<span class="swatch" style="background: {t.color}"></span>
								<code class="hex">{t.color}</code>
							</td>
							<td class="desc">{t.description ?? ''}</td>
							<td class="count">{t.quest_count}</td>
							<td class="row-actions">
								<button onclick={() => startEdit(t)} disabled={busy}>수정</button>
								<button
									class="danger"
									onclick={() => askDelete(t)}
									disabled={busy || t.quest_count > 0}
									title={t.quest_count > 0
										? `사용 중 quest ${t.quest_count}개 — 먼저 이동`
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
		<p class="hint">prefix 자체 rename 은 quest slug cascade 라 지원 안 함.</p>
	{/if}
</section>

{#if creating}
	<div class="ov" role="presentation">
		<div class="modal" role="dialog" aria-modal="true" tabindex="-1">
			<h3 class="modal-title">새 quest type</h3>
			<div class="form">
				<label>
					<span>prefix</span>
					<input
						type="text"
						bind:value={newPrefix}
						placeholder="DEV / BUG / REQ 같은 1~6자"
						maxlength="6"
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
						bind:value={newDesc}
						placeholder="(선택) 짧은 설명"
						disabled={busy}
					/>
				</label>
			</div>
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
			<h3 class="modal-title">Type 삭제</h3>
			<p class="modal-msg">
				<strong>{confirmDelete.prefix}</strong> type 을 삭제할까요? <br />
				디스크의 <code>.guild/types/{confirmDelete.prefix}.toml</code> 파일도 함께 제거됩니다.
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
	/* DEV-074 fix6 / DEV-116 fix: `--btn-primary-*` 토큰. */
	button.save {
		background: var(--btn-primary-bg);
		border-color: var(--btn-primary-border);
		color: var(--btn-primary-text);
	}
	button.save:hover:not(:disabled) {
		background: var(--btn-primary-bg-hover);
		border-color: var(--btn-primary-border-hover);
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
	.desc {
		color: var(--text);
	}
	.count {
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
	input[type='text'].prefix-input {
		width: 7ch;
		text-transform: uppercase;
		font-family: 'SFMono-Regular', Consolas, monospace;
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
	.hint {
		margin-top: 0.75rem;
		color: var(--text-muted);
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
	.modal-msg strong {
		color: var(--text-strong);
	}
	.form {
		display: flex;
		flex-direction: column;
		gap: 0.75rem;
		margin-bottom: 1rem;
	}
	.form label {
		display: flex;
		align-items: center;
		gap: 0.6rem;
		font-size: 0.875rem;
	}
	.form label > span {
		flex: 0 0 4rem;
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
