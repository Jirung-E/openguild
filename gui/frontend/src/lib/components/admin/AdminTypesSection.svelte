<script lang="ts">
	import { onMount } from 'svelte';
	import { adminApi, type QuestTypeWithCount } from '$lib/api/admin';
	// DEV-119: window.confirm() 대신 인앱 ConfirmDialog.
	import ConfirmDialog from '../ConfirmDialog.svelte';
	// DEV-205 모듈5: 타입 관리 i18n.
	import { locale, t } from '$lib/stores/locale';

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
		'#4a90d9',
		'#e94f4f',
		'#7bb87f',
		'#f5a623',
		'#8e4ec6',
		'#1abc9c',
		'#e91e63',
		'#34495e',
		'#16a085',
		'#d35400',
		'#2c3e50',
		'#c0392b'
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
	// DEV-119: rename cascade 확인 — 이전엔 window.confirm() (Tauri 에서 silent return).
	let confirmRename = $state<{ oldPrefix: string; newPrefix: string; count: number } | null>(null);

	onMount(refresh);

	export async function refresh() {
		try {
			types = await adminApi.listTypes();
		} catch (e) {
			onmessage({ kind: 'error', text: `${t('adminTypes.listLoadFailedPre', $locale)}${e}` });
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
		const renaming = !!newPrefix && newPrefix !== editing;
		// BUG-018 / DEV-119: prefix rename cascade 확인 — 인앱 모달.
		if (renaming) {
			const count = types.find((t) => t.prefix === editing)?.quest_count ?? 0;
			confirmRename = { oldPrefix: editing, newPrefix, count };
			return; // 모달 onconfirm 이 doSaveEdit 호출.
		}
		await doSaveEdit(false, '');
	}

	async function doSaveEdit(renaming: boolean, newPrefix: string) {
		if (!editing) return;
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
					? `${t('adminTypes.updatedCascadePre', $locale)}${editing}' → '${newPrefix}${t('adminTypes.updatedCascadeMid', $locale)}`
					: `${t('adminTypes.updatedCascadePre', $locale)}${editing}${t('adminTypes.updatedSimplePost', $locale)}`
			});
			editing = null;
			await refresh();
		} catch (e) {
			onmessage({ kind: 'error', text: `${t('adminTypes.updateFailedPre', $locale)}${e}` });
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
			onmessage({ kind: 'error', text: t('adminTypes.prefixRequired', $locale) });
			return;
		}
		busy = true;
		try {
			await adminApi.createType({
				prefix,
				color: newColor,
				description: newDesc.trim() || null
			});
			onmessage({
				kind: 'success',
				text: `${t('adminTypes.addedPre', $locale)}${prefix}${t('adminTypes.addedPost', $locale)}`
			});
			creating = false;
			await refresh();
		} catch (e) {
			onmessage({ kind: 'error', text: `${t('adminTypes.addFailedPre', $locale)}${e}` });
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
			onmessage({
				kind: 'success',
				text: `${t('adminTypes.deletedPre', $locale)}${target.prefix}${t('adminTypes.deletedPost', $locale)}`
			});
			await refresh();
		} catch (e) {
			onmessage({ kind: 'error', text: `${t('adminTypes.deleteFailedPre', $locale)}${e}` });
		} finally {
			busy = false;
		}
	}
</script>

