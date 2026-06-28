<!--
  DEV-063: 업데이트 알림 + 수동 체크 결과 — 단일 floating toast (우하단).

  DEV-194 후속: 이전엔 이 컴포넌트가 상단 배너(layout 밀어냄, actionable
  상태만) + welcome/settings 각자의 floating toast(checking/uptodate/error
  포함, 전부 중복 구현) 셋으로 나뉘어 있었다 — 같은 메시지가 트리거(자동 vs
  수동)에 따라 다른 위치/스타일로 뜨고, 수동 확인 시 available 등은 전역
  배너 + 페이지 토스트에 동시에 떴다. 결정(옵션 A): **모든 상태를 여기
  하나로 통합** — layout 에 한 번만 mount, 우하단 floating(레이아웃 안
  밀어냄), idle 이 아닌 모든 상태를 렌더. welcome/settings 는 이제
  `checkForUpdate()` 호출만 하고 결과 표시는 전부 이 컴포넌트가 담당.

  - 앱 시작 시 background silent 체크 — 새 버전 있을 때만 노출.
  - DEV-083: 켜져 있는 동안 주기적 (6시간) 재확인 — 장시간 실행 세션도 감지.
  - 사용자가 직접 "업데이트 확인" 도 가능 (Nav / welcome / settings 어디서든).
  - 새 버전: release notes + "지금 업데이트" / "닫기".
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
	import { detectEnvironment } from '$lib/api/transport';
	// DEV-074 fix17: release notes <pre> 도 overlay.
	import OverlayScrollbar from './OverlayScrollbar.svelte';

	let notesEl: HTMLPreElement | undefined = $state(undefined);
	const isTauri = detectEnvironment() === 'tauri';

	// DEV-083: 주기적 재확인 간격. 너무 짧으면 GitHub rate / 노이즈, 너무 길면
	// 장시간 켜둔 세션이 새 버전 놓침. 6시간이 타협점.
	const PERIODIC_CHECK_MS = 6 * 60 * 60 * 1000;

	onMount(() => {
		if (!isTauri) return; // 브라우저 모드 — 업데이트 개념 없음.

		// 시작 시 1회 silent 체크 — 최신이면 아무 것도 안 뜸(updater.ts 가
		// silent 일 때 uptodate/error 를 idle 로 되돌림).
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

{#if isTauri && $updateState.status !== 'idle'}
	<div class="upd-toast" class:err={$updateState.status === 'error'} role="status">
		<button class="upd-toast-x" title="닫기" onclick={() => dismissUpdate()}>×</button>
		{#if $updateState.status === 'checking'}
			<p class="t-title">업데이트 확인 중…</p>
		{:else if $updateState.status === 'uptodate'}
			<p class="t-title ok">최신 버전입니다.</p>
		{:else if $updateState.status === 'available'}
			<p class="t-title">새 버전 <strong>{$updateState.version}</strong> 사용 가능</p>
			{#if $updateState.notes}
				<details>
					<summary>릴리즈 노트</summary>
					<pre bind:this={notesEl}>{$updateState.notes}</pre>
					{#if notesEl}
						<OverlayScrollbar target={notesEl} />
					{/if}
				</details>
			{/if}
			<button class="btn-primary" onclick={() => downloadAndRelaunch()}>
				지금 업데이트 (다운로드 + 재시작)
			</button>
		{:else if $updateState.status === 'downloading'}
			<p class="t-title">
				다운로드 중… {$updateState.pct !== null ? `${$updateState.pct}%` : ''}
			</p>
		{:else if $updateState.status === 'ready'}
			<p class="t-title ok">설치 완료 — 재시작 중…</p>
		{:else if $updateState.status === 'error'}
			<p class="t-title err">확인 실패</p>
			<p class="t-msg">{$updateState.message}</p>
		{/if}
	</div>
{/if}

<style>
	/* DEV-085 / DEV-194: 업데이트 결과 floating toast — fixed, 우하단.
		 레이아웃 안 밀어냄(이전 상단 배너의 근본 문제 해결). */
	.upd-toast {
		position: fixed;
		right: 1.5rem;
		bottom: 1.5rem;
		z-index: 60;
		max-width: calc(22.5rem * var(--popup-scale, 1)); /* BUG-064 */
		padding: 0.85rem 2rem 0.85rem 1rem;
		background: var(--bg-elevated);
		border: 1px solid var(--border);
		border-left: 3px solid var(--success-strong);
		border-radius: 8px;
		box-shadow: 0 6px 20px rgba(0, 0, 0, 0.5);
		font-size: 0.85rem;
		color: var(--text);
	}
	.upd-toast.err {
		border-left-color: var(--danger);
	}
	.upd-toast-x {
		position: absolute;
		top: 0.4rem;
		right: 0.5rem;
		background: none;
		border: none;
		color: var(--text-faint);
		font-size: 1.1rem;
		line-height: 1;
		cursor: pointer;
	}
	.upd-toast-x:hover {
		color: var(--text);
	}
	.upd-toast .t-title {
		margin: 0;
		font-weight: 600;
	}
	.upd-toast .t-title.ok {
		color: var(--success);
	}
	.upd-toast .t-title.err {
		color: var(--danger);
	}
	.upd-toast .t-msg {
		margin: 0.35rem 0 0;
		color: var(--text-muted);
		font-size: 0.8rem;
		word-break: break-word;
	}
	.upd-toast details {
		margin: 0.5rem 0;
		color: var(--text-muted);
	}
	.upd-toast pre {
		white-space: pre-wrap;
		background: var(--bg);
		border: 1px solid var(--bg-subtle);
		border-radius: 6px;
		padding: 0.5rem 0.75rem;
		max-height: 8rem;
		overflow-y: auto;
		margin: 0.4rem 0 0;
		/* DEV-074 fix17: native scrollbar 숨김 — OverlayScrollbar 가 대신 그림. */
		scrollbar-width: none;
	}
	.upd-toast pre::-webkit-scrollbar {
		display: none;
	}
	.upd-toast .btn-primary {
		margin-top: 0.5rem;
		padding: 0.4rem 0.9rem;
		background: var(--btn-primary-bg);
		border: 1px solid var(--btn-primary-border);
		border-radius: 6px;
		color: var(--btn-primary-text);
		font-size: 0.85rem;
		cursor: pointer;
	}
	.upd-toast .btn-primary:hover {
		background: var(--btn-primary-bg-hover);
		border-color: var(--btn-primary-border-hover);
	}
</style>
