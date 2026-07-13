<!--
  BUG-041: DB schema 가 현재 binary 보다 새로운 경우 사용자에게 알림.

  배경 — 신규 빌드 (mig N+k) 가 사용자의 길드 DB 에 적용된 뒤, 이전 release
  binary 가 같은 DB 를 열려고 하면 schema mismatch. `set_ignore_missing(true)`
  로 panic 자체는 막지만, 사용자가 모르고 계속 옛 GUI 를 쓸 위험 → 본 banner
  로 명시 알림.

  Tauri 환경에서만 의미 있음 — HTTP 모드는 backend 가 알아서 처리.

  표시 내용:
  - 현재 GUI 버전 (binary_version).
  - GUI 가 알고 있는 max migration vs DB 가 가진 ahead version 들.
  - GitHub release 페이지 링크 (가장 명확한 행동 가이드).
-->
<script lang="ts">
	import { onMount } from 'svelte';
	import { detectEnvironment } from '$lib/api/transport';

	let isAhead = $state(false);
	let aheadVersions = $state<number[]>([]);
	let binaryVersion = $state('');
	let latestKnown = $state<number | null>(null);
	let dismissed = $state(false);

	// GitHub release 페이지 — README 의 release 절차와 동기화.
	const RELEASE_URL = 'https://github.com/Jirung-E/openguild/releases/latest';

	const DISMISS_KEY = 'openguild.schemaAheadBannerDismissed';

	async function check() {
		if (detectEnvironment() !== 'tauri') return;
		try {
			const { invoke } = await import('@tauri-apps/api/core');
			const status = (await invoke('get_db_schema_status')) as {
				is_ahead: boolean;
				ahead_versions: number[];
				binary_version: string;
				latest_known_migration: number | null;
			};
			isAhead = status.is_ahead;
			aheadVersions = status.ahead_versions ?? [];
			binaryVersion = status.binary_version ?? '';
			latestKnown = status.latest_known_migration;
			// 같은 ahead 셋에 대해 한 번 닫으면 그 세션 동안 안 다시 보임 —
			// localStorage 의 signature 비교 (binary_version 도 포함 — 사용자가
			// 업데이트해서 binary 가 바뀌면 다시 노출).
			const sig = `${binaryVersion}|${aheadVersions.join(',')}`;
			const seen = localStorage.getItem(DISMISS_KEY);
			if (seen === sig) {
				dismissed = true;
			}
		} catch {
			// 명령 미존재 (구 backend) — 조용히 무시.
		}
	}

	function dismiss() {
		dismissed = true;
		try {
			const sig = `${binaryVersion}|${aheadVersions.join(',')}`;
			localStorage.setItem(DISMISS_KEY, sig);
		} catch {
			/* ignore */
		}
	}

	// BUG-040: 외부 링크 → 시스템 브라우저. layout 의 anchor intercept 가 일반
	// `<a>` 는 자동 처리하지만 본 banner 는 button 이라 명시적으로 openUrl 호출.
	async function openRelease() {
		try {
			const { openUrl } = await import('@tauri-apps/plugin-opener');
			await openUrl(RELEASE_URL);
		} catch {
			window.open(RELEASE_URL, '_blank');
		}
	}

	onMount(() => {
		check();
	});

	// 표시용 — 가장 큰 ahead version.
	let maxAhead = $derived(aheadVersions.length > 0 ? Math.max(...aheadVersions) : null);
</script>

{#if isAhead && !dismissed}
	<!-- BUG-139: 상단 in-flow 배너(레이아웃 밀어냄) → UpdateBanner 와 동일한
	     플로팅 toast 패턴. UpdateBanner(우하단)/ToastHost(우상단)와 안 겹치게
	     좌상단에 배치. -->
	<div class="banner" role="alert">
		<button class="btn-dismiss" onclick={dismiss} aria-label="닫기" title="닫기">×</button>
		<div class="msg">
			<strong>⚠ 이 길드 DB 가 현재 GUI 버전보다 새롭습니다</strong>
			<span class="detail">
				현재 GUI: <code>v{binaryVersion}</code>
				{#if latestKnown != null}
					(migration {latestKnown} 까지 인식)
				{/if}
				· 이 길드 DB: migration <strong>{maxAhead}</strong>
				{#if aheadVersions.length > 1}
					(+{aheadVersions.length - 1} more)
				{/if}
				적용됨. 일부 데이터가 제대로 표시되지 않거나 reindex 가 실패할 수 있습니다.
			</span>
		</div>
		<button class="btn-update" onclick={openRelease}> 최신 버전 받기 ↗ </button>
	</div>
{/if}

<style>
	/* BUG-139: UpdateBanner(.upd-toast, 우하단 fixed)와 동일한 플로팅 카드
	   패턴 — 레이아웃을 밀어내지 않음. UpdateBanner/ToastHost 와 안 겹치게
	   좌상단(top-left)에 배치. */
	.banner {
		position: fixed;
		top: calc(1rem + var(--titlebar-h, 0px));
		left: 1rem;
		z-index: 60;
		display: flex;
		flex-direction: column;
		gap: 0.5rem;
		max-width: calc(24rem * var(--popup-scale, 1));
		padding: 0.85rem 2rem 0.85rem 1rem;
		background: var(--bg-elevated);
		border: 1px solid var(--border);
		border-left: 3px solid var(--orange);
		border-radius: 8px;
		box-shadow: 0 6px 20px rgba(0, 0, 0, 0.5);
		font-size: 0.85rem;
		color: var(--text);
	}
	.msg {
		display: flex;
		flex-direction: column;
		gap: 0.3rem;
	}
	.msg strong {
		font-weight: 600;
		color: var(--orange);
	}
	.detail {
		font-size: 0.78rem;
		color: var(--text-muted);
	}
	.detail code {
		background: color-mix(in srgb, var(--text) 10%, transparent);
		padding: 0 0.25rem;
		border-radius: 3px;
		font-size: 0.85em;
		color: var(--text);
	}
	.btn-update {
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
	.btn-update:hover {
		background: color-mix(in srgb, var(--warning) 80%, white);
	}
	.btn-dismiss {
		position: absolute;
		top: 0.4rem;
		right: 0.5rem;
		background: none;
		border: none;
		color: var(--text-muted);
		cursor: pointer;
		font-size: 1.1rem;
		line-height: 1;
		padding: 0 0.3rem;
	}
	.btn-dismiss:hover {
		color: var(--text);
	}
</style>
