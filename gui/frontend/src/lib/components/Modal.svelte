<!--
  DEV-440 후속: 모달 틀 하나 — 뒤 막 · 상자 · 제목 · 닫기 X · Esc · 포커스 · 스크롤 잠금 · 단추 줄.

  예전에는 창마다(확인 창 · 새 퀘스트 · 새 캠페인 · 관리의 추가 · 삭제 · 퀘스트 삭제 · 타입 바꾸기 · 연관 고르기 ·
  환영 화면 · 보드 확인 …) 막 · 상자 여백 · 제목 크기 · X 모양을 따로 적어 조금씩 달랐다(X 만 세 가지).
  여기서 한 번 정하고, 창은 내용과 단추만 넘긴다.

  - **위 정렬 + 상자의 auto 여백**(BUG-199): 짧으면 가운데처럼 보이고, 화면보다 길면 위부터 보이며 막이 스크롤을
    맡는다 — 가운데 정렬이면 넘칠 때 제목 · X 가 잘려 손댈 수 없다.
  - **X 는 DOM 의 맨 뒤**에 두고 자리만 오른쪽 위로 옮긴다 — `modalFocus` 가 첫 포커스를 X 에 주지 않게(입력칸 ·
    `data-autofocus` 단추가 먼저).
  - 막을 누르면 닫는다(`closeOnBackdrop`). 일하는 중(`busy`)에는 막 · Esc · X 로 닫히지 않는다.
-->
<script lang="ts">
	import type { Snippet } from 'svelte';
	import Icon from '$lib/components/Icon.svelte';
	import { modalScrollLock } from '$lib/actions/modal-scroll-lock';
	import { modalFocus } from '$lib/actions/modal-focus';
	import { t, locale } from '$lib/stores/locale';

	type Props = {
		/** 제목. 없으면 제목 줄을 그리지 않는다(내용이 바로 시작). */
		title?: string;
		onclose: () => void;
		/** 확인을 묻는 창이면 `alertdialog`. */
		role?: 'dialog' | 'alertdialog';
		/** 상자 최대 폭 — sm 26.25rem(확인 창) · md 30rem · lg 40rem(만들기 창 · 보드 설정) · xl 56rem(고르기 · 충돌). */
		size?: 'sm' | 'md' | 'lg' | 'xl';
		/** 오른쪽 위 닫기 X. 확인 창처럼 단추 둘로 끝나는 창은 끈다. */
		closeButton?: boolean;
		closeOnBackdrop?: boolean;
		/** 일하는 중 — 막 · Esc · X 로 닫지 않는다. */
		busy?: boolean;
		/** 막의 겹침 순서. 창이 놓이는 화면에 따라 다르다(보드 위 500 등). */
		z?: number;
		/** 제목 아래 글을 스크린리더가 함께 읽게 할 요소 id. */
		describedby?: string;
		/** 상자에 더 붙일 클래스(창마다의 안쪽 배치). */
		boxClass?: string;
		children: Snippet;
		/** 아래 단추 줄 — 보통 `ActionButtons`. 오른쪽 끝에 붙는다. */
		actions?: Snippet;
	};

	let {
		title,
		onclose,
		role = 'dialog',
		size = 'md',
		closeButton = true,
		closeOnBackdrop = true,
		busy = false,
		z = 200,
		describedby,
		boxClass,
		children,
		actions
	}: Props = $props();

	const uid = $props.id();

	function tryClose() {
		if (!busy) onclose();
	}
</script>

<div
	class="modal-ov"
	style:z-index={z}
	use:modalScrollLock
	role="presentation"
	onclick={(e) => {
		if (closeOnBackdrop && e.target === e.currentTarget) tryClose();
	}}
>
	<div
		class="modal-box modal-frame {size} {boxClass ?? ''}"
		{role}
		aria-modal="true"
		aria-labelledby={title ? `${uid}-title` : undefined}
		aria-describedby={describedby}
		tabindex="-1"
		use:modalFocus={{ onEscape: tryClose }}
	>
		{#if title}
			<h3 class="modal-frame-title" class:with-x={closeButton} id="{uid}-title">{title}</h3>
		{/if}
		{@render children()}
		{#if actions}
			<div class="modal-frame-actions">{@render actions()}</div>
		{/if}
		{#if closeButton}
			<button
				type="button"
				class="modal-frame-x"
				onclick={tryClose}
				disabled={busy}
				aria-label={t('common.close', $locale)}
				title={t('common.close', $locale)}><Icon name="close" size="1em" /></button
			>
		{/if}
	</div>
</div>

<style>
	.modal-ov {
		position: fixed;
		inset: 0;
		background: var(--backdrop);
		display: flex;
		align-items: flex-start;
		justify-content: center;
		overflow-y: auto;
		overscroll-behavior: contain;
		padding: var(--sp-16);
	}
	.modal-frame {
		position: relative;
		margin: auto;
		width: 100%;
		padding: var(--sp-16) var(--sp-24);
		animation: modal-in var(--dur-mid) cubic-bezier(0.34, 1.3, 0.64, 1) forwards;
		transform-origin: top center;
	}
	/* BUG-064: 폭은 rem × 팝업 배율 — UI 크기와 함께 커진다. */
	.modal-frame.sm {
		max-width: calc(26.25rem * var(--popup-scale, 1));
	}
	.modal-frame.md {
		max-width: calc(30rem * var(--popup-scale, 1));
	}
	.modal-frame.lg {
		max-width: min(calc(40rem * var(--popup-scale, 1)), 92vw);
	}
	.modal-frame.xl {
		max-width: min(calc(56rem * var(--popup-scale, 1)), 92vw);
	}
	.modal-frame-title {
		margin: 0 0 var(--sp-12);
		font-size: var(--fs-lg);
		font-weight: var(--fw-semibold);
		color: var(--text-strong);
		overflow-wrap: anywhere;
	}
	/* 오른쪽 위 X 자리만큼 비운다. */
	.modal-frame-title.with-x {
		padding-right: var(--sp-24);
	}
	.modal-frame-actions {
		display: flex;
		justify-content: flex-end;
		align-items: center;
		gap: var(--sp-8);
		margin-top: var(--sp-16);
	}
	.modal-frame-x {
		position: absolute;
		top: var(--sp-12);
		right: var(--sp-16);
		display: inline-flex;
		align-items: center;
		justify-content: center;
		padding: var(--sp-4);
		background: none;
		border: none;
		border-radius: var(--r-sm);
		color: var(--text-faint);
		font-size: 1.25rem; /* token-ok: × 글리프 크기 — 제목 줄과 높이를 맞춘다 */
		line-height: var(--lh-tight);
		cursor: pointer;
		transition:
			color var(--dur-fast),
			background var(--dur-fast);
	}
	.modal-frame-x:hover:not(:disabled) {
		color: var(--text);
		background: var(--bg-subtle);
	}
	.modal-frame-x:focus-visible {
		outline: none;
		box-shadow: var(--ring);
	}
	@keyframes modal-in {
		from {
			opacity: 0;
			transform: translateY(-0.5rem) scale(0.98);
		}
		to {
			opacity: 1;
			transform: none;
		}
	}
</style>
