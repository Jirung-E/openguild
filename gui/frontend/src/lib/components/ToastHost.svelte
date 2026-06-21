<!--
  앱 공용 toast 표시 영역. layout 에 1회 마운트. showToast() 로 추가된 메시지를
  우상단에 쌓아 보여주고, 클릭 또는 자동(기본 4초) 으로 닫힘. alert() 대체 —
  모달/보드 어디서든 동일 UI.
-->
<script lang="ts">
	import { toasts, dismissToast } from '$lib/stores/toast';
</script>

<div class="toast-wrap" role="status" aria-live="polite">
	{#each $toasts as t (t.id)}
		<button class="toast {t.variant}" onclick={() => dismissToast(t.id)} title="닫기">
			{t.message}
		</button>
	{/each}
</div>

<style>
	/* 모달(z 100) / admin toast(z 1000) 보다 위. */
	.toast-wrap {
		position: fixed;
		top: 1rem;
		right: 1rem;
		z-index: 2000;
		display: flex;
		flex-direction: column;
		gap: 0.5rem;
		max-width: calc(26.25rem * var(--popup-scale, 1)); /* BUG-064 */
		pointer-events: none;
	}
	.toast {
		text-align: left;
		padding: 0.7rem 1rem;
		border-radius: 6px;
		border: 1px solid var(--border);
		background: var(--bg-elevated);
		color: var(--text-strong);
		font-size: 0.875rem;
		line-height: 1.45;
		box-shadow: 0 8px 24px rgba(0, 0, 0, 0.4);
		cursor: pointer;
		pointer-events: auto;
		white-space: pre-wrap;
		animation: toast-in 0.18s ease-out;
	}
	.toast.error {
		border-color: color-mix(in srgb, var(--danger) 55%, transparent);
		background: color-mix(in srgb, var(--danger) 14%, var(--bg-elevated));
		color: var(--danger);
	}
	.toast.success {
		border-color: color-mix(in srgb, var(--success) 55%, transparent);
		background: color-mix(in srgb, var(--success) 14%, var(--bg-elevated));
		color: var(--success);
	}
	@keyframes toast-in {
		from {
			opacity: 0;
			transform: translateY(-8px);
		}
		to {
			opacity: 1;
			transform: translateY(0);
		}
	}
</style>
