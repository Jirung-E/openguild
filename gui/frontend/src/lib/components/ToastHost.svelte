<!--
  DEV-259: 앱 알림 통합 호스트 (layout 에 1회 마운트).
  `notifications` 스토어(도착순)를 우하단 한 컬럼으로 렌더 — 새 알림이 맨
  아래(코너)에, 기존은 위로 밀림. 종류(kind)별로 렌더:
   - toast : 텍스트(자동 소멸). showToast() 로 추가.
   - update: 업데이터 상태 카드.
   - schema: DB schema-ahead 경고 카드.
  업데이트·스키마 알림의 소스 watcher(주기 체크 / schema 조회 → 스토어
  upsert)도 이 호스트가 내장한다(이전의 UpdateBanner/SchemaAheadBanner
  껍데기 컴포넌트를 흡수·제거).
-->
<script lang="ts">
	import { onMount } from 'svelte';
	import { get } from 'svelte/store';
	import {
		notifications,
		upsertNotif,
		dismissNotif,
		dismissSchema,
		schemaSig,
		isSchemaDismissed,
		computeVisible,
		MAX_VISIBLE_NOTIFS,
		showToast
	} from '$lib/stores/toast';
	import {
		updateState,
		checkForUpdate,
		downloadAndRelaunch,
		dismissUpdate
	} from '$lib/api/updater';
	import { detectEnvironment } from '$lib/api/transport';
	import OverlayScrollbar from './OverlayScrollbar.svelte';
	import { locale, t } from '$lib/stores/locale';

	const RELEASE_URL = 'https://github.com/Jirung-E/openguild/releases/latest';
	const isTauri = detectEnvironment() === 'tauri';

	let notesEl: HTMLPreElement | undefined = $state(undefined);

	// 스키마 경고의 "최신 버전 받기" — 시스템 브라우저로. (button 이라 layout 의
	// anchor intercept 를 안 타서 명시 호출.)
	async function openRelease() {
		try {
			const { openUrl } = await import('@tauri-apps/plugin-opener');
			await openUrl(RELEASE_URL);
		} catch {
			try {
				window.open(RELEASE_URL, '_blank');
			} catch {
				/* ignore */
			}
		}
	}

	// ── DEV-259: 알림 소스 watcher 를 이 호스트로 흡수 ──────────────────
	// 이전엔 UpdateBanner/SchemaAheadBanner 가 아무것도 렌더 안 하면서 로직만
	// 돌리는 껍데기 컴포넌트였음 → 여기로 합쳐 두 컴포넌트를 제거.

	// DEV-083: 주기적 업데이트 재확인 간격(6시간 타협점).
	const PERIODIC_CHECK_MS = 6 * 60 * 60 * 1000;
	// BUG-096: uptodate/error 는 정보성이라 일정 시간 후 자동으로 idle 로.
	const AUTO_DISMISS_MS = 5000;

	// BUG-170: 디버그 훅 opt-in — dev 서버는 항상, 릴리스 빌드는 사용자가
	// `__ogEnableDebug()` 로 켠 경우에만. (localStorage 플래그)
	const DEBUG_HOOKS_KEY = 'openguild.debugHooks';
	function debugHooksEnabled(): boolean {
		if (import.meta.env.DEV) return true;
		try {
			return localStorage.getItem(DEBUG_HOOKS_KEY) === '1';
		} catch {
			return false;
		}
	}

	// DEV-063/DEV-259: 업데이터 상태 → 'update' 알림 presence(제자리 갱신).
	// DEV-266: dev 모드는 브라우저에서도 허용 — __ogNotify.update() 트리거용.
	// BUG-170: 디버그 훅을 켠 릴리스 빌드에서도 동일하게 허용(트리거 검증).
	$effect(() => {
		if (!isTauri && !debugHooksEnabled()) return;
		if ($updateState.status === 'idle') {
			dismissNotif('update');
		} else {
			upsertNotif({ id: 'update', kind: 'update' });
		}
	});
	// BUG-096: uptodate/error 자동 닫힘(→ idle → 위 effect 가 알림 제거).
	$effect(() => {
		const s = $updateState.status;
		if (s !== 'uptodate' && s !== 'error') return;
		const handle = setTimeout(() => dismissUpdate(), AUTO_DISMISS_MS);
		return () => clearTimeout(handle);
	});

	// BUG-041/BUG-139: DB schema 가 binary 보다 앞서면 'schema' 알림.
	async function checkSchema() {
		if (!isTauri) return;
		try {
			const { invoke } = await import('@tauri-apps/api/core');
			const status = (await invoke('get_db_schema_status')) as {
				is_ahead: boolean;
				ahead_versions: number[];
				binary_version: string;
				latest_known_migration: number | null;
			};
			if (!status.is_ahead) {
				dismissNotif('schema');
				return;
			}
			const ahead = status.ahead_versions ?? [];
			const binaryVersion = status.binary_version ?? '';
			if (isSchemaDismissed(schemaSig(binaryVersion, ahead))) return;
			upsertNotif({
				id: 'schema',
				kind: 'schema',
				binaryVersion,
				aheadVersions: ahead,
				latestKnown: status.latest_known_migration
			});
		} catch {
			// 명령 미존재(구 backend) — 조용히 무시.
		}
	}

	// ── DEV-266: 표시 상한 + "+N개 더" 축약 ─────────────────────────────
	// 동시 다발 알림이 스택을 무한정 키우지 않게 computeVisible 로 접는다.
	// 칩 클릭 시 임시 전체 펼침 — 개수가 상한 아래로 내려가면 자동 복귀.
	let expanded = $state(false);
	const view = $derived(computeVisible($notifications, expanded));
	$effect(() => {
		if ($notifications.length <= MAX_VISIBLE_NOTIFS) expanded = false;
	});

	onMount(() => {
		checkSchema();
		// DEV-266: dev 전용 알림 트리거 — schema-ahead 는 '구버전 binary + 신버전
		// DB', 업데이트 카드는 실제 릴리즈가 있어야만 떠서 동시 발생 조합을
		// 실기로 재현하기 어렵다. 개발 모드에서만 콘솔 헬퍼를 노출해 임의
		// 조합을 수동 재현할 수 있게 한다. 예:
		//   __ogNotify.toast('hello', 'error'); __ogNotify.schema(); __ogNotify.update('available');
		// BUG-170: 기존엔 `import.meta.env.DEV` 로만 게이트해서 실제 릴리스
		// 빌드(just build → exe)에는 훅이 아예 없었고, DEV-266 의 "테스트 방법"
		// 대로 콘솔에서 호출하면 `__ogNotify is not defined` 였다(사용자 보고).
		// 알림 스택은 실기 빌드에서 확인해야 의미가 있으므로, 릴리스 빌드에서도
		// **사용자가 명시적으로 켰을 때만** 노출한다:
		//   __ogEnableDebug()  →  reload  →  __ogNotify.* 사용 가능
		// 기본값은 여전히 꺼짐이라 일반 사용자 콘솔은 깨끗하다.
		if (typeof window !== 'undefined') {
			(window as unknown as Record<string, unknown>).__ogEnableDebug = (on = true) => {
				try {
					if (on) localStorage.setItem(DEBUG_HOOKS_KEY, '1');
					else localStorage.removeItem(DEBUG_HOOKS_KEY);
				} catch {
					/* private 모드 등 — 무시 */
				}
				location.reload();
			};
		}
		if (debugHooksEnabled() && typeof window !== 'undefined') {
			(window as unknown as Record<string, unknown>).__ogNotify = {
				toast: showToast,
				schema: () =>
					upsertNotif({
						id: 'schema',
						kind: 'schema',
						binaryVersion: '0.0.0-dev',
						aheadVersions: [99],
						latestKnown: 1
					}),
				update: (status = 'available') =>
					updateState.set(
						status === 'available'
							? { status: 'available', version: '9.9.9-dev', notes: 'dev trigger' }
							: ({ status } as never)
					)
			};
		}
		if (!isTauri) return; // 브라우저 모드 — 업데이트 개념 없음.
		checkForUpdate({ silent: true });
		const handle = setInterval(() => {
			const s = get(updateState).status;
			if (s === 'available' || s === 'downloading' || s === 'ready') return;
			checkForUpdate({ silent: true });
		}, PERIODIC_CHECK_MS);
		return () => clearInterval(handle);
	});
