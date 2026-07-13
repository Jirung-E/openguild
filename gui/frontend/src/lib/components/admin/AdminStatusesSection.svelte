<script lang="ts">
	import { onMount } from 'svelte';
	import { adminApi, type QuestStatusWithCount } from '$lib/api/admin';
	// DEV-119: window.confirm() 대신 인앱 ConfirmDialog.
	import ConfirmDialog from '../ConfirmDialog.svelte';
	// DEV-205 모듈5: 상태 관리 i18n.
	import { locale, t } from '$lib/stores/locale';

	type Msg = { kind: 'info' | 'success' | 'error'; text: string } | null;
	let { onmessage }: { onmessage: (m: Msg) => void } = $props();

	let statuses = $state<QuestStatusWithCount[]>([]);
	let busy = $state(false);
	let editing: string | null = $state(null); // 원래 slug.
	let editSlug = $state(''); // BUG-018: slug rename inline.
	let editNameEn = $state('');
	let editNameKo = $state('');
	let editColor = $state('');
	let editCountsAsDone = $state(false); // DEV-093

	let creating = $state(false);
	let newNameEn = $state('');
	let newNameKo = $state('');
	let newColor = $state('#8b95a1');
	// sort_order 는 backend 가 max+1 로 자동.

	// DEV-014 후속 (fix5): 추가 시 기본 색 다양화 — 사용 중 색 회피.
	// BUG-061: palette 는 반드시 구체 hex — status 색은 TOML 에 저장되는
	// 데이터라 CSS var() 불가. var 문자열이 <input type="color"> 에 binding
	// 되면 검은색 fallback (사용자 보고 '새 status 가 무조건 검은색').
	const COLOR_PALETTE = [
		'#8b95a1',
		'#4a90d9',
		'#7bb87f',
		'#f5a623',
		'#e94f4f',
		'#8e4ec6',
		'#1abc9c',
		'#e91e63',
		'#34495e',
		'#16a085',
		'#d35400',
		'#2c3e50'
	];
	function pickNextColor(): string {
		const used = new Set(statuses.map((s) => s.color.toLowerCase()));
		for (const c of COLOR_PALETTE) {
			if (!used.has(c.toLowerCase())) return c;
		}
		return COLOR_PALETTE[statuses.length % COLOR_PALETTE.length];
	}

	let confirmDelete: QuestStatusWithCount | null = $state(null);
	// DEV-119: rename cascade 확인 — 이전엔 window.confirm() 사용했으나 Tauri
	// WebView 에서 silent return → 클릭 한 번에 영구 cascade. 인앱 모달로 교체.
	let confirmRename = $state<{ oldSlug: string; newSlug: string; count: number } | null>(null);

	onMount(refresh);

	export async function refresh() {
		try {
			statuses = await adminApi.listStatuses();
		} catch (e) {
			onmessage({ kind: 'error', text: `${t('adminStatuses.listLoadFailedPre', $locale)}${e}` });
		}
	}

	function startEdit(s: QuestStatusWithCount) {
		editing = s.slug;
		editSlug = s.slug;
		editNameEn = s.name_en;
		editNameKo = s.name_ko;
		editColor = s.color;
		editCountsAsDone = s.counts_as_done ?? false; // DEV-093
	}
	function cancelEdit() {
		editing = null;
	}

	async function saveEdit() {
		if (!editing) return;
		const newSlug = editSlug.trim();
		const renaming = !!newSlug && newSlug !== editing;
		// BUG-018 / DEV-119: slug 변경 시 cascade 확인 — 인앱 모달.
		if (renaming) {
			const count = statuses.find((s) => s.slug === editing)?.quest_count ?? 0;
			confirmRename = { oldSlug: editing, newSlug, count };
			return; // 모달 onconfirm 이 doSaveEdit 호출.
		}
		await doSaveEdit(false, '');
	}

	async function doSaveEdit(renaming: boolean, newSlug: string) {
		if (!editing) return;
		busy = true;
		try {
			await adminApi.updateStatus(editing, {
				new_slug: renaming ? newSlug : undefined,
				name_en: editNameEn,
				name_ko: editNameKo,
				color: editColor,
				counts_as_done: editCountsAsDone // DEV-093
			});
			onmessage({
				kind: 'success',
				text: renaming
					? `${t('adminTypes.updatedCascadePre', $locale)}${editing}' → '${newSlug}${t('adminTypes.updatedCascadeMid', $locale)}`
					: `${t('adminTypes.updatedCascadePre', $locale)}${editing}${t('adminTypes.updatedSimplePost', $locale)}`
			});
			editing = null;
			await refresh();
		} catch (e) {
			onmessage({ kind: 'error', text: `${t('adminStatuses.updateFailedPre', $locale)}${e}` });
		} finally {
			busy = false;
		}
	}

	function openCreate() {
		newNameEn = '';
		newNameKo = '';
		newColor = pickNextColor();
		creating = true;
	}

	async function doCreate() {
		const en = newNameEn.trim();
		const ko = newNameKo.trim();
		// DEV-014 후속: name_ko 는 선택 (한국어 입력 없어도 추가 가능).
		if (!en) {
			onmessage({ kind: 'error', text: t('adminStatuses.nameEnRequired', $locale) });
			return;
		}
		busy = true;
		try {
			await adminApi.createStatus({
				name_en: en,
				name_ko: ko,
				color: newColor
			});
			onmessage({ kind: 'success', text: `${t('adminTypes.addedPre', $locale)}${en}${t('adminTypes.addedPost', $locale)}` });
			creating = false;
			await refresh();
		} catch (e) {
			onmessage({ kind: 'error', text: `${t('adminStatuses.addFailedPre', $locale)}${e}` });
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
			onmessage({ kind: 'success', text: `${t('adminTypes.deletedPre', $locale)}${target.slug}${t('adminTypes.deletedPost', $locale)}` });
			await refresh();
		} catch (e) {
			onmessage({ kind: 'error', text: `${t('adminStatuses.deleteFailedPre', $locale)}${e}` });
		} finally {
			busy = false;
		}
	}
