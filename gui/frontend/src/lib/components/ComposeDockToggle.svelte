<!--
  DEV-442: 댓글 · 메모 팝업([[ComposeDock]]) 켜기/끄기 — [M↓] 옆에 둔다(admin).

  켜져 있으면 [M↓] 처럼 칠해진다. 앱 전체에 하나뿐인 값이라 어느 입력창에서 눌러도 같다.
-->
<script lang="ts">
	import { locale, t } from '$lib/stores/locale';
	import { composeDockEnabled, toggleComposeDock } from '$lib/stores/composeDockEnabled';

	const title = $derived(
		$composeDockEnabled ? t('compose.dockOn', $locale) : t('compose.dockOff', $locale)
	);
</script>

<button
	type="button"
	class="dock-toggle"
	class:active={$composeDockEnabled}
	onclick={toggleComposeDock}
	{title}
	aria-label={title}
	aria-pressed={$composeDockEnabled}
>
	<!-- 화면(큰 틀) 아래에 붙은 입력창(아래 막대). -->
	<svg
		viewBox="0 0 16 16"
		fill="none"
		stroke="currentColor"
		stroke-width="1.4"
		stroke-linecap="round"
		stroke-linejoin="round"
		aria-hidden="true"
	>
		<rect x="2" y="2.5" width="12" height="11" rx="1.5" />
		<rect x="4" y="9.5" width="8" height="2" rx="0.6" fill="currentColor" />
	</svg>
</button>

<style>
	/* [M↓](`.ce-toggle`)과 같은 모양 — 나란히 놓인다. */
	.dock-toggle {
		flex-shrink: 0;
		align-self: center;
		display: inline-flex;
		align-items: center;
		padding: var(--sp-2) var(--sp-6);
		/* 높이를 [M↓] 의 글자 줄에 맞춘다 — 그림만 두면 줄 높이가 없어 4px 낮았다. */
		font-size: var(--fs-xs);
		font-family: var(--font-mono);
		border: var(--bw) solid var(--border);
		border-radius: var(--r-sm);
		background: transparent;
		color: var(--text-muted);
		cursor: pointer;
	}
	.dock-toggle:hover {
		color: var(--text);
	}
	.dock-toggle.active {
		background: color-mix(in srgb, var(--accent) 18%, transparent);
		border-color: var(--accent);
		color: var(--text);
	}
	/* `display: block` 이 아니면 svg 가 글자처럼 놓여 기준선만큼 아래로 내려간다. */
	.dock-toggle svg {
		display: block;
		width: 0.875rem;
		height: 0.875rem;
		margin-block: calc((1lh - 0.875rem) / 2); /* token-ok: 그림 높이를 글자 줄 높이에 맞추는 계산 */
	}
</style>
