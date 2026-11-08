<!--
  DEV-440: 취소 · 확인 단추 한 쌍 — 순서를 여기 한 곳에서 정한다.

  **취소가 왼쪽, 하는 일(저장 · 만들기 · 삭제 …)이 오른쪽.** 화면마다 따로 쓰던 시절에는 확인 창(ConfirmDialog ·
  퀘스트 삭제)은 취소가 왼쪽, 보드 상태 바꾸기 · 만들기 창 · 편집 뒤 저장 줄은 저장이 왼쪽이라 같은 앱에서 손이
  엇갈렸다. 단추만 그리고 감싸는 줄(정렬 · 간격)은 부르는 쪽이 정한다 — 줄 모양은 화면마다 그대로 둔다.
-->
<script lang="ts">
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
		/** 시험이 하는 일 단추를 찾는 이름(`data-testid`). */
		testid?: string;
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
		testid
	}: Props = $props();

	const base = $derived(buttonClass ?? 'btn');
</script>

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