</script>

<div class="notif-stack" role="status" aria-live="polite">
	<!-- DEV-266: 상한 초과분 축약 칩 — 클릭 시 전체 펼침. 스택 맨 위(코너
	     반대쪽)에 둬 최신/지속 알림을 가리지 않는다. -->
	{#if view.hidden > 0}
		<button class="card more-chip" onclick={() => (expanded = true)}>
			{t('notif.more', $locale).replace('{n}', String(view.hidden))}
		</button>
	{/if}
	{#each view.visible as n (n.id)}
		{#if n.kind === 'toast'}
			<button
				class="card toast {n.variant}"
				onclick={() => dismissNotif(n.id)}
				title={t('update.close', $locale)}
			>
				{n.message}
				{#if n.count > 1}
					<!-- DEV-266: 중복 억제 — 같은 알림 재발생 횟수 뱃지. -->
					<span class="count">×{n.count}</span>
				{/if}
			</button>
		{:else if n.kind === 'update'}
			<div class="card upd" class:err={$updateState.status === 'error'} role="status">
				<button class="card-x" title={t('update.close', $locale)} onclick={() => dismissUpdate()}>×</button>
				{#if $updateState.status === 'checking'}
					<p class="t-title">{t('update.checking', $locale)}</p>
				{:else if $updateState.status === 'uptodate'}
					<p class="t-title ok">{t('update.uptodate', $locale)}</p>
				{:else if $updateState.status === 'available'}
					<p class="t-title">{t('update.availablePre', $locale)}<strong>{$updateState.version}</strong>{t('update.availablePost', $locale)}</p>
					{#if $updateState.notes}
						<details>
							<summary>{t('update.releaseNotes', $locale)}</summary>
							<pre bind:this={notesEl}>{$updateState.notes}</pre>
							{#if notesEl}
								<OverlayScrollbar target={notesEl} />
							{/if}
						</details>
					{/if}
					<button class="btn-primary" onclick={() => downloadAndRelaunch()}>
						{t('update.installBtn', $locale)}
					</button>
				{:else if $updateState.status === 'downloading'}
					<p class="t-title">
						{t('update.downloading', $locale)} {$updateState.pct !== null ? `${$updateState.pct}%` : ''}
					</p>
				{:else if $updateState.status === 'ready'}
					<p class="t-title ok">{t('update.ready', $locale)}</p>
				{:else if $updateState.status === 'error'}
					<p class="t-title err">{t('update.error', $locale)}</p>
					<p class="t-msg">{$updateState.message}</p>
				{/if}
			</div>
		{:else if n.kind === 'schema'}
			<div class="card schema" role="alert">
				<button
					class="card-x"
					onclick={() => dismissSchema(schemaSig(n.binaryVersion, n.aheadVersions))}
					aria-label={t('update.close', $locale)}
					title={t('update.close', $locale)}>×</button
				>
				<div class="msg">
					<strong>⚠ 이 길드 DB 가 현재 GUI 버전보다 새롭습니다</strong>
					<span class="detail">
						현재 GUI: <code>v{n.binaryVersion}</code>
						{#if n.latestKnown != null}
							(migration {n.latestKnown} 까지 인식)
						{/if}
						· 이 길드 DB: migration <strong>{Math.max(...n.aheadVersions)}</strong>
						{#if n.aheadVersions.length > 1}
							(+{n.aheadVersions.length - 1} more)
						{/if}
						적용됨. 일부 데이터가 제대로 표시되지 않거나 reindex 가 실패할 수 있습니다.
					</span>
				</div>
				<button class="btn-update" onclick={openRelease}> 최신 버전 받기 ↗ </button>
			</div>
		{/if}
	{/each}
</div>

<style>
	/* DEV-259: 우하단 단일 스택. bottom 고정 + column → 새 항목이 코너에,
	   기존은 위로 밀림. */
	.notif-stack {
		position: fixed;
		right: 1.5rem;
		bottom: 1.5rem;
		z-index: 2000;
		display: flex;
		flex-direction: column;
		gap: 0.5rem;
		align-items: stretch;
		width: calc(26.25rem * var(--popup-scale, 1)); /* BUG-064 */
		max-width: calc(100vw - 3rem);
		pointer-events: none;
	}
	.card {
		position: relative;
		text-align: left;
		border-radius: 8px;
		border: 1px solid var(--border);
		background: var(--bg-elevated);
		box-shadow: 0 6px 20px rgba(0, 0, 0, 0.5);
		color: var(--text);
		font-size: 0.85rem;
		pointer-events: auto;
		animation: notif-in 0.18s ease-out;
	}
	@keyframes notif-in {
		from {
			opacity: 0;
			transform: translateY(8px);
		}
		to {
			opacity: 1;
			transform: translateY(0);
		}
	}
	.card-x {
		position: absolute;
		top: 0.4rem;
		right: 0.5rem;
		background: none;
		border: none;
		color: var(--text-faint);
		font-size: 1.1rem;
		line-height: 1;
		cursor: pointer;
		padding: 0 0.3rem;
	}
	.card-x:hover {
		color: var(--text);
	}

	/* ── toast ── */
	.toast {
		padding: 0.85rem 1rem;
		border-left: 3px solid var(--accent);
		line-height: 1.45;
		cursor: pointer;
		white-space: pre-wrap;
	}
	.toast.error {
		border-left-color: var(--danger);
		color: var(--danger);
	}
	.toast.success {
		border-left-color: var(--success-strong);
		color: var(--success);
	}
	/* DEV-266: 같은 알림 재발생 횟수 뱃지. */
	.toast .count {
		margin-left: 0.4rem;
		font-size: 0.72rem;
		font-weight: 700;
		color: var(--text-muted);
		background: color-mix(in srgb, var(--text) 10%, transparent);
		border-radius: 999px;
		padding: 0.05rem 0.4rem;
	}
	/* DEV-266: "+N개 더" 축약 칩. */
	.more-chip {
		align-self: flex-end;
		padding: 0.3rem 0.7rem;
		font-size: 0.75rem;
		color: var(--text-muted);
		cursor: pointer;
	}
	.more-chip:hover {
		color: var(--text);
	}

	/* ── update ── */
	.upd {
		padding: 0.85rem 2rem 0.85rem 1rem;
		border-left: 3px solid var(--success-strong);
	}
	.upd.err {
		border-left-color: var(--danger);
	}
	.upd .t-title {
		margin: 0;
		font-weight: 600;
	}
	.upd .t-title.ok {
		color: var(--success);
	}
	.upd .t-title.err {
		color: var(--danger);
	}
	.upd .t-msg {
		margin: 0.35rem 0 0;
		color: var(--text-muted);
		font-size: 0.8rem;
		word-break: break-word;
	}
	.upd details {
		margin: 0.5rem 0;
		color: var(--text-muted);
	}
	.upd pre {
		white-space: pre-wrap;
		background: var(--bg);
		border: 1px solid var(--bg-subtle);
		border-radius: 6px;
		padding: 0.5rem 0.75rem;
		max-height: 8rem;
		overflow-y: auto;
		margin: 0.4rem 0 0;
		scrollbar-width: none;
	}
	.upd pre::-webkit-scrollbar {
		display: none;
	}
	.upd .btn-primary {
		margin-top: 0.5rem;
		padding: 0.4rem 0.9rem;
		background: var(--btn-primary-bg);
		border: 1px solid var(--btn-primary-border);
		border-radius: 6px;
		color: var(--btn-primary-text);
		font-size: 0.85rem;
		cursor: pointer;
	}
	.upd .btn-primary:hover {
		background: var(--btn-primary-bg-hover);
		border-color: var(--btn-primary-border-hover);
	}

	/* ── schema ── */
	.schema {
		display: flex;
		flex-direction: column;
		gap: 0.5rem;
		padding: 0.85rem 2rem 0.85rem 1rem;
		border-left: 3px solid var(--orange);
	}
	.schema .msg {
		display: flex;
		flex-direction: column;
		gap: 0.3rem;
	}
	.schema .msg strong {
		font-weight: 600;
		color: var(--orange);
	}
	.schema .detail {
		font-size: 0.78rem;
		color: var(--text-muted);
	}
	.schema .detail code {
		background: color-mix(in srgb, var(--text) 10%, transparent);
		padding: 0 0.25rem;
		border-radius: 3px;
		font-size: 0.85em;
		color: var(--text);
	}
	.schema .btn-update {
		align-self: flex-start;
		background: var(--warning);
		color: var(--bg);
		border: none;
		border-radius: 4px;
		padding: 0.3rem 0.85rem;
		font-size: 0.78rem;
		font-weight: 600;
		cursor: pointer;
	}
	.schema .btn-update:hover {
		background: color-mix(in srgb, var(--warning) 80%, white);
	}
</style>
