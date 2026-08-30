<script lang="ts">
	import { onMount } from 'svelte';
	import { adminApi } from '$lib/api/admin';
	import type { SkippedFile, JournalTail } from '$lib/api/admin';
	import type { DriftReport, SnapshotInfo } from '$lib/types';
	import { detectEnvironment } from '$lib/api/transport';
	// DEV-205 모듈5: Admin 페이지 i18n.
	import { locale, t } from '$lib/stores/locale';
	// DEV-259: 로컬 토스트 복제 제거 — 앱 공용 toast 로 위임.
	import { showToast } from '$lib/stores/toast';
	// DEV-014: Quest type / status 커스터마이즈 섹션.
	import AdminTypesSection from '$lib/components/admin/AdminTypesSection.svelte';
	import AdminStatusesSection from '$lib/components/admin/AdminStatusesSection.svelte';
	// DEV-068: `.guild/tags/{slug}.toml` 정의 (색 / 설명).
	import AdminTagDefsSection from '$lib/components/admin/AdminTagDefsSection.svelte';
	// DEV-119: window.confirm() 대신 인앱 모달 (Tauri 에서 native confirm silent return).
	import ConfirmDialog from '$lib/components/ConfirmDialog.svelte';

	let snapshots = $state<SnapshotInfo[]>([]);
	let drift = $state<DriftReport | null>(null);
	// 비정상 파일 (정의되지 않은 status / 파싱 실패) — reindex/sync 에서 조용히
	// skip 되므로 명시 경고. Tauri 에선 시동 시 list_problem_files 로, web 에선
	// reindex 결과의 skipped 로 채워진다.
	let problemFiles = $state<SkippedFile[]>([]);
	let busy = $state(false);
	// DEV-119: 복원 확인 — 인앱 모달. null = 닫힘, 객체 = 열림 ({ts} 는 undefined 면 최신).
	// reindex 는 사용자 지시로 sweep 제외 (idempotent, 파일 truth 불변).
	let confirmRestore = $state<{ ts: string | undefined } | null>(null);
	// DEV-162: 런타임 정비 — journal tail 뷰 (null = 아직 미조회).
	let journal = $state<JournalTail | null>(null);

	function onSectionMessage(m: { kind: 'info' | 'success' | 'error'; text: string } | null) {
		if (!m) return;
		if (m.kind === 'success') showSuccess(m.text);
		else if (m.kind === 'info') showInfo(m.text);
		else showError(m.text);
	}

	onMount(async () => {
		await refresh();
		await loadProblemFiles();
	});

	/** Tauri 전용: 비정상 파일 목록 조회 (web 에선 noop — HTTP route 없음). */
	async function loadProblemFiles() {
		if (detectEnvironment() !== 'tauri') return;
		try {
			const { invoke } = await import('@tauri-apps/api/core');
			problemFiles = await invoke<SkippedFile[]>('list_problem_files');
		} catch {
			/* 길드 모드 아님 / 조회 실패 — 무시 */
		}
	}

	async function refresh() {
		try {
			snapshots = await adminApi.listSnapshots();
		} catch (e) {
			showError(`${t('admin.listLoadFailedPre', $locale)}${e}`);
		}
	}

	function formatSize(bytes: number): string {
		if (bytes < 1024) return `${bytes} B`;
		if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
		return `${(bytes / (1024 * 1024)).toFixed(2)} MB`;
	}

	function formatTimestamp(ts: string): string {
		// BUG-086: 스냅샷 timestamp 는 UTC 정규형("20260516-103341")로 저장 →
		// 표시할 때만 로컬로 변환. 파싱 실패 시 원본 그대로.
		if (ts.length !== 15 || ts[8] !== '-') return ts;
		const [y, mo, d, h, mi, s] = [
			+ts.slice(0, 4),
			+ts.slice(4, 6),
			+ts.slice(6, 8),
			+ts.slice(9, 11),
			+ts.slice(11, 13),
			+ts.slice(13, 15)
		];
		const dt = new Date(Date.UTC(y, mo - 1, d, h, mi, s));
		if (Number.isNaN(dt.getTime())) return ts;
		const p = (n: number) => String(n).padStart(2, '0');
		return `${dt.getFullYear()}-${p(dt.getMonth() + 1)}-${p(dt.getDate())} ${p(dt.getHours())}:${p(dt.getMinutes())}:${p(dt.getSeconds())}`;
	}

	// DEV-259: 페이지 로컬 토스트 복제 구현 제거 — 앱 공용 showToast()/ToastHost
	// 로 위임(ToastHost 를 고쳐도 admin 에 반영 안 되던 이중 구현이 "알림
	// 통일했는데 재발"의 원인이었음). 함수명은 유지해 24곳 호출부 무변경.
	function showSuccess(text: string) {
		showToast(text, 'success');
	}
	function showInfo(text: string) {
		showToast(text, 'info');
	}
	function showError(text: string) {
		showToast(text, 'error', 6000);
	}

	async function onCreateSnapshot() {
		busy = true;
		try {
			const info = await adminApi.createSnapshot();
			showSuccess(`${t('admin.backupCreatedPre', $locale)}${formatTimestamp(info.timestamp)}`);
			await refresh();
		} catch (e) {
			showError(`${t('admin.backupCreateFailedPre', $locale)}${e}`);
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
			// BUG-076: restore 가 파일 복구 + reindex(캐시 재구축)까지 수행. 별도
			// reindex 안내(데이터 소실 위험) 제거. 파일/DB 변경 반영 위해 새로고침.
			showSuccess(
				`${t('admin.restoreDonePre', $locale)}${formatTimestamp(res.restored_to)}${t('admin.restoreDonePost', $locale)}`
			);
			setTimeout(() => window.location.reload(), 800);
		} catch (e) {
			showError(`${t('admin.restoreFailedPre', $locale)}${e}`);
		} finally {
			busy = false;
		}
	}

	// DEV-175: 백업 삭제 — 인앱 confirm 후 삭제 + 목록 갱신.
	let confirmDeleteSnap = $state<string | null>(null);
	async function doDeleteSnapshot(ts: string) {
		busy = true;
		try {
			await adminApi.deleteSnapshot(ts);
			snapshots = await adminApi.listSnapshots();
			showSuccess(`${t('admin.backupDeletedPre', $locale)}${formatTimestamp(ts)}`);
		} catch (e) {
			showError(`${t('admin.backupDeleteFailedPre', $locale)}${e}`);
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
				showSuccess(t('admin.driftOkMsg', $locale));
			} else {
				showInfo(`${total}${t('admin.driftFoundPost', $locale)}`);
			}
		} catch (e) {
			showError(`${t('admin.driftCheckFailedPre', $locale)}${e}`);
		} finally {
			busy = false;
		}
	}

	async function onReindex() {
		if (!confirm(t('admin.reindexConfirm', $locale))) return;
		busy = true;
		try {
			const result = await adminApi.reindex();
			drift = null;
			problemFiles = result.skipped;
			// reindex 후 항상 새로고침. 새로고침 시 +layout 의 시동 스캔
			// (list_problem_files → 토스트) + admin 의 loadProblemFiles(패널) 가 다시
			// 돌아 '무엇이 문제인지'가 자동으로 표시된다. (이전엔 문제 있을 때 reload
			// 안 해서 수동 Ctrl+R 전까지 상세가 안 보였음.) 데이터 stale 도 해소.
			if (result.skipped.length > 0) {
				showError(
					`${t('admin.reindexSkippedPre', $locale)}${result.skipped.length}${t('admin.reindexSkippedPost', $locale)}`
				);
			} else {
				showSuccess(t('admin.reindexDone', $locale));
			}
			// 토스트가 잠깐 보이도록 짧은 지연 후 full reload.
			setTimeout(() => window.location.reload(), 800);
		} catch (e) {
			showError(`${t('admin.reindexFailedPre', $locale)}${e}`);
		} finally {
			busy = false;
		}
	}

	// DEV-162: index.db VACUUM (dead row 공간 회수).
	async function onVacuum() {
		busy = true;
		try {
			const r = await adminApi.vacuum();
			const pct = r.before_bytes > 0 ? ((r.saved_bytes / r.before_bytes) * 100).toFixed(1) : '0';
			if (r.saved_bytes > 0) {
				showSuccess(
					`${t('admin.vacuumDonePre', $locale)}${r.saved_bytes.toLocaleString()}${t('admin.vacuumDoneMid', $locale)}${pct}${t('admin.vacuumDonePost', $locale)}`
				);
			} else {
				showInfo(t('admin.vacuumNoSpace', $locale));
			}
		} catch (e) {
			showError(`${t('admin.vacuumFailedPre', $locale)}${e}`);
		} finally {
			busy = false;
		}
	}

	// DEV-162: journal.db(AOF) 최근 op 조회.
	async function onJournalTail() {
		busy = true;
		try {
			journal = await adminApi.journalTail(50);
			if (journal.total === 0) {
				showInfo(t('admin.journalEmptyMsg', $locale));
			}
		} catch (e) {
			showError(`${t('admin.journalLoadFailedPre', $locale)}${e}`);
		} finally {
			busy = false;
		}
	}
