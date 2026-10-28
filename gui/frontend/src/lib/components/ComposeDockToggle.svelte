<!--
  DEV-442: 댓글 · 메모 팝업([[ComposeDock]]) 켜기/끄기 — [M↓] 옆에 둔다(admin).

  켜져 있으면 [M↓] 처럼 칠해진다. DEV-459: 값은 입력창 종류(댓글 · 답글 · 메모)마다 따로다 —
  어느 종류인지는 감싼 [[ComposeDock]] 이 문맥으로 알려 준다.
-->
<script lang="ts">
	import { getContext } from 'svelte';
	import { locale, t } from '$lib/stores/locale';
	import { composeDockEnabled, toggleComposeDock } from '$lib/stores/composeDockEnabled';
	import { COMPOSE_DOCK, type ComposeDockContext } from '$lib/utils/compose-dock-context';

	const kind = getContext<ComposeDockContext | undefined>(COMPOSE_DOCK)?.kind ?? 'comment';
	const on = $derived($composeDockEnabled[kind]);
	const title = $derived(on ? t('compose.dockOn', $locale) : t('compose.dockOff', $locale));
</script>

<button
	type="button"
	class="dock-toggle"
	class:active={on}
	onclick={() => toggleComposeDock(kind)}
	{title}
	aria-label={title}
	aria-pressed={on}
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
		/* [M↓](`.ce-toggle`)와 **같은 크기**로 박는다(admin) — 값은 둘이 같아야 한다. */
		display: inline-flex;
		align-items: center;
		justify-content: center;
		box-sizing: border-box;
		width: 1.75rem;
		height: 1.5rem;
		padding: 0;
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
	}
</style>