</script>

<section>
	<div class="section-header">
		<h2>Quest Statuses</h2>
		<div class="actions">
			<button onclick={openCreate} disabled={busy}>{t('adminStatuses.newStatus', $locale)}</button>
			<button onclick={refresh} disabled={busy}>{t('admin.refresh', $locale)}</button>
		</div>
	</div>

	{#if statuses.length === 0}
		<p class="empty">{t('adminStatuses.empty', $locale)}</p>
	{:else}
		<table>
			<thead>
				<tr>
					<th style="width: 12ch">slug</th>
					<th>name_en</th>
					<th>name_ko</th>
					<th style="width: 5ch">{t('adminTypes.colColor', $locale)}</th>
					<!-- DEV-093: 캠페인 진행도용 "완료" 카운트 토글. -->
					<th style="width: 7ch" title={t('adminStatuses.doneColTitle', $locale)}>{t('adminStatuses.doneCol', $locale)}</th>
					<th style="width: 8ch">{t('adminTypes.colInUse', $locale)}</th>
					<th style="width: 14ch"></th>
				</tr>
			</thead>
			<tbody>
				{#each statuses as s (s.id)}
					<tr>
						{#if editing === s.slug}
							<!-- BUG-018: slug 도 inline 편집 가능. 변경 시 cascade confirm. -->
							<td>
								<input
									type="text"
									bind:value={editSlug}
									maxlength="32"
									pattern="[a-z0-9_]+"
									title={t('adminStatuses.slugTitle', $locale)}
									disabled={busy}
									class="slug-input"
								/>
							</td>
							<td>
								<input
									type="text"
									bind:value={editNameEn}
									maxlength="32"
									pattern="[A-Za-z][A-Za-z0-9 _\-]*"
									title={t('adminStatuses.nameEnTitle', $locale)}
									disabled={busy}
								/>
							</td>
							<td>
								<input type="text" bind:value={editNameKo} maxlength="32" disabled={busy} />
							</td>
							<td><input type="color" bind:value={editColor} disabled={busy} /></td>
							<td style="text-align: center;">
								<input
									type="checkbox"
									bind:checked={editCountsAsDone}
									disabled={busy}
									title={t('adminStatuses.countsAsDoneTitle', $locale)}
								/>
							</td>
							<td class="count">{s.quest_count}</td>
							<td class="row-actions">
								<button class="save" onclick={saveEdit} disabled={busy}>{t('common.save', $locale)}</button>
								<button onclick={cancelEdit} disabled={busy}>{t('common.cancel', $locale)}</button>
							</td>
						{:else}
							<td><code>{s.slug}</code></td>
							<td>{s.name_en}</td>
							<td>{s.name_ko || '—'}</td>
							<td>
								<span class="swatch" style="background: {s.color}"></span>
								<code class="hex">{s.color}</code>
							</td>
							<td style="text-align: center;">
								{#if s.counts_as_done}
									<span class="done-mark" title={t('adminStatuses.countsAsDoneMark', $locale)}>✓</span>
								{:else}
									<span class="dim">—</span>
								{/if}
							</td>
							<td class="count">{s.quest_count}</td>
							<td class="row-actions">
								<button onclick={() => startEdit(s)} disabled={busy}>{t('adminTypes.edit', $locale)}</button>
								<button
									class="danger"
									onclick={() => askDelete(s)}
									disabled={busy || s.quest_count > 0}
									title={s.quest_count > 0
										? `${t('adminTypes.inUsePre', $locale)}${s.quest_count}${t('adminTypes.inUsePost', $locale)}`
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
		<p class="hint">{t('adminStatuses.slugFrozenHint', $locale)}</p>
	{/if}
</section>

{#if creating}
	<div class="ov" role="presentation">
		<div class="modal" role="dialog" aria-modal="true" tabindex="-1">
			<h3 class="modal-title">{t('adminStatuses.newStatusTitle', $locale)}</h3>
			<div class="form">
				<label>
					<span>name_en</span>
					<input
						type="text"
						bind:value={newNameEn}
						placeholder={t('adminStatuses.nameEnPlaceholder', $locale)}
						maxlength="32"
						pattern="[A-Za-z][A-Za-z0-9 _\-]*"
						title={t('adminStatuses.nameEnTitle', $locale)}
						disabled={busy}
					/>
				</label>
				<label>
					<span>name_ko</span>
					<input
						type="text"
						bind:value={newNameKo}
						placeholder={t('adminStatuses.nameKoPlaceholder', $locale)}
						maxlength="32"
						disabled={busy}
					/>
				</label>
				<label>
					<span>{t('adminTypes.colColor', $locale)}</span>
					<input type="color" bind:value={newColor} disabled={busy} />
					<code class="hex">{newColor}</code>
				</label>
			</div>
			<p class="form-note">{t('adminStatuses.newStatusNote', $locale)}</p>
			<div class="modal-actions">
				<button class="btn-yes" onclick={doCreate} disabled={busy}>{t('common.add', $locale)}</button>
				<button class="btn-no" onclick={() => (creating = false)} disabled={busy}>{t('common.cancel', $locale)}</button>
			</div>
		</div>
	</div>
{/if}

{#if confirmDelete}
	<div class="ov" role="presentation">
		<div class="modal" role="dialog" aria-modal="true" tabindex="-1">
			<h3 class="modal-title">{t('adminStatuses.deleteStatusTitle', $locale)}</h3>
			<p class="modal-msg">
				<strong>{confirmDelete.name_en}</strong> (<code>{confirmDelete.slug}</code>){t('adminStatuses.deleteMsg1', $locale)}
				<br />
				{t('adminStatuses.deleteMsg2', $locale)}<code>.guild/statuses/</code>{t('adminStatuses.deleteMsg3', $locale)}
			</p>
			<div class="modal-actions">
				<button class="btn-yes danger" onclick={doDelete} disabled={busy}>{t('detail.delete', $locale)}</button>
				<button class="btn-no" onclick={() => (confirmDelete = null)} disabled={busy}>{t('common.cancel', $locale)}</button>
			</div>
		</div>
	</div>
{/if}

<!-- DEV-119: slug rename cascade 확인 — 인앱 모달. -->
<ConfirmDialog
	open={confirmRename !== null}
	title={t('adminStatuses.renameSlugTitle', $locale)}
	message={confirmRename
		? `'${confirmRename.oldSlug}' → '${confirmRename.newSlug}'${t('adminStatuses.renameSlugConfirm1', $locale)}` +
			`${confirmRename.count}${t('adminStatuses.renameSlugConfirm2', $locale)}`
		: ''}
	confirmLabel={t('common.change', $locale)}
	danger
	onconfirm={() => {
		const r = confirmRename;
		confirmRename = null;
		if (r) doSaveEdit(true, r.newSlug);
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
	/* DEV-074 fix6 / DEV-116 fix: 다른 save 버튼과 동일 패턴 — `--btn-primary-*`
	   토큰. 라이트모드에서도 명도 적정 (success 기반, fix5 의 #1a7f37). */
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
	input[type='text'].slug-input {
		width: 14ch;
		font-family: 'SFMono-Regular', Consolas, monospace;
		text-transform: lowercase;
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
