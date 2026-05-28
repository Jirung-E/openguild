<!--
  DEV-063: 업데이트 알림 배너 + 수동 체크 버튼.

  방식 (완전 자동 X — 알림 후 사용자 선택):
  - 앱 시작 시 background silent 체크 — 새 버전 있을 때만 배너.
  - DEV-083: 켜져 있는 동안 주기적 (6시간) 재확인 — 장시간 실행 세션도 감지.
  - 사용자가 직접 "업데이트 확인" 도 가능 (Nav 에서 호출).
  - 새 버전: release notes + "지금 업데이트" / "나중에".
  - 다운로드/설치/재시작은 사용자가 "지금 업데이트" 누를 때만.
-->
<script lang="ts">
	import { onMount } from 'svelte';
	import { get } from 'svelte/store';
	import {
		updateState,
		checkForUpdate,
		downloadAndRelaunch,
		dismissUpdate
	} from '$lib/api/updater';

	// DEV-083: 주기적 재확인 간격. 너무 짧으면 GitHub rate / 노이즈, 너무 길면
	// 장시간 켜둔 세션이 새 버전 놓침. 6시간이 타협점.
	const PERIODIC_CHECK_MS = 6 * 60 * 60 * 1000;

	onMount(() => {
		// 시작 시 1회 silent 체크 — 최신이면 아무 것도 안 뜸.
		checkForUpdate({ silent: true });

		// 켜져 있는 동안 주기적 재확인. 단, 이미 안내 중 / 다운로드 중이면
		// 사용자 흐름 방해하지 않게 skip.
		const handle = setInterval(() => {
			const s = get(updateState).status;
			if (s === 'available' || s === 'downloading' || s === 'ready') return;
			checkForUpdate({ silent: true });
		}, PERIODIC_CHECK_MS);
		return () => clearInterval(handle);
	});
</script>

{#if $updateState.status === 'available'}
	<div class="upd-banner" role="alert">
		<div class="upd-main">
			<span class="upd-title">새 버전 {$updateState.version} 사용 가능</span>
			{#if $updateState.notes}
				<details class="upd-notes">
					<summary>릴리즈 노트</summary>
					<pre>{$updateState.notes}</pre>
				</details>
			{/if}
		</div>
		<div class="upd-actions">
			<button class="upd-btn primary" onclick={() => downloadAndRelaunch()}>지금 업데이트</button>
			<button class="upd-btn" onclick={() => dismissUpdate()}>나중에</button>
		</div>
	</div>
{:else if $updateState.status === 'downloading'}
	<div class="upd-banner" role="status">
		<div class="upd-main">
			<span class="upd-title">{$updateState.version} 다운로드 중…</span>
			<div class="upd-progress">
				<div
					class="upd-progress-fill"
					class:indeterminate={$updateState.pct === null}
					style:width={$updateState.pct !== null ? `${$updateState.pct}%` : undefined}
				></div>
			</div>
			{#if $updateState.pct !== null}
				<span class="upd-pct">{$updateState.pct}%</span>
			{/if}
		</div>
	</div>
{:else if $updateState.status === 'ready'}
	<div class="upd-banner" role="status">
		<div class="upd-main">
			<span class="upd-title">업데이트 설치 완료 — 재시작 중…</span>
		</div>
	</div>
{:else if $updateState.status === 'checking'}
	<div class="upd-banner subtle" role="status">
		<div class="upd-main"><span class="upd-title">업데이트 확인 중…</span></div>
	</div>
{:else if $updateState.status === 'uptodate'}
	<div class="upd-banner subtle" role="status">
		<div class="upd-main"><span class="upd-title">최신 버전입니다.</span></div>
		<div class="upd-actions">
			<button class="upd-btn" onclick={() => dismissUpdate()}>닫기</button>
		</div>
	</div>
{:else if $updateState.status === 'error'}
	<div class="upd-banner error" role="alert">
		<div class="upd-main">
			<span class="upd-title">업데이트 확인 실패</span>
			<span class="upd-err">{$updateState.message}</span>
		</div>
		<div class="upd-actions">
			<button class="upd-btn" onclick={() => dismissUpdate()}>닫기</button>
		</div>
	</div>
{/if}

<style>
	.upd-banner {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 1rem;
		padding: 0.5rem 1.5rem;
		background: #16301f;
		border-bottom: 1px solid #2ea043;
		color: #c9d1d9;
		font-size: 0.85rem;
	}
	.upd-banner.subtle { background: #161b22; border-bottom-color: #30363d; color: #8b949e; }
	.upd-banner.error { background: #2a1010; border-bottom-color: #f85149; }

	.upd-main { display: flex; align-items: center; gap: 0.75rem; flex: 1; min-width: 0; }
	.upd-title { font-weight: 600; white-space: nowrap; }
	.upd-err { color: #f85149; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }

	.upd-notes { font-size: 0.8rem; color: #8b949e; }
	.upd-notes summary { cursor: pointer; }
	.upd-notes pre {
		margin: 0.4rem 0 0;
		white-space: pre-wrap;
		max-height: 8rem;
		overflow-y: auto;
		background: #0d1117;
		border: 1px solid #21262d;
		border-radius: 6px;
		padding: 0.5rem 0.75rem;
	}

	.upd-progress {
		width: 180px;
		height: 6px;
		background: #21262d;
		border-radius: 3px;
		overflow: hidden;
	}
	.upd-progress-fill {
		height: 100%;
		background: #2ea043;
		transition: width 0.2s;
	}
	/* contentLength 미상 시 — 좌우 왕복 애니메이션. */
	.upd-progress-fill.indeterminate {
		width: 35% !important;
		animation: upd-indet 1.1s ease-in-out infinite;
	}
	@keyframes upd-indet {
		0% { margin-left: -35%; }
		100% { margin-left: 100%; }
	}
	.upd-pct { font-variant-numeric: tabular-nums; color: #8b949e; }

	.upd-actions { display: flex; gap: 0.4rem; flex-shrink: 0; }
	.upd-btn {
		padding: 0.3rem 0.75rem;
		border-radius: 6px;
		border: 1px solid #30363d;
		background: transparent;
		color: #c9d1d9;
		font-size: 0.8rem;
		cursor: pointer;
	}
	.upd-btn:hover { background: #21262d; }
	.upd-btn.primary { background: #238636; border-color: #2ea043; color: #fff; }
	.upd-btn.primary:hover { background: #2ea043; }
</style>
