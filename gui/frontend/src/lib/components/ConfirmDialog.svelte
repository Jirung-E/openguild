<!--
  DEV-118: 인앱 확인 모달 — 브라우저 `window.confirm()` 대신.

  이유:
   - Tauri WebView 의 native confirm 은 환경에 따라 dialog 안 뜨고 silent
     return — 사용자가 "삭제했더니 확인 없이 진행" 으로 인식.
   - 일관된 시각 (theme / 토큰 / 다크모드).
   - 키보드 접근성 (Esc 닫기, Enter 확인).

  사용:
  ```svelte
  let confirmOpen = $state(false);
  let onConfirm: () => void = () => {};

  function askDelete(...) {
    onConfirm = () => doDelete(...);
    confirmOpen = true;
  }

  <ConfirmDialog
    open={confirmOpen}
    title="삭제"
    message={`'${name}' 을 삭제할까요?`}
    confirmLabel="삭제"
    danger
    on:confirm={() => { confirmOpen = false; onConfirm(); }}
    on:cancel={() => (confirmOpen = false)}
  />
  ```
-->
<script lang="ts">
	type Props = {
		open: boolean;
		title?: string;
		message: string;
		confirmLabel?: string;
		cancelLabel?: string;
		danger?: boolean;
		onconfirm?: () => void;
		oncancel?: () => void;
	};

	let {
		open,
		title = '확인',
		message,
		confirmLabel = '확인',
		cancelLabel = '취소',
		danger = false,
		onconfirm,
		oncancel
	}: Props = $props();

	function close() {
		oncancel?.();
	}
	function confirm() {
		onconfirm?.();
	}

	function onkeydown(e: KeyboardEvent) {
		if (!open) return;
		if (e.key === 'Escape') {
			e.preventDefault();
			close();
		} else if (e.key === 'Enter') {
			e.preventDefault();
			confirm();
		}
	}
</script>

<svelte:window onkeydown={onkeydown} />

{#if open}
	<div
		class="ov"
		role="presentation"
		onclick={(e) => {
			if (e.target === e.currentTarget) close();
		}}
	>
		<div class="modal" role="alertdialog" aria-modal="true" tabindex="-1">
			<h3 class="modal-title">{title}</h3>
			<p class="modal-msg">{message}</p>
			<div class="modal-actions">
				<button class="btn-no" onclick={close}>{cancelLabel}</button>
				<button class="btn-yes" class:danger onclick={confirm}>{confirmLabel}</button>
			</div>
		</div>
	</div>
{/if}

<style>
	.ov {
		position: fixed;
		inset: 0;
		background: rgba(0, 0, 0, 0.55);
		z-index: 200;
		display: flex;
		align-items: center;
		justify-content: center;
		padding: 1rem;
	}
	.modal {
		background: var(--bg-elevated);
		border: 1px solid var(--border);
		border-radius: 10px;
		width: 100%;
		max-width: 26.25rem; /* BUG-064 */
		padding: 1.1rem 1.4rem 1.2rem;
		box-shadow: 0 16px 40px rgba(0, 0, 0, 0.55);
		color: var(--text);
	}
	.modal-title {
		margin: 0 0 0.5rem;
		font-size: 1rem;
		font-weight: 600;
		color: var(--text-strong);
	}
	.modal-msg {
		margin: 0 0 1rem;
		font-size: 0.875rem;
		color: var(--text);
		white-space: pre-wrap;
		word-break: break-word;
	}
	.modal-actions {
		display: flex;
		gap: 0.5rem;
		justify-content: flex-end;
	}
	.btn-no {
		padding: 0.4rem 1rem;
		background: transparent;
		border: 1px solid var(--border);
		border-radius: 6px;
		color: var(--text-muted);
		font-size: 0.875rem;
		cursor: pointer;
	}
	.btn-no:hover { background: var(--bg-subtle); }
	.btn-yes {
		padding: 0.4rem 1.1rem;
		background: var(--btn-primary-bg);
		border: 1px solid var(--btn-primary-border);
		border-radius: 6px;
		color: var(--btn-primary-text);
		font-size: 0.875rem;
		cursor: pointer;
	}
	.btn-yes:hover { background: var(--btn-primary-bg-hover); border-color: var(--btn-primary-border-hover); }
	.btn-yes.danger {
		background: color-mix(in srgb, var(--danger) 18%, transparent);
		border-color: var(--danger);
		color: var(--danger);
	}
	.btn-yes.danger:hover { background: color-mix(in srgb, var(--danger) 32%, transparent); }
</style>
