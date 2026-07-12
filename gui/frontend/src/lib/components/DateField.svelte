<!--
  DEV-205: 언어 반응 날짜 입력.

  배경: 크로미움/WebView2 의 네이티브 <input type="date"> 표기 형식은
  navigator.language(OS 로케일)만 따르고 lang 속성/앱 언어를 무시한다 —
  앱을 영어로 바꿔도 '년-월-일' 로 보이는 문제(사용자 보고).

  해결: 값은 앱 전역과 동일한 ISO(YYYY-MM-DD) 텍스트로 직접 보여주고
  (언어 무관·명확), 달력은 📅 버튼이 숨긴 네이티브 date input 의
  showPicker()(사용자 제스처) 로 띄운다. 두 입력이 같은 value 를 공유한다.
-->
<script lang="ts">
	import { locale, t } from '$lib/stores/locale';

	let {
		value = $bindable(''),
		disabled = false,
		ariaLabel = ''
	}: { value?: string; disabled?: boolean; ariaLabel?: string } = $props();

	let nativeEl = $state<HTMLInputElement | null>(null);

	function openPicker() {
		if (disabled) return;
		try {
			nativeEl?.showPicker?.();
		} catch {
			// 제스처 밖/미지원 — 무시. 텍스트로 직접 입력 가능.
		}
	}
</script>

<span class="datefield" class:disabled>
	<input
		class="df-text"
		type="text"
		inputmode="numeric"
		placeholder="YYYY-MM-DD"
		pattern={'\\d{4}-\\d{2}-\\d{2}'}
		bind:value
		{disabled}
		aria-label={ariaLabel || t('common.pickDate', $locale)}
	/>
	<button
		type="button"
		class="df-btn"
		onclick={openPicker}
		{disabled}
		aria-label={t('common.pickDate', $locale)}
		title={t('common.pickDate', $locale)}>📅</button
	>
	<!-- 달력 UI 전용 — 시각적으로 숨기되 showPicker 를 위해 렌더는 유지. -->
	<input class="df-native" type="date" bind:this={nativeEl} bind:value tabindex="-1" aria-hidden="true" />
</span>

<style>
	.datefield {
		display: inline-flex;
		align-items: center;
		gap: 2px;
		position: relative;
	}
	.df-text {
		width: 6.6rem;
		padding: 0.2rem 0.4rem;
		font-size: 0.8rem;
		font-family: 'JetBrains Mono', ui-monospace, monospace;
		background: var(--bg);
		border: 1px solid var(--border);
		border-radius: 5px;
		color: var(--text);
	}
	.df-text:focus {
		outline: none;
		border-color: var(--accent);
	}
	.df-btn {
		display: inline-flex;
		align-items: center;
		justify-content: center;
		width: 1.5rem;
		height: 1.5rem;
		padding: 0;
		font-size: 0.85rem;
		background: var(--bg-subtle);
		border: 1px solid var(--border);
		border-radius: 5px;
		cursor: pointer;
	}
	.df-btn:hover:not(:disabled) {
		background: var(--border);
	}
	.datefield.disabled,
	.df-btn:disabled {
		opacity: 0.5;
		cursor: not-allowed;
	}
	/* 네이티브 date 입력은 달력 트리거용으로만 — 화면에서 숨김(display:none 은
	   showPicker 를 막으므로 크기 0 + opacity 0). */
	.df-native {
		position: absolute;
		right: 0;
		bottom: 0;
		width: 1px;
		height: 1px;
		opacity: 0;
		pointer-events: none;
	}
</style>