</script>

<svelte:head>
	<title>Admin · openguild</title>
</svelte:head>

<!-- DEV-259: 페이지 로컬 toast 마크업 제거 — 전역 ToastHost 가 렌더. -->

<div class="page">
	<h1>{t('admin.title', $locale)}</h1>
	<p class="note">{t('admin.noAuthWarn', $locale)}</p>

	<AdminTypesSection onmessage={onSectionMessage} />
	<AdminStatusesSection onmessage={onSectionMessage} />
	<AdminTagDefsSection onmessage={onSectionMessage} />

	<section>
		<div class="section-header">
			<h2>{t('admin.backupsHeading', $locale)}</h2>
			<div class="actions">
				<button onclick={onCreateSnapshot} disabled={busy}>{t('admin.newBackup', $locale)}</button>
				<button onclick={refresh} disabled={busy}>{t('admin.refresh', $locale)}</button>
			</div>
		</div>

		<!-- BUG-188: 백업 범위 안내 — 첨부는 백업 대상이 아니다. 목록이 비어
		     있을 때도 보여야 하므로 {#if} 밖에 둔다(백업을 처음 만들기 전에
		     알아야 할 정보다). -->
		<p class="hint scope">{t('admin.backupScopeHint', $locale)}</p>

		{#if snapshots.length === 0}
			<p class="empty">{t('admin.noBackups', $locale)}</p>
		{:else}
			<table>
				<thead>
					<tr>
						<th>{t('admin.colTime', $locale)}</th>
						<th>{t('admin.colSize', $locale)}</th>
						<th></th>
					</tr>
				</thead>
				<tbody>
					{#each snapshots.slice().reverse() as s (s.timestamp)}
						<tr>
							<td><code>{formatTimestamp(s.timestamp)}</code></td>
							<td>{formatSize(s.size_bytes)}</td>
							<td class="snap-actions">
								<button class="restore" onclick={() => onRestore(s.timestamp)} disabled={busy}
									>{t('admin.restore', $locale)}</button
								>
								<button
									class="del-snap"
									title={t('admin.deleteThisBackup', $locale)}
									onclick={() => (confirmDeleteSnap = s.timestamp)}
									disabled={busy}>{t('detail.delete', $locale)}</button
								>
							</td>
						</tr>
					{/each}
				</tbody>
			</table>
			<p class="hint">{t('admin.autoBackupHint', $locale)}</p>
		{/if}
	</section>

	<section>
		<div class="section-header">
			<h2>{t('admin.driftHeading', $locale)}</h2>
			<div class="actions">
				<button onclick={onCheckDrift} disabled={busy}>{t('admin.check', $locale)}</button>
				<button onclick={onReindex} disabled={busy}>Reindex</button>
			</div>
		</div>

		{#if problemFiles.length > 0}
			<div class="problem-files" role="alert">
				<h3>
					{t('admin.problemFilesPre', $locale)}{problemFiles.length}{t(
						'admin.problemFilesPost',
						$locale
					)}
				</h3>
				<p class="hint">
					{t('admin.problemFilesHint', $locale)}
				</p>
				<ul>
					{#each problemFiles as p (p.path)}
						<li><code>{p.path}</code><span class="reason"> — {p.reason}</span></li>
					{/each}
				</ul>
			</div>
		{/if}

		{#if drift === null}
			<p class="empty">{t('admin.driftEmpty', $locale)}</p>
		{:else}
			{@const total =
				drift.fresh_files.length + drift.missing_in_index.length + drift.stale_in_index.length}
			{#if total === 0}
				<p class="ok">{t('admin.driftOk', $locale)}</p>
			{:else}
				<div class="drift-report">
					{#if drift.missing_in_index.length > 0}
						<div>
							<h3>{t('admin.missingInIndex', $locale)}{drift.missing_in_index.length})</h3>
							<ul>
								{#each drift.missing_in_index as slug}<li><code>{slug}</code></li>{/each}
							</ul>
						</div>
					{/if}
					{#if drift.stale_in_index.length > 0}
						<div>
							<h3>{t('admin.staleInIndex', $locale)}{drift.stale_in_index.length})</h3>
							<ul>
								{#each drift.stale_in_index as slug}<li><code>{slug}</code></li>{/each}
							</ul>
						</div>
					{/if}
					{#if drift.fresh_files.length > 0}
						<div>
							<h3>{t('admin.freshFiles', $locale)}{drift.fresh_files.length})</h3>
							<ul>
								{#each drift.fresh_files as slug}<li><code>{slug}</code></li>{/each}
							</ul>
						</div>
					{/if}
					<p class="hint">{t('admin.reindexHint', $locale)}</p>
				</div>
			{/if}
		{/if}
	</section>

	<!-- DEV-162: 런타임 정비 — VACUUM + journal(AOF) tail. -->
	<section>
		<div class="section-header">
			<h2>{t('admin.maintHeading', $locale)}</h2>
			<div class="actions">
				<button onclick={onVacuum} disabled={busy}>{t('admin.vacuum', $locale)}</button>
				<button onclick={onJournalTail} disabled={busy}>{t('admin.recentOps', $locale)}</button>
			</div>
		</div>

		{#if journal === null}
			<p class="empty">
				{t('admin.maintEmptyHint', $locale)}
			</p>
		{:else if journal.total === 0}
			<p class="ok">{t('admin.journalEmptyMsg', $locale)}</p>
		{:else}
			<p class="hint">
				{t('admin.journalTotalPre', $locale)}{journal.total}{t(
					'admin.journalTotalMid',
					$locale
				)}{journal.rows.length}{t('admin.journalTotalPost', $locale)}
			</p>
			<ul class="journal">
				{#each journal.rows as op (op.id)}
					<li>
						<code class="jop">#{op.id} {op.op}</code>
						<span class="jts">{op.ts}</span>
						<div class="jargs"><code>{op.args}</code></div>
					</li>
				{/each}
			</ul>
		{/if}
	</section>
</div>

<!-- DEV-119: backup 복원 확인 — 인앱 모달. -->
<ConfirmDialog
	open={confirmRestore !== null}
	title={t('admin.restoreTitle', $locale)}
	message={confirmRestore
		? `${t('admin.restoreConfirmPre', $locale)}${confirmRestore.ts ? formatTimestamp(confirmRestore.ts) : t('admin.latest', $locale)}${t('admin.restoreConfirmMid', $locale)}` +
			t('admin.restoreConfirmPost', $locale)
		: ''}
	confirmLabel={t('admin.restore', $locale)}
	danger
	onconfirm={() => {
		const r = confirmRestore;
		confirmRestore = null;
		if (r) doRestore(r.ts);
	}}
	oncancel={() => (confirmRestore = null)}
/>

<!-- DEV-175: 백업 삭제 확인 — 인앱 모달. -->
<ConfirmDialog
	open={confirmDeleteSnap !== null}
	title={t('admin.deleteTitle', $locale)}
	message={confirmDeleteSnap
		? `${t('admin.deleteConfirmPre', $locale)}${formatTimestamp(confirmDeleteSnap)}${t('admin.deleteConfirmPost', $locale)}`
		: ''}
	confirmLabel={t('detail.delete', $locale)}
	danger
	onconfirm={() => {
		const ts = confirmDeleteSnap;
		confirmDeleteSnap = null;
		if (ts) doDeleteSnapshot(ts);
	}}
	oncancel={() => (confirmDeleteSnap = null)}
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
		/* 다른 admin 섹션(Statuses/Types/TagDefs 컴포넌트)과 동일 토큰.
		   이전 --nav-bg/--nav-border 는 다크에서 보라빛이라 섹션마다 색이 달랐음. */
		background: var(--bg-elevated);
		border: var(--bw) solid var(--border);
		border-radius: var(--r-lg);
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
	.actions {
		display: flex;
		flex-wrap: wrap;
		gap: 0.5rem;
	}
	button {
		padding: 0.4rem 0.9rem;
		background: var(--bg-subtle);
		border: var(--bw) solid var(--border);
		border-radius: var(--r-md);
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
	/* DEV-175: 백업 삭제 버튼. */
	/* BUG-143: td 에 display:flex 금지(table-cell 렌더 깨짐 — 행 구분선
	   어긋남). 일반 셀 + 버튼 간격은 margin. */
	.snap-actions {
		white-space: nowrap;
	}
	.snap-actions button + button {
		margin-left: 0.4rem;
	}
	/* BUG-196 후속(admin 재보고 "백업 섹션이 아직 삐져나옴"): 영어에서 Restore +
	   Delete 를 nowrap 으로 한 줄에 묶으면 그 칸만 180px 를 요구하고, table 은
	   width:100% 라도 내용 최소폭 아래로 못 줄어들어 **표가 섹션 밖으로** 나갔다
	   (그때 tr 아래 테두리가 같이 나가 '구분선이 삐져나온' 것처럼 보인다).
	   좁은 화면에선 버튼이 쌓이도록 풀고 셀 여백도 줄인다. */
	button.del-snap {
		color: var(--danger);
		border-color: color-mix(in srgb, var(--danger) 45%, transparent);
	}
	button.del-snap:hover:not(:disabled) {
		background: color-mix(in srgb, var(--danger) 14%, transparent);
	}
	table {
		width: 100%;
		border-collapse: collapse;
	}
	th,
	td {
		padding: 0.5rem 0.75rem;
		text-align: left;
		border-bottom: var(--bw) solid var(--border);
	}
	th {
		color: var(--text-muted);
		font-weight: 500;
		font-size: 0.85rem;
	}
	td code {
		font-family: var(--font-mono);
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
	/* BUG-188: 백업 범위 안내는 목록 위에 오므로 아래 여백으로 목록과 띄운다. */
	.hint.scope {
		margin: 0 0 0.75rem;
		line-height: 1.5;
	}
	.drift-report ul {
		margin: 0.25rem 0 0.75rem 1.25rem;
		padding: 0;
	}
	.drift-report li {
		font-family: var(--font-mono);
		font-size: 0.85rem;
		color: var(--text);
		list-style: disc;
	}
	/* DEV-162: journal(AOF) tail 뷰. */
	.journal {
		margin: 0.5rem 0 0;
		padding: 0;
		list-style: none;
		display: flex;
		flex-direction: column;
		gap: 0.5rem;
	}
	.journal li {
		padding: 0.4rem 0.6rem;
		border: var(--bw) solid var(--border);
		border-radius: var(--r-md);
		background: var(--bg-subtle);
	}
	.jop {
		font-family: var(--font-mono);
		font-size: 0.82rem;
		color: var(--text-strong);
	}
	.jts {
		margin-left: 0.5rem;
		font-size: 0.75rem;
		color: var(--text-muted);
	}
	.jargs {
		margin-top: 0.25rem;
		font-family: var(--font-mono);
		font-size: 0.78rem;
		color: var(--text-muted);
		word-break: break-all;
	}
	.problem-files {
		margin-bottom: 1rem;
		padding: 0.75rem 1rem;
		border-radius: var(--r-lg);
		background: color-mix(in srgb, var(--danger) 12%, transparent);
		border: var(--bw) solid color-mix(in srgb, var(--danger) 40%, transparent);
	}
	.problem-files h3 {
		margin: 0 0 0.25rem 0;
		font-size: 0.95rem;
		color: var(--danger);
	}
	.problem-files ul {
		margin: 0.5rem 0 0 1.25rem;
		padding: 0;
	}
	.problem-files li {
		font-size: 0.85rem;
		color: var(--text);
		list-style: disc;
		margin-bottom: 0.15rem;
	}
	.problem-files code {
		font-family: var(--font-mono);
	}
	.problem-files .reason {
		color: var(--text-muted);
	}
	/* DEV-259: 로컬 toast CSS 제거 — 전역 ToastHost 스타일 단일화. */

	/* 위 주석의 수정 본체 — BUG-200 후속 감사에서 **순서 때문에 죽어 있던** 걸
	   발견해 스타일 끝으로 옮겼다(th/td 여백 축소가 아래 기본 규칙에 밀렸다). */
	@media (max-width: 640px) {
		.snap-actions {
			white-space: normal;
			display: flex;
			flex-wrap: wrap;
			gap: 0.35rem;
		}
		.snap-actions button + button {
			margin-left: 0;
		}
		th,
		td {
			padding: 0.45rem 0.4rem;
		}
	}
</style>
