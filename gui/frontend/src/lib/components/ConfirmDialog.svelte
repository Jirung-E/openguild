<!--
  DEV-118: 인앱 확인 모달 — 브라우저 `window.confirm()` 대신.

  이유:
   - Tauri WebView 의 native confirm 은 환경에 따라 dialog 안 뜨고 silent
     return — 사용자가 "삭제했더니 확인 없이 진행" 으로 인식.
   - 일관된 시각 (theme / 토큰 / 다크모드).
   - 키보드 접근성 (Esc 닫기, Enter 는 포커스된 버튼).
   - BUG-272: 열리면 포커스가 대화상자 안으로 가고 Tab 이 밖으로 새지 않는다. 키는 대화상자에서만 받는다 —
     예전엔 window 에서 받아, 뒤 페이지에 포커스가 있어도 스친 Enter 가 '확인'을 눌렀다.
     위험한 동작(`danger`)은 '취소'에서 시작한다 — Enter 를 한 번 더 눌러도 지우지 않는다.

  DEV-440 후속: 틀은 `Modal`, 단추는 `ActionButtons` — 창마다 따로 만들던 확인 창(보드 상태 바꾸기 · 퀘스트 삭제 ·
  타입 바꾸기 · 관리 삭제 · 환영 화면 비우기 · 빼기)도 이걸 쓴다. 글 아래에 더 보일 것(경고 줄 · 파일 경로)은
  `children` 으로 넘긴다. 일하는 중(`busy`)이면 단추가 꺼지고 닫히지 않는다.

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
	import type { Snippet } from 'svelte';
	import ActionButtons from '$lib/components/ActionButtons.svelte';
	import Modal from '$lib/components/Modal.svelte';
	// DEV-205 모듈1: 기본 라벨 i18n. 호출부가 라벨을 넘기면 그대로 쓰고,
	// 미지정(undefined)일 때만 locale 사전 기본값으로 대체.
	import { locale, t } from '$lib/stores/locale';

	type Props = {
		open: boolean;
		title?: string;
		/** 글. 굵은 글 · 경로처럼 꾸밈이 필요하면 비우고 `children` 으로. */
		message?: string;
		confirmLabel?: string;
		cancelLabel?: string;
		danger?: boolean;
		/** 일하는 중 — 단추를 끄고 닫히지 않는다. */
		busy?: boolean;
		/** 막의 겹침 순서(`Modal` 의 `z`). */
		z?: number;
		/** 상자 폭(`Modal` 의 `size`) — 고를 것이 딸린 확인 창(하위 퀘스트 함께 지우기 등)은 md. */
		size?: 'sm' | 'md';
		/** 하는 일 단추의 `data-testid`. */
		testid?: string;
		/** 글 아래에 더 보일 것 — 경고 줄 · 파일 경로 등. */
		children?: Snippet;
		onconfirm?: () => void;
		oncancel?: () => void;
	};

	let {
		open,
		title,
		message,
		confirmLabel,
		cancelLabel,
		danger = false,
		busy = false,
		z,
		size = 'sm',
		testid,
		children,
		onconfirm,
		oncancel
	}: Props = $props();

	const uid = $props.id();
	const displayTitle = $derived(title ?? t('common.confirm', $locale));
	const displayConfirm = $derived(confirmLabel ?? t('common.confirm', $locale));
	const displayCancel = $derived(cancelLabel ?? t('common.cancel', $locale));

	// BUG-272: Esc 는 Modal(modalFocus)이 받아 닫고 밖으로 올려 보내지 않는다. Enter 는 포커스된 단추가 스스로 누른다.
	const close = () => oncancel?.();
</script>

{#if open}
	<Modal
		title={displayTitle}
		role="alertdialog"
		{size}
		closeButton={false}
		{busy}
		{z}
		describedby="{uid}-msg"
		onclose={close}
	>
		<div class="confirm-body" id="{uid}-msg">
			{#if message}<p class="confirm-msg">{message}</p>{/if}
			{@render children?.()}
		</div>
		{#snippet actions()}
			<ActionButtons
				label={displayConfirm}
				cancelLabel={displayCancel}
				kind={danger ? 'danger' : 'primary'}
				autofocus={danger ? 'cancel' : 'confirm'}
				{busy}
				{testid}
				onconfirm={() => onconfirm?.()}
				oncancel={close}
			/>
		{/snippet}
	</Modal>
{/if}

<style>
	.confirm-body {
		display: flex;
		flex-direction: column;
		gap: var(--sp-8);
	}
	.confirm-body :global(p) {
		margin: 0;
	}
	.confirm-msg {
		margin: 0;
		font-size: var(--fs-md);
		color: var(--text);
		white-space: pre-wrap;
		word-break: break-word;
	}
</style>
