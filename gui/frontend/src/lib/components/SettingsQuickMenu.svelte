<!--
  DEV-138: 설정 퀵메뉴 — Nav 의 ⚙ 클릭 시 dropdown.

  자주 쓰는 표시 설정 (테마 / UI 크기 / 컨텐츠 폭) 을 페이지 이동 없이 즉시
  조절. '전체 설정' 으로 /settings 진입. DEV-125 의 standalone 테마 토글
  버튼은 본 메뉴로 흡수.
-->
<script lang="ts">
	import { onMount } from 'svelte';
	import { theme, setTheme, type ThemeChoice } from '$lib/stores/theme';
	// DEV-114: 커스텀 프리셋 — 기본 3개 옆에 노출 + 기본 테마 클릭 시 커스텀 해제.
	import {
		customThemes,
		activeCustomTheme,
		activatePreset,
		deactivateCustom
	} from '$lib/stores/customThemes';
	import {
		uiScale,
		setUiScale,
		beginUiScaleDrag,
		endUiScaleDrag,
		MIN_SCALE,
		MAX_SCALE
	} from '$lib/stores/uiScale';
	import {
		contentWidth,
		setContentWidth,
		MIN_CONTENT_WIDTH,
		MAX_CONTENT_WIDTH,
		isFullWidth
	} from '$lib/stores/contentWidth';
	// DEV-015 (MVP): 언어 토글 — 이 메뉴의 라벨들을 t() 로 전환. 전역 스윕은 후속(DEV-205).
	import { locale, setLocale, t, type Locale } from '$lib/stores/locale';
	import CustomSlider from './CustomSlider.svelte';

	let { onclose }: { onclose: () => void } = $props();

	/**
	 * BUG-322: **타이틀바를 눌러도 안 닫히던 것**(admin).
	 *
	 * 두 가지가 겹쳤다. 타이틀바는 이 메뉴의 투명 오버레이보다 **위에** 있어서(z-index
	 * 1100 대 150) 오버레이가 그 클릭을 못 받는다. 그리고 Tauri 가 넣어 주는 드래그
	 * 스크립트가 타이틀바 위 `mousedown` 에서 `stopImmediatePropagation()` 을 부르고 창
	 * 끌기를 시작해, 클릭 자체가 안 생기기도 한다.
	 *
	 * 그래서 오버레이에 기대지 않고 **capture 단계**에서 직접 받는다 — capture 는
	 * window → document → 대상 순서라 그 스크립트보다 먼저 돌고 막힘의 영향도 안 받는다.
	 * 타이틀바의 ☰ 메뉴([[BUG-155]])와 접힌 메뉴([[BUG-185]])가 이미 쓰는 방식이다.
	 */
	onMount(() => {
		const onDown = (e: MouseEvent) => {
			const t = e.target;
			if (!(t instanceof Element)) return onclose();
			// ⚙ 버튼과 이 메뉴는 같은 자리에 싸여 있다 — 그 안이면 놔둔다(다시 누르는 것은
			// 그 버튼이 알아서 접는다). 투명 오버레이도 그 안이지만 그건 '바깥' 이다.
			if (t.closest('.settings-wrap') && !t.closest('.qm-ov')) return;
			onclose();
		};
		window.addEventListener('mousedown', onDown, { capture: true });
		return () => window.removeEventListener('mousedown', onDown, { capture: true });
	});

	// $derived — $locale 변경 시 라벨이 즉시 다시 계산되어야 (plain const 는 초기값에 고정됨).
	let THEME_OPTIONS = $derived<{ value: ThemeChoice; label: string }[]>([
		{ value: 'system', label: t('settings.theme.system', $locale) },
		{ value: 'light', label: t('settings.theme.light', $locale) },
		{ value: 'dark', label: t('settings.theme.dark', $locale) }
	]);

	// DEV-015 후속(사용자 피드백): 언어 선택 버튼의 라벨(언어 이름 자체)은
	// 현재 선택된 언어로 번역되면 안 됨 — "한국어"/"English" 는 그 언어를
	// 가리키는 고유명사라 항상 같은 표기로 보여야 선택 중인 항목을 혼동 없이
	// 알 수 있다(영어 선택 시 "Korean"으로 바뀌면 원래 한국어 옵션이었는지
	// 헷갈림). t() 로 번역하지 않고 고정 표기.
	const LOCALE_OPTIONS: { value: Locale; label: string }[] = [
		{ value: 'ko', label: '한국어' },
		{ value: 'en', label: 'English' }
	];

	function onkeydown(e: KeyboardEvent) {
		if (e.key === 'Escape') {
			e.preventDefault();
			onclose();
		}
	}
</script>

<svelte:window {onkeydown} />

<!-- 바깥 클릭 닫기 — 투명 오버레이. 타이틀바처럼 이 위에 오는 것은 아래 capture 가 받는다. -->
<div class="qm-ov" role="presentation" onclick={onclose}></div>

