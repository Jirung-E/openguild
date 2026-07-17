<!--
  앱 공용 toast 표시 영역. layout 에 1회 마운트. showToast() 로 추가된 메시지를
  보여주고, 클릭 또는 자동(기본 4초) 으로 닫힘. alert() 대체 —
  모달/보드 어디서든 동일 UI.

  DEV-259(사용자 결정: "업데이트 확인 알림처럼 표시되어야"): 위치/스타일을
  UpdateBanner(우하단 카드, border-left 강조)와 통일 — 우상단 스택에서
  우하단 스택(새 항목이 아래)으로 이동. UpdateBanner/SchemaAheadBanner 는
  행동 필요·상태 유지형이라 별도 유지(동시 발생 정책은 후속 퀘스트).
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
	/* DEV-259: UpdateBanner(.upd-toast)와 동일 계열 — 우하단, 카드형.
	   UpdateBanner(z 60)보다 위에 잠깐 떠도 자동 소멸되므로 허용(완전한
	   동시 배치 정책은 알림 시스템 개선 퀘스트에서). */
	.toast-wrap {
		position: fixed;
		right: 1.5rem;
		bottom: 1.5rem;
		z-index: 2000;
		display: flex;
		flex-direction: column;
		gap: 0.5rem;
		max-width: calc(26.25rem * var(--popup-scale, 1)); /* BUG-064 */
		pointer-events: none;
	}
	.toast {
		text-align: left;
		padding: 0.85rem 1rem;
		border-radius: 8px;
		border: 1px solid var(--border);
		border-left: 3px solid var(--accent);
		background: var(--bg-elevated);
		color: var(--text);
		font-size: 0.85rem;
		line-height: 1.45;
		box-shadow: 0 6px 20px rgba(0, 0, 0, 0.5);
		cursor: pointer;
		pointer-events: auto;
		white-space: pre-wrap;
		animation: toast-in 0.18s ease-out;
	}
	.toast.error {
		border-left-color: var(--danger);
		color: var(--danger);
	}
	.toast.success {
		border-left-color: var(--success-strong);
		color: var(--success);
	}
	@keyframes toast-in {
		from {
			opacity: 0;
			transform: translateY(8px);
		}
		to {
			opacity: 1;
			transform: translateY(0);
		}
	}
</style>
