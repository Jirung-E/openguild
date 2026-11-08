<!--
  DEV-440: 취소 · 확인 단추 한 쌍 — 순서를 여기 한 곳에서 정한다.

  **취소가 왼쪽, 하는 일(저장 · 만들기 · 삭제 …)이 오른쪽.** 화면마다 따로 쓰던 시절에는 확인 창(ConfirmDialog ·
  퀘스트 삭제)은 취소가 왼쪽, 보드 상태 바꾸기 · 만들기 창 · 편집 뒤 저장 줄은 저장이 왼쪽이라 같은 앱에서 손이
  엇갈렸다.

  **줄은 오른쪽 끝에 붙는다**(admin: "퀘스트 생성창이랑 편집의 저장/취소는 왜 왼쪽 정렬임?") — 확인 창과 같은
  자리. 바깥 줄이 flex 든 block 이든 `margin-left: auto` + 오른쪽 정렬로 끝에 간다. 같은 줄에 둘 다른 단추
  (템플릿 저장 등)는 `extra` 로 넘기면 왼쪽 끝에 놓인다.
-->
<script lang="ts">
	import type { Snippet } from 'svelte';
	import { t, locale } from '$lib/stores/locale';

	type Props = {
		/** 하는 일 단추의 글. */
		label: string;
		onconfirm?: () => void;
		oncancel: () => void;
		/** 하는 일 단추의 색 — `primary`(초록) · `danger`(빨강) · `accent`(파랑) · `plain`(테두리만). */
		kind?: 'primary' | 'danger' | 'accent' | 'plain';
		/** 취소 단추의 글 — 없으면 "취소". */
		cancelLabel?: string;
		/** 하는 일 단추를 끈다(빈 제목 등). 취소는 그대로 둔다. */
		disabled?: boolean;
		/** 일하는 중 — 두 단추를 다 끈다. */
		busy?: boolean;
		/** 하는 일 단추를 폼 제출로(`onconfirm` 대신 폼의 submit 이 받는다). */
		submit?: boolean;
		/** 처음 포커스를 어느 단추에 둘지(모달 안에서 `modalFocus` 가 본다). 지우기처럼 위험하면 `cancel`. */
		autofocus?: 'confirm' | 'cancel';
		title?: string;
		/** 단추 모양을 화면이 따로 정할 때(관리 표 · 설정의 작은 단추). 주면 `btn` 클래스 대신 쓴다. */
		buttonClass?: string;
		/** `buttonClass` 와 함께 — 하는 일 단추에만 더 붙일 클래스(관리 표의 `save` 처럼 화면이 칠하는 것). */
		primaryClass?: string;
		/** 작은 단추(`btn-sm`) — 설정의 작은 줄 등. */
		small?: boolean;
		/** 시험이 하는 일 단추를 찾는 이름(`data-testid`). */
		testid?: string;
		/** 같은 줄 왼쪽 끝에 둘 다른 단추들. */
		extra?: Snippet;
	};

	let {
		label,
		onconfirm,
		oncancel,
		kind = 'primary',
		cancelLabel,
		disabled = false,
		busy = false,
		submit = false,
		autofocus,
		title,
		buttonClass,
		primaryClass,
		small = false,
		testid,
		extra
	}: Props = $props();

	const base = $derived(buttonClass ?? (small ? 'btn btn-sm' : 'btn'));
</script>

{#if extra}<div class="action-extra">{@render extra()}</div>{/if}
<div class="action-buttons">
	<button
		type="button"
		class={base}
		data-autofocus={autofocus === 'cancel' ? '' : undefined}
		onclick={oncancel}
		disabled={busy}>{cancelLabel ?? t('common.cancel', $locale)}</button
	>
	<button
		type={submit ? 'submit' : 'button'}
		class={primaryClass ? `${base} ${primaryClass}` : base}
		class:btn-primary={buttonClass === undefined && kind === 'primary'}
		class:btn-danger={buttonClass === undefined && kind === 'danger'}
		class:btn-accent={buttonClass === undefined && kind === 'accent'}
		data-autofocus={autofocus === 'confirm' ? '' : undefined}
		data-testid={testid}
		{title}
		onclick={submit ? undefined : onconfirm}
		disabled={busy || disabled}>{label}</button
	>
</div>

<style>
	/* 바깥 줄이 flex 면 auto 여백이 오른쪽 끝으로 밀고, block 이면 이 줄이 한 줄을 다 쓰며 오른쪽으로 모은다. */
	.action-buttons {
		display: flex;
		justify-content: flex-end;
		align-items: center;
		gap: var(--sp-8);
		margin-left: auto;
	}
	.action-extra {
		display: flex;
		align-items: center;
		gap: var(--sp-8);
	}
</style>