<div class="qm" role="menu" aria-label={t('nav.settingsQuickMenu', $locale)}>
	<div class="qm-row">
		<span class="qm-label">{t('settings.theme', $locale)}</span>
		<div class="qm-seg" role="group" aria-label={t('settings.theme', $locale)}>
			{#each THEME_OPTIONS as o (o.value)}
				<button
					class="qm-seg-btn"
					class:active={!$activeCustomTheme && $theme === o.value}
					onclick={() => {
						// DEV-114: 커스텀 활성 중 기본 테마 클릭 → override 해제 후 전환.
						if ($activeCustomTheme) deactivateCustom();
						setTheme(o.value);
					}}>{o.label}</button
				>
			{/each}
			{#each $customThemes as p (p.name)}
				<button
					class="qm-seg-btn qm-custom"
					class:active={$activeCustomTheme === p.name}
					onclick={() => activatePreset(p.name)}>{p.name}</button
				>
			{/each}
		</div>
	</div>

	<!-- DEV-015 (MVP): 언어 토글 — 이 메뉴의 라벨에 즉시 반영, 전역 적용은 후속. -->
	<div class="qm-row">
		<span class="qm-label">{t('settings.language', $locale)}</span>
		<div class="qm-seg" role="group" aria-label={t('settings.language', $locale)}>
			{#each LOCALE_OPTIONS as o (o.value)}
				<button
					class="qm-seg-btn"
					class:active={$locale === o.value}
					onclick={() => setLocale(o.value)}>{o.label}</button
				>
			{/each}
		</div>
	</div>

	<div class="qm-row">
		<span class="qm-label">{t('settings.uiScale', $locale)}</span>
		<div class="qm-slider">
			<CustomSlider
				value={$uiScale}
				min={MIN_SCALE}
				max={MAX_SCALE}
				step={0.01}
				ariaLabel={t('settings.uiScale', $locale)}
				onChange={setUiScale}
				onDragStart={beginUiScaleDrag}
				onDragEnd={endUiScaleDrag}
			/>
			<span class="qm-val">{Math.round($uiScale * 100)}%</span>
		</div>
	</div>

	<div class="qm-row">
		<span class="qm-label">{t('settings.contentWidth', $locale)}</span>
		<div class="qm-slider">
			<CustomSlider
				value={$contentWidth}
				min={MIN_CONTENT_WIDTH}
				max={MAX_CONTENT_WIDTH}
				step={10}
				ariaLabel={t('settings.contentWidth', $locale)}
				onChange={setContentWidth}
			/>
			<!-- DEV-275(사용자 보고): 설정 페이지에는 최대값에서 "전체" 라벨이
			     나오는데 이 퀵메뉴만 `3200px` 숫자를 그대로 보여줘서, 끝까지 밀어도
			     3200 에서 멈춘 것처럼 보였다. 같은 설정의 두 UI 가 다르게 표시되던
			     문제 — 판정은 설정 페이지와 동일한 isFullWidth() 로 통일. -->
			<span class="qm-val">
				{#if isFullWidth($contentWidth)}
					{t('settings.contentWidthFull', $locale)}
				{:else}
					{$contentWidth}px
				{/if}
			</span>
		</div>
	</div>

	<a class="qm-all" href="/settings" onclick={onclose}>{t('settings.all', $locale)}</a>
</div>

<style>
	.qm-ov {
		position: fixed;
		inset: 0;
		z-index: 150;
		background: transparent;
	}
	.qm {
		position: absolute;
		top: calc(100% + 6px);
		right: 0;
		z-index: 151;
		/* DEV-138 fix3: px 고정이면 UI 크기 (rem scale, DEV-101) ↑ 시 글자가
		   삐져나감 — rem 으로 같이 스케일. 화면보다 커지지 않게 안전망. */
		width: 17.5rem;
		max-width: calc(100vw - 2rem);
		padding: 0.75rem;
		background: var(--bg-elevated);
		border: var(--bw) solid var(--border);
		border-radius: var(--r-xl);
		box-shadow: 0 10px 30px var(--shadow);
		display: flex;
		flex-direction: column;
		gap: 0.7rem;
	}
	.qm-row {
		display: flex;
		flex-direction: column;
		gap: 0.3rem;
	}
	.qm-label {
		font-size: 0.72rem;
		color: var(--text-muted);
	}
	.qm-seg {
		display: flex;
		gap: 0.25rem;
	}
	.qm-seg-btn {
		flex: 1;
		min-width: 0; /* DEV-138 fix3: flex 자식이 내용 폭으로 안 밀려나게. */
		padding: 0.3rem 0;
		background: transparent;
		border: var(--bw) solid var(--border);
		border-radius: var(--r-md);
		color: var(--text-muted);
		font-size: 0.78rem;
		cursor: pointer;
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}
	.qm-seg-btn:hover {
		color: var(--text);
		border-color: var(--text-faint);
	}
	.qm-seg-btn.active {
		background: color-mix(in srgb, var(--accent) 15%, transparent);
		border-color: var(--accent);
		color: var(--accent);
	}
	/* DEV-114: 커스텀 프리셋 버튼 — 좁은 메뉴라 최소 폭만 확보, 이름은 ellipsis. */
	.qm-seg-btn.qm-custom {
		padding: 0.3rem 0.35rem;
	}
	.qm-slider {
		display: flex;
		align-items: center;
		gap: 0.5rem;
	}
	.qm-slider :global(.slider) {
		flex: 1;
	}
	.qm-val {
		min-width: 3.2rem;
		flex-shrink: 0;
		white-space: nowrap;
		text-align: right;
		font-size: 0.75rem;
		color: var(--text);
		font-variant-numeric: tabular-nums;
	}
	.qm-all {
		margin-top: 0.1rem;
		padding: 0.4rem 0;
		text-align: center;
		border-top: var(--bw) solid var(--bg-subtle);
		color: var(--accent);
		font-size: 0.8rem;
		text-decoration: none;
	}
	.qm-all:hover {
		text-decoration: underline;
	}
</style>
