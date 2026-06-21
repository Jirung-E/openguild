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
	// DEV-074 fix17: release notes <pre> 도 overlay.
	import OverlayScrollbar from './OverlayScrollbar.svelte';

	let notesEl: HTMLPreElement | undefined = $state(undefined);

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
					<pre bind:this={notesEl}>{$updateState.notes}</pre>
					{#if notesEl}
						<OverlayScrollbar target={notesEl} />
					{/if}
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
{/if}

<!-- DEV-085: checking / uptodate / error 같은 transient (수동 체크 피드백) 는
     전역 배너에서 제거 — 상단 배너가 레이아웃을 밀어내는 문제. 그 상태들은
     설정 페이지의 floating toast 가 담당. 전역 배너는 actionable / 진행 중
     (available / downloading / ready) 만 — 어디서든 봐야 하는 것. -->

<style>
	.upd-banner {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 1rem;
		padding: 0.5rem 1.5rem;
		background: color-mix(in srgb, var(--success) 18%, var(--bg-elevated));
		border-bottom: 1px solid var(--success-strong);
		color: var(--text);
		font-size: 0.85rem;
	}
	.upd-main {
		display: flex;
		align-items: center;
		gap: 0.75rem;
		flex: 1;
		min-width: 0;
	}
	.upd-title {
		font-weight: 600;
		white-space: nowrap;
	}

	.upd-notes {
		font-size: 0.8rem;
		color: var(--text-muted);
	}
	.upd-notes summary {
		cursor: pointer;
	}
	.upd-notes pre {
		margin: 0.4rem 0 0;
		white-space: pre-wrap;
		max-height: 8rem;
		overflow-y: auto;
		background: var(--bg);
		border: 1px solid var(--bg-subtle);
		border-radius: 6px;
		padding: 0.5rem 0.75rem;
		/* DEV-074 fix17: native scrollbar 숨김 — OverlayScrollbar 가 대신 그림. */
		scrollbar-width: none;
	}
	.upd-notes pre::-webkit-scrollbar {
		display: none;
	}

	.upd-progress {
		width: 11.25rem; /* BUG-064 */
		height: 6px;
		background: var(--bg-subtle);
		border-radius: 3px;
		overflow: hidden;
	}
	.upd-progress-fill {
		height: 100%;
		background: var(--success-strong);
		transition: width 0.2s;
	}
	/* contentLength 미상 시 — 좌우 왕복 애니메이션. */
	.upd-progress-fill.indeterminate {
		width: 35% !important;
		animation: upd-indet 1.1s ease-in-out infinite;
	}
	@keyframes upd-indet {
		0% {
			margin-left: -35%;
		}
		100% {
			margin-left: 100%;
		}
	}
	.upd-pct {
		font-variant-numeric: tabular-nums;
		color: var(--text-muted);
	}

	.upd-actions {
		display: flex;
		gap: 0.4rem;
		flex-shrink: 0;
	}
	.upd-btn {
		padding: 0.3rem 0.75rem;
		border-radius: 6px;
		border: 1px solid var(--border);
		background: transparent;
		color: var(--text);
		font-size: 0.8rem;
		cursor: pointer;
	}
	.upd-btn:hover {
		background: var(--bg-subtle);
	}
	.upd-btn.primary {
		background: var(--btn-primary-bg);
		border-color: var(--btn-primary-border);
		color: var(--btn-primary-text);
	}
	.upd-btn.primary:hover {
		background: var(--btn-primary-bg-hover);
		border-color: var(--btn-primary-border-hover);
	}
</style>
