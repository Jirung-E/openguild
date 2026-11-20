<!--
  DEV-486: 새 버전 알림 카드 안의 할 일 — 예전 업데이트 상태 카드에 있던 것(릴리스 노트 · 설치 단추 · 받는 중
  진행률)을 알림 카드 안에 둔다. 떠 있는 알림과 알림함의 줄이 같은 것을 보인다 — 알림을 닫아도 알림함에서 설치한다.
  카드를 누르는 것(열기)과 섞이지 않게 이 안의 누름 · 키는 카드로 올려 보내지 않는다.
-->
<script lang="ts">
	import OverlayScrollbar from './OverlayScrollbar.svelte';
	import { locale, t } from '$lib/stores/locale';
	import { availableUpdate, downloadAndRelaunch, updateState } from '$lib/api/updater';

	let notesEl: HTMLPreElement | undefined = $state(undefined);
	const busy = $derived($updateState.status === 'downloading' || $updateState.status === 'ready');
</script>

<!-- 카드(열기)로 누름 · 키를 올려 보내지 않는 칸일 뿐이라 역할이 없다. -->
<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="nu" onclick={(e) => e.stopPropagation()} onkeydown={(e) => e.stopPropagation()}>
	{#if $updateState.status === 'downloading'}
		<p class="nu-state">
			{t('update.downloading', $locale)}
			{$updateState.pct !== null ? `${$updateState.pct}%` : ''}
		</p>
	{:else if $updateState.status === 'ready'}
		<p class="nu-state ok">{t('update.ready', $locale)}</p>
	{/if}
	{#if $availableUpdate && !busy}
		{#if $availableUpdate.notes}
			<details>
				<summary>{t('update.releaseNotes', $locale)}</summary>
				<pre bind:this={notesEl}>{$availableUpdate.notes}</pre>
				{#if notesEl}
					<OverlayScrollbar target={notesEl} />
				{/if}
			</details>
		{/if}
		<button class="btn btn-primary btn-sm" onclick={() => void downloadAndRelaunch()}>
			{t('update.installBtn', $locale)}
		</button>
	{/if}
</div>

<style>
	.nu {
		display: flex;
		flex-direction: column;
		align-items: flex-start;
		gap: var(--sp-6);
		margin-top: var(--sp-6);
		cursor: default;
	}
	.nu-state {
		margin: 0;
		font-size: var(--fs-sm);
		color: var(--text-muted);
	}
	.nu-state.ok {
		color: var(--success);
	}
	details {
		align-self: stretch;
		position: relative;
		font-size: var(--fs-sm);
		color: var(--text-muted);
	}
	summary {
		cursor: pointer;
	}
	pre {
		white-space: pre-wrap;
		background: var(--bg);
		border: var(--bw) solid var(--bg-subtle);
		border-radius: var(--r-md);
		padding: var(--sp-8) var(--sp-12);
		max-height: 8rem;
		overflow-y: auto;
		margin: var(--sp-6) 0 0;
		scrollbar-width: none;
	}
	pre::-webkit-scrollbar {
		display: none;
	}
</style>
