<script lang="ts">
	import { onMount } from 'svelte';
	import { adminApi } from '$lib/api/admin';
	import type { DriftReport, SnapshotInfo } from '$lib/types';
	// DEV-014: Quest type / status 커스터마이즈 섹션.
	import AdminTypesSection from '$lib/components/admin/AdminTypesSection.svelte';
	import AdminStatusesSection from '$lib/components/admin/AdminStatusesSection.svelte';
	// DEV-068: `.guild/tags/{slug}.toml` 정의 (색 / 설명).
	import AdminTagDefsSection from '$lib/components/admin/AdminTagDefsSection.svelte';
	// DEV-119: window.confirm() 대신 인앱 모달 (Tauri 에서 native confirm silent return).
	import ConfirmDialog from '$lib/components/ConfirmDialog.svelte';

	let snapshots = $state<SnapshotInfo[]>([]);
	let drift = $state<DriftReport | null>(null);
	let busy = $state(false);
	let message = $state<{ kind: 'info' | 'success' | 'error'; text: string } | null>(null);
	// DEV-119: 복원 확인 — 인앱 모달. null = 닫힘, 객체 = 열림 ({ts} 는 undefined 면 최신).
	// reindex 는 사용자 지시로 sweep 제외 (idempotent, 파일 truth 불변).
	let confirmRestore = $state<{ ts: string | undefined } | null>(null);

	function onSectionMessage(
		m: { kind: 'info' | 'success' | 'error'; text: string } | null
	) {
		if (!m) return;
		if (m.kind === 'success') showSuccess(m.text);
		else if (m.kind === 'info') showInfo(m.text);
		else showError(m.text);
	}

	onMount(async () => {
		await refresh();
	});

	async function refresh() {
		try {
			snapshots = await adminApi.listSnapshots();
		} catch (e) {
			showError(`목록 조회 실패: ${e}`);
		}
	}

	function formatSize(bytes: number): string {
		if (bytes < 1024) return `${bytes} B`;
		if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
		return `${(bytes / (1024 * 1024)).toFixed(2)} MB`;
	}

	function formatTimestamp(ts: string): string {
		// "20260516-103341" → "2026-05-16 10:33:41"
		if (ts.length !== 15 || ts[8] !== '-') return ts;
		return `${ts.slice(0, 4)}-${ts.slice(4, 6)}-${ts.slice(6, 8)} ${ts.slice(9, 11)}:${ts.slice(11, 13)}:${ts.slice(13, 15)}`;
	}

	function showSuccess(text: string) {
		message = { kind: 'success', text };
		setTimeout(() => (message = null), 4000);
	}
	function showInfo(text: string) {
		message = { kind: 'info', text };
		setTimeout(() => (message = null), 4000);
	}
	function showError(text: string) {
		message = { kind: 'error', text };
		setTimeout(() => (message = null), 6000);
	}

	async function onCreateSnapshot() {
		busy = true;
		try {
			const info = await adminApi.createSnapshot();
			showSuccess(`백업 생성: ${formatTimestamp(info.timestamp)}`);
			await refresh();
		} catch (e) {
			showError(`백업 생성 실패: ${e}`);
		} finally {
			busy = false;
		}
	}

	function onRestore(ts?: string) {
		// DEV-119: native confirm 대신 인앱 모달 — onconfirm 에서 doRestore.
		confirmRestore = { ts };
	}

	async function doRestore(ts: string | undefined) {
		busy = true;
		try {
			const res = await adminApi.restore(ts);
			showSuccess(`복원 완료: ${formatTimestamp(res.restored_to)}. 파일 동기화를 위해 'reindex' 가 필요할 수 있습니다.`);
		} catch (e) {
			showError(`복원 실패: ${e}`);
		} finally {
			busy = false;
		}
	}

	async function onCheckDrift() {
		busy = true;
		try {
			drift = await adminApi.checkDrift();
			const total =
				drift.fresh_files.length + drift.missing_in_index.length + drift.stale_in_index.length;
			if (total === 0) {
				showSuccess('drift 없음 — 파일과 캐시 일치');
			} else {
				showInfo(`${total} 항목 drift 발견 — 아래 보고서 확인`);
			}
		} catch (e) {
			showError(`drift 검사 실패: ${e}`);
		} finally {
			busy = false;
		}
	}

	async function onReindex() {
		if (!confirm('파일들로부터 index.db 를 재구축합니다. 계속할까요?')) return;
		busy = true;
		try {
			await adminApi.reindex();
			showSuccess('reindex 완료');
			drift = null;
		} catch (e) {
			showError(`reindex 실패: ${e}`);
		} finally {
			busy = false;
		}
	}