<section>
	<div class="section-header">
		<h2>Quest Types</h2>
		<div class="actions">
			<button onclick={openCreate} disabled={busy}>{t('adminTypes.newType', $locale)}</button>
			<button onclick={refresh} disabled={busy}>{t('admin.refresh', $locale)}</button>
		</div>
	</div>

	{#if types.length === 0}
		<p class="empty">{t('adminTypes.empty', $locale)}</p>
	{:else}
		<!-- BUG-143: 좁은 폭에서 셀 내용이 줄바꿈되며 깨지는 대신 표 자체를
		     가로 스크롤 — 셀은 nowrap(아래 CSS)로 고정. -->
		<div class="table-wrap">
			<table>
				<thead>
					<tr>
						<th style="width: 6ch">prefix</th>
						<th style="width: 5ch">{t('adminTypes.colColor', $locale)}</th>
						<th>{t('adminTypes.colDesc', $locale)}</th>
						<th style="width: 8ch">{t('adminTypes.colInUse', $locale)}</th>
						<th style="width: 14ch"></th>
					</tr>
				</thead>
				<tbody>
					{#each types as ty (ty.id)}
						<tr>
							{#if editing === ty.prefix}
								<!-- BUG-018: prefix 도 inline 편집 가능. 변경 시 cascade confirm. -->
								<td>
									<input
										type="text"
										bind:value={editPrefix}
										maxlength="6"
										pattern="[A-Z0-9]+"
										title={t('adminTypes.prefixTitle', $locale)}
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
										placeholder={t('adminTypes.descPlaceholder', $locale)}
										disabled={busy}
									/>
								</td>
								<td class="count">{ty.quest_count}</td>
								<td class="row-actions">
									<button class="save" onclick={saveEdit} disabled={busy}
										>{t('common.save', $locale)}</button
									>
									<button onclick={cancelEdit} disabled={busy}>{t('common.cancel', $locale)}</button
									>
								</td>
							{:else}
								<td><code>{ty.prefix}</code></td>
								<td>
									<span class="swatch" style="background: {ty.color}"></span>
									<code class="hex">{ty.color}</code>
								</td>
								<td class="desc">{ty.description ?? ''}</td>
								<td class="count">{ty.quest_count}</td>
								<td class="row-actions">
									<button onclick={() => startEdit(ty)} disabled={busy}
										>{t('adminTypes.edit', $locale)}</button
									>
									<button
										class="danger"
										onclick={() => askDelete(ty)}
										disabled={busy || ty.quest_count > 0}
										title={ty.quest_count > 0
											? `${t('adminTypes.inUsePre', $locale)}${ty.quest_count}${t('adminTypes.inUsePost', $locale)}`
											: t('detail.delete', $locale)}
									>
										{t('detail.delete', $locale)}
									</button>
								</td>
							{/if}
						</tr>
					{/each}
				</tbody>
			</table>
		</div>
		<p class="hint">{t('adminTypes.renameCascadeHint', $locale)}</p>
	{/if}
</section>

{#if creating}
	<div class="ov" role="presentation">
		<div class="modal" role="dialog" aria-modal="true" tabindex="-1">
			<h3 class="modal-title">{t('adminTypes.newTypeTitle', $locale)}</h3>
			<div class="form">
				<label>
					<span>prefix</span>
					<input
						type="text"
						bind:value={newPrefix}
						placeholder={t('adminTypes.prefixPlaceholder', $locale)}
						maxlength="6"
						disabled={busy}
					/>
				</label>
				<label>
					<span>{t('adminTypes.colColor', $locale)}</span>
					<input type="color" bind:value={newColor} disabled={busy} />
					<code class="hex">{newColor}</code>
				</label>
				<label>
					<span>{t('adminTypes.colDesc', $locale)}</span>
					<input
						type="text"
						bind:value={newDesc}
						placeholder={t('adminTypes.descShortPlaceholder', $locale)}
						disabled={busy}
					/>
				</label>
			</div>
			<div class="modal-actions">
				<button class="btn-yes" onclick={doCreate} disabled={busy}
					>{t('common.add', $locale)}</button
				>
				<button class="btn-no" onclick={() => (creating = false)} disabled={busy}
					>{t('common.cancel', $locale)}</button
				>
			</div>
		</div>
	</div>
{/if}

{#if confirmDelete}
	<div class="ov" role="presentation">
		<div class="modal" role="dialog" aria-modal="true" tabindex="-1">
			<h3 class="modal-title">{t('adminTypes.deleteTypeTitle', $locale)}</h3>
			<p class="modal-msg">
				<strong>{confirmDelete.prefix}</strong>{t('adminTypes.deleteMsg1', $locale)}<br />
				{t('adminTypes.deleteMsg2', $locale)}<code>.guild/types/{confirmDelete.prefix}.toml</code
				>{t('adminTypes.deleteMsg3', $locale)}
			</p>
			<div class="modal-actions">
				<button class="btn-yes danger" onclick={doDelete} disabled={busy}
					>{t('detail.delete', $locale)}</button
				>
				<button class="btn-no" onclick={() => (confirmDelete = null)} disabled={busy}
					>{t('common.cancel', $locale)}</button
				>
			</div>
		</div>
	</div>
{/if}

<!-- DEV-119: prefix rename cascade 확인 — 인앱 모달. -->
<ConfirmDialog
	open={confirmRename !== null}
	title={t('adminTypes.renamePrefixTitle', $locale)}
	message={confirmRename
		? `${t('adminTypes.updatedCascadePre', $locale)}${confirmRename.oldPrefix}' → '${confirmRename.newPrefix}${t('adminTypes.renameConfirmMid', $locale)}` +
			`${t('adminTypes.renameConfirmCount1', $locale)}${confirmRename.count}${t('adminTypes.renameConfirmCount2', $locale)}` +
			`${t('adminTypes.renameConfirmMention1', $locale)}${confirmRename.oldPrefix}${t('adminTypes.renameConfirmMention2', $locale)}`
		: ''}
	confirmLabel={t('common.change', $locale)}
	danger
	onconfirm={() => {
		const r = confirmRename;
		confirmRename = null;
		if (r) doSaveEdit(true, r.newPrefix);
	}}
	oncancel={() => (confirmRename = null)}
/>

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
		/* BUG-196: 라벨이 긴 언어(영어)에서 제목+버튼이 한 줄을 넘겨 섹션 밖으로
		   삐져나왔다. 줄바꿈을 허용해 버튼 묶음이 아래로 내려가게. */
		flex-wrap: wrap;
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
		flex-wrap: wrap;
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
	/* BUG-143: 좁은 폭에선 셀 줄바꿈(스와치/hex 쌓임, CJK 버튼 글자 세로
	   꺾임) 대신 표를 가로 스크롤. */
	.table-wrap {
		overflow-x: auto;
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
		/* BUG-143: CJK 는 word-break 없이도 글자 단위로 꺾임 — 명시 nowrap. */
		white-space: nowrap;
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
		/* 설명은 길 수 있음 — 유일하게 줄바꿈 허용(대신 최소 폭 확보). */
		white-space: normal;
		min-width: 14rem;
	}
	.count {
		color: var(--text-muted);
	}
	/* BUG-143: td 에 display:flex 를 걸면 table-cell 렌더가 깨져 행 구분선이
	   다른 컬럼과 어긋나 보였음 — 일반 셀 + 우측 정렬 + 버튼 간격은 margin. */
	.row-actions {
		text-align: right;
	}
	.row-actions button + button {
		margin-left: 0.3rem;
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
		max-width: calc(28.75rem * var(--popup-scale, 1)); /* BUG-064 */
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