</script>

<svelte:head>
	<title>Admin · openguild</title>
</svelte:head>

<!-- DEV-014 후속 (fix5): toast 메시지 — 모달이 떠 있어도 위에 표시되도록
     page 컨테이너 밖 + fixed positioning + 모달보다 높은 z-index. -->
{#if message}
	<div class="toast-wrap" role="status" aria-live="polite">
		<div class="message {message.kind}">{message.text}</div>
	</div>
{/if}

<div class="page">
	<h1>관리자 (Admin)</h1>
	<p class="note">
		⚠ 인증 없음 — MVP 단계. 멀티유저로 확장 시 보호 필요.
	</p>

	<AdminTypesSection onmessage={onSectionMessage} />
	<AdminStatusesSection onmessage={onSectionMessage} />
	<AdminTagDefsSection onmessage={onSectionMessage} />

	<section>
		<div class="section-header">
			<h2>백업 (Snapshots)</h2>
			<div class="actions">
				<button onclick={onCreateSnapshot} disabled={busy}>+ 새 백업</button>
				<button onclick={refresh} disabled={busy}>새로고침</button>
			</div>
		</div>

		{#if snapshots.length === 0}
			<p class="empty">백업 없음. "+ 새 백업" 을 눌러 첫 백업을 생성하세요.</p>
		{:else}
			<table>
				<thead>
					<tr>
						<th>시간</th>
						<th>크기</th>
						<th></th>
					</tr>
				</thead>
				<tbody>
					{#each snapshots.slice().reverse() as s (s.timestamp)}
						<tr>
							<td><code>{formatTimestamp(s.timestamp)}</code></td>
							<td>{formatSize(s.size_bytes)}</td>
							<td>
								<button class="restore" onclick={() => onRestore(s.timestamp)} disabled={busy}
									>복원</button
								>
							</td>
						</tr>
					{/each}
				</tbody>
			</table>
			<p class="hint">
				자동 백업: 매 mutation 후 정책 검사 (ops 50 회 OR 24 시간 도달 시).
			</p>
		{/if}
	</section>

	<section>
		<div class="section-header">
			<h2>Drift 검사</h2>
			<div class="actions">
				<button onclick={onCheckDrift} disabled={busy}>검사</button>
				<button onclick={onReindex} disabled={busy}>Reindex</button>
			</div>
		</div>

		{#if drift === null}
			<p class="empty">파일 vs index.db 일치성 검사. 외부 편집 / git pull 후 활용.</p>
		{:else}
			{@const total =
				drift.fresh_files.length + drift.missing_in_index.length + drift.stale_in_index.length}
			{#if total === 0}
				<p class="ok">✓ drift 없음</p>
			{:else}
				<div class="drift-report">
					{#if drift.missing_in_index.length > 0}
						<div>
							<h3>파일은 있는데 index 에 없음 ({drift.missing_in_index.length})</h3>
							<ul>
								{#each drift.missing_in_index as slug}<li><code>{slug}</code></li>{/each}
							</ul>
						</div>
					{/if}
					{#if drift.stale_in_index.length > 0}
						<div>
							<h3>index 에 있는데 파일이 없음 ({drift.stale_in_index.length})</h3>
							<ul>
								{#each drift.stale_in_index as slug}<li><code>{slug}</code></li>{/each}
							</ul>
						</div>
					{/if}
					{#if drift.fresh_files.length > 0}
						<div>
							<h3>파일이 index 보다 새것 ({drift.fresh_files.length})</h3>
							<ul>
								{#each drift.fresh_files as slug}<li><code>{slug}</code></li>{/each}
							</ul>
						</div>
					{/if}
					<p class="hint">Reindex 버튼으로 캐시를 파일 기준으로 재구축할 수 있습니다.</p>
				</div>
			{/if}
		{/if}
	</section>
</div>

<!-- DEV-119: backup 복원 확인 — 인앱 모달. -->
<ConfirmDialog
	open={confirmRestore !== null}
	title="백업 복원"
	message={confirmRestore
		? `정말 "${confirmRestore.ts ? formatTimestamp(confirmRestore.ts) : '최신'}" 백업으로 복원하시겠습니까?\n\n` +
			`현재 상태가 덮어써집니다 (직전 .pre-restore.db 로 자동 백업됨).`
		: ''}
	confirmLabel="복원"
	danger
	onconfirm={() => {
		const r = confirmRestore;
		confirmRestore = null;
		if (r) doRestore(r.ts);
	}}
	oncancel={() => (confirmRestore = null)}
/>

<style>
	.page {
		max-width: var(--content-max-width, 900px);
		margin: 0 auto;
		padding: 2rem 1.5rem;
		color: var(--text);
	}
	h1 {
		margin: 0 0 0.5rem 0;
		font-size: 1.4rem;
	}
	h2 {
		margin: 0;
		font-size: 1.1rem;
	}
	h3 {
		margin: 0.75rem 0 0.25rem;
		font-size: 0.95rem;
		color: var(--text-muted);
	}
	.note {
		color: var(--text-muted);
		font-size: 0.85rem;
		margin-bottom: 1.5rem;
	}
	section {
		margin-bottom: 2.5rem;
		padding: 1.25rem;
		background: var(--nav-bg);
		border: 1px solid var(--nav-border);
		border-radius: 8px;
	}
	.section-header {
		display: flex;
		align-items: center;
		justify-content: space-between;
		margin-bottom: 1rem;
	}
	.actions {
		display: flex;
		gap: 0.5rem;
	}
	button {
		padding: 0.4rem 0.9rem;
		background: var(--nav-border);
		border: 1px solid var(--border);
		border-radius: 6px;
		color: var(--text);
		font-size: 0.85rem;
		cursor: pointer;
		transition: background 0.15s;
	}
	button:hover:not(:disabled) {
		background: var(--border);
	}
	button:disabled {
		opacity: 0.5;
		cursor: not-allowed;
	}
	/* DEV-074 fix6: --btn-warning-* 토큰으로 통일. */
	button.restore {
		background: var(--btn-warning-bg);
		border-color: var(--btn-warning-border);
		color: var(--btn-warning-text);
	}
	button.restore:hover:not(:disabled) {
		background: var(--btn-warning-bg-hover);
	}
	table {
		width: 100%;
		border-collapse: collapse;
	}
	th,
	td {
		padding: 0.5rem 0.75rem;
		text-align: left;
		border-bottom: 1px solid var(--nav-border);
	}
	th {
		color: var(--text-muted);
		font-weight: 500;
		font-size: 0.85rem;
	}
	td code {
		font-family: 'Cascadia Code', 'Courier New', monospace;
		font-size: 0.85rem;
	}
	.empty {
		color: var(--text-muted);
		font-size: 0.875rem;
		margin: 0;
	}
	.ok {
		color: var(--success-strong);
		font-weight: 500;
	}
	.hint {
		color: var(--text-muted);
		font-size: 0.825rem;
		margin-top: 0.75rem;
	}
	.drift-report ul {
		margin: 0.25rem 0 0.75rem 1.25rem;
		padding: 0;
	}
	.drift-report li {
		font-family: 'Cascadia Code', 'Courier New', monospace;
		font-size: 0.85rem;
		color: var(--text);
		list-style: disc;
	}
	/* DEV-014 후속 (fix5): toast wrapper — 화면 우상단 고정.
	   모달 (.ov z-index 100) 보다 높은 z-index 로 가려지지 않게. */
	.toast-wrap {
		position: fixed;
		top: 1rem;
		right: 1rem;
		z-index: 1000;
		max-width: 420px;
		pointer-events: none;
	}
	.message {
		padding: 0.75rem 1rem;
		border-radius: 6px;
		font-size: 0.875rem;
		color: var(--text-strong);
		box-shadow: 0 8px 24px rgba(0, 0, 0, 0.5);
		pointer-events: auto;
		animation: toast-in 0.18s ease-out;
	}
	@keyframes toast-in {
		from { opacity: 0; transform: translateY(-8px); }
		to   { opacity: 1; transform: translateY(0); }
	}
	.message.info {
		background: color-mix(in srgb, var(--accent) 18%, transparent);
		border: 1px solid color-mix(in srgb, var(--accent) 45%, transparent);
	}
	.message.success {
		background: color-mix(in srgb, var(--success) 18%, transparent);
		border: 1px solid color-mix(in srgb, var(--success) 45%, transparent);
	}
	.message.error {
		background: color-mix(in srgb, var(--danger) 18%, transparent);
		border: 1px solid color-mix(in srgb, var(--danger) 45%, transparent);
	}
</style>
