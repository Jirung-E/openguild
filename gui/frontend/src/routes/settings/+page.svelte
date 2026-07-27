<!--
  DEV-084: 설정 페이지.

  좌측 세로 서브탭 (정보 / 업데이트 / 추후 항목) + 우측 패널. 자주 안 쓰는
  비-주요 기능 묶음 — 상단 nav 의 ⚙ 아이콘으로 진입.

  - 정보: 앱 이름 / 버전 / 저장소 링크.
  - 업데이트: 수동 체크 버튼만 — 결과 표시는 전역 알림 호스트(ToastHost,
    우하단 통합 스택, DEV-259)가 담당. Tauri 전용.
-->
<script lang="ts">
	import { onMount } from 'svelte';
	import { detectEnvironment } from '$lib/api/transport';
	import { updateState, checkForUpdate } from '$lib/api/updater';
	import {
		uiScale,
		setUiScale,
		resetUiScale,
		beginUiScaleDrag,
		endUiScaleDrag,
		MIN_SCALE,
		MAX_SCALE,
		DEFAULT_SCALE
	} from '$lib/stores/uiScale';
	import {
		contentWidth,
		setContentWidth,
		resetContentWidth,
		MIN_CONTENT_WIDTH,
		MAX_CONTENT_WIDTH,
		isFullWidth,
		DEFAULT_CONTENT_WIDTH
	} from '$lib/stores/contentWidth';
	import { theme, setTheme, type ThemeChoice, type EffectiveTheme } from '$lib/stores/theme';
	// DEV-114: 커스텀 테마 — 프리셋 저장/활성화 + 토큰 color picker.
	import {
		TOKEN_CATALOG,
		tokenLabel,
		customThemes,
		activeCustomTheme,
		activatePreset,
		deactivateCustom,
		savePreset,
		deletePreset,
		setActiveOverride,
		clearActiveOverride,
		exportPresetsJson,
		importPresetsJson,
		computedTokenValue
	} from '$lib/stores/customThemes';
	// DEV-114: export/import 결과 안내 — 앱 공용 toast. 삭제 확인은 인앱 모달
	// (no-native-dialogs 규칙 — confirm() 금지).
	import { showToast } from '$lib/stores/toast';
	import ConfirmDialog from '$lib/components/ConfirmDialog.svelte';
	// DEV-015 (MVP): 언어 토글 — 설정 페이지에도 노출.
	import { locale, setLocale, t, type Locale } from '$lib/stores/locale';
	// DEV-130: 편집기 들여쓰기 설정 (tab/space + 2/4칸).
	import {
		editorSettings,
		setTabMode,
		setIndentSize,
		type IndentSize
	} from '$lib/stores/editorSettings';
	// DEV-101 fix2: native input[type=range] 의 drag 문제 (값 재바인딩 →
	// thumb 튐, UI scale 의 자기 자신 변형 → 손 놓침) 회피한 델타 기반 슬라이더.
	import CustomSlider from '$lib/components/CustomSlider.svelte';
	// DEV-113: 원격 서버 모드 — 연결/해제는 Welcome 화면에서, 여기서는 상태만 읽음.
	// BUG-099: isRemoteSessionActive 도 — remoteServerUrl 만 보면 이전 세션의
	// 잔존 값과 "이번 세션에 실제로 연결함"을 구분 못 함(BUG-095 와 동일 이유).
	import { remoteServerUrl, isRemoteSessionActive } from '$lib/stores/remoteServer';
	// DEV-207 후속(사용자 보고: "길드를 열었다가 welcome으로 돌아가서
	// 확인했을때" 여전히 길드가 열려있는 것처럼 표시됨): launch_mode 는
	// Welcome 재방문으로 안 풀리는 Rust 상태라 stale 할 수 있다 —
	// 보드 마운트/Welcome 마운트가 갱신하는 세션 플래그로 보강.
	import { isGuildContextActive } from '$lib/stores/guildSession';

	// DEV-101 fix3: 즉시 반영 — store 가 source of truth, drag 중에도 매 step 적용.
	// preview / displayScale wrapper 제거.

	// DEV-052 / DEV-101 fix6: 탭 분리 — '정보' / '표시'.
	// DEV-207 후속: '원격 서버' 전용 탭은 폐기 — '정보' 탭에 길드 위치 한
	// 줄로 통합(사용자 피드백: "그냥 정보 탭에 원격이면 주소, 로컬이면 경로
	// 출력해서 보여주고, 웰컴페이지에서 들어간거면 표시 안하면 되는거 아님?").
	type Tab = 'info' | 'display' | 'editor';
	let activeTab = $state<Tab>('info');

	// ─── DEV-114: 커스텀 테마 편집기 ───
	// 편집 대상 = 활성 프리셋. 새 프리셋 생성 → 즉시 활성화 → picker 로 조정.
	let showAdvancedTokens = $state(false);
	let creatingPreset = $state(false);
	let newPresetName = $state('');
	let newPresetBase = $state<EffectiveTheme>('dark');
	let presetError = $state<string | null>(null);
	// picker 초기값 재계산 트리거 — override 변경/활성 전환 시 bump.
	let pickerVersion = $state(0);

	const activePreset = $derived(
		$activeCustomTheme ? ($customThemes.find((p) => p.name === $activeCustomTheme) ?? null) : null
	);

	function submitCreatePreset() {
		const name = newPresetName.trim();
		if (!name) {
			presetError = t('settings.presetNameRequired', $locale);
			return;
		}
		if ($customThemes.some((p) => p.name === name)) {
			presetError = t('settings.presetNameExists', $locale);
			return;
		}
		savePreset({ name, base: newPresetBase, overrides: {} });
		activatePreset(name);
		creatingPreset = false;
		newPresetName = '';
		presetError = null;
		pickerVersion++;
	}

	function pickBaseTheme(opt: ThemeChoice) {
		// 커스텀 활성 중 dark/light/system 클릭 → 커스텀 해제 후 기본 테마로.
		if ($activeCustomTheme) deactivateCustom();
		setTheme(opt);
		pickerVersion++;
	}

	function pickCustomPreset(name: string) {
		activatePreset(name);
		pickerVersion++;
	}

	function onTokenInput(token: string, e: Event) {
		const v = (e.currentTarget as HTMLInputElement).value;
		setActiveOverride(token, v);
	}

	function resetToken(token: string) {
		clearActiveOverride(token);
		pickerVersion++;
	}

	async function exportPresets() {
		try {
			await navigator.clipboard.writeText(exportPresetsJson());
			showToast(t('settings.copiedJson', $locale), 'success');
		} catch {
			showToast(t('settings.copyFailed', $locale), 'error');
		}
	}

	let importText = $state('');
	let importing = $state(false);
	function importPresets() {
		try {
			const n = importPresetsJson(importText);
			showToast(`${t('settings.importedPresetsPre', $locale)}${n}${t('settings.importedPresetsPost', $locale)}`, 'success');
			importText = '';
			importing = false;
		} catch (e) {
			showToast(e instanceof Error ? e.message : t('settings.jsonError', $locale), 'error');
		}
	}

	// 삭제 확인 (no-native-dialogs — confirm() 금지).
	let confirmDeletePresetName = $state<string | null>(null);
	function doDeletePreset() {
		const name = confirmDeletePresetName;
		confirmDeletePresetName = null;
		if (!name) return;
		deletePreset(name);
		pickerVersion++;
	}

	const isTauri = detectEnvironment() === 'tauri';

	// 앱 메타 — Tauri 에서는 실제 버전 (tauri.conf.json), 브라우저는 placeholder.
	let appVersion = $state('—');
	let appName = $state('openguild');
	const repoUrl = 'https://github.com/Jirung-E/openguild';

	// BUG-099(사용자 보고: "Welcome 에서 설정으로 바로 들어가면 '원격 서버'
	// 탭에 아직 아무 길드도 안 열렸는데 '로컬'로 잘못 표시됨"): "현재 모드"가
	// remoteServerUrl 존재 여부만 봐서, 길드를 하나도 안 연 상태(Welcome)에서
	// 도 "로컬 (이 PC 의 길드 파일 직접 사용)"이라고 잘못 표시했다. 실제로
	// 길드가 열려있는지(로컬 launch_mode === 'guild' 또는 원격 활성)까지
	// 함께 봐야 정확함 — Welcome 에서 들어온 거면(anyGuildOpen=false) 이
	// 줄 자체를 표시 안 함(DEV-207).
	let localGuildOpen = $state(false);
	// DEV-207: 로컬일 때 보여줄 실제 길드 경로.
	let guildPath = $state<string | null>(null);

	onMount(async () => {
		if (!isTauri) {
			appVersion = t('settings.devBrowser', $locale);
			return;
		}
		try {
			const { getVersion, getName } = await import('@tauri-apps/api/app');
			appVersion = await getVersion();
			appName = await getName();
		} catch {
			appVersion = t('settings.unknown', $locale);
		}
		try {
			const { invoke } = await import('@tauri-apps/api/core');
			const info = await invoke<{ mode: string }>('launch_mode');
			localGuildOpen = info.mode === 'guild';
			if (localGuildOpen) {
				guildPath = await invoke<string>('current_guild_path');
			}
		} catch {
			/* 알 수 없으면 false 유지(anyGuildOpen 이 원격 여부로 보완). */
		}
	});

	// 원격이 "이번 세션에 실제로" 활성인지(BUG-095 의 board guard 와 동일 기준).
	const isRemoteActive = $derived(!!$remoteServerUrl && isRemoteSessionActive());
	// DEV-207 후속: localGuildOpen/isRemoteActive 만으로는 Welcome 재방문 후
	// stale 상태를 못 거른다 — isGuildContextActive() 로 "보드가 마지막으로
	// bounce 없이 마운트됐는지"까지 함께 확인.
	const anyGuildOpen = $derived(isGuildContextActive() && (localGuildOpen || isRemoteActive));
</script>

<div class="settings">
	<aside class="side">
		<h1>{t('settings.title', $locale)}</h1>
		<nav>
			<!-- DEV-052 / DEV-101 fix6: 탭 분리 — '정보' / '표시'. -->
			<button
				class="tab"
				class:active={activeTab === 'info'}
				onclick={() => (activeTab = 'info')}
				aria-pressed={activeTab === 'info'}>{t('settings.tabInfo', $locale)}</button
			>
			<button
				class="tab"
				class:active={activeTab === 'display'}
				onclick={() => (activeTab = 'display')}
				aria-pressed={activeTab === 'display'}>{t('settings.tabDisplay', $locale)}</button
			>
			<!-- DEV-130: 편집기 들여쓰기 설정. -->
			<button
				class="tab"
				class:active={activeTab === 'editor'}
				onclick={() => (activeTab = 'editor')}
				aria-pressed={activeTab === 'editor'}>{t('settings.tabEditor', $locale)}</button
			>
		</nav>
	</aside>

	<section class="panel">
		{#if activeTab === 'editor'}
			<!-- DEV-130: 본문 편집기 들여쓰기 — 코드 편집기처럼 tab/space + 칸수 선택. -->
			<h2>{t('settings.editorHeading', $locale)}</h2>
			<dl class="info-grid">
				<dt>{t('settings.tabBehavior', $locale)}</dt>
				<dd class="theme-row">
					<div class="theme-toggle" role="group" aria-label={t('settings.tabBehavior', $locale)}>
						<button
							class="th-btn"
							class:active={$editorSettings.tabMode === 'tab'}
							onclick={() => setTabMode('tab')}
							aria-pressed={$editorSettings.tabMode === 'tab'}>{t('settings.tabChar', $locale)}</button
						>
						<button
							class="th-btn"
							class:active={$editorSettings.tabMode === 'space'}
							onclick={() => setTabMode('space')}
							aria-pressed={$editorSettings.tabMode === 'space'}>{t('settings.tabSpace', $locale)}</button
						>
					</div>
					<p class="scale-hint">
						{t('settings.tabHint', $locale)}
					</p>
				</dd>
				<dt>{t('settings.indentSize', $locale)}</dt>
				<dd class="theme-row">
					<div class="theme-toggle" role="group" aria-label={t('settings.indentSize', $locale)}>
						{#each [2, 4] as n (n)}
							<button
								class="th-btn"
								class:active={$editorSettings.indentSize === n}
								onclick={() => setIndentSize(n as IndentSize)}
								aria-pressed={$editorSettings.indentSize === n}>{n}{t('settings.indentUnit', $locale)}</button
							>
						{/each}
					</div>
					<p class="scale-hint">
						{t('settings.indentHint', $locale)}
					</p>
				</dd>
			</dl>
		{:else if activeTab === 'info'}
			<h2>{t('settings.infoHeading', $locale)}</h2>
			<dl class="info-grid">
				<dt>{t('settings.appName', $locale)}</dt>
				<dd>{appName}</dd>
				<dt>{t('settings.version', $locale)}</dt>
				<dd>
					<span>{appVersion}</span>
					{#if isTauri}
						<!-- DEV-086: 버전 아래 '슬쩍' 업데이트 확인. 결과는 floating toast. -->
						<button
							class="btn-check-upd"
							onclick={() => checkForUpdate()}
							disabled={$updateState.status === 'checking' || $updateState.status === 'downloading'}
						>
							{$updateState.status === 'checking' ? t('settings.checking', $locale) : t('settings.checkUpdate', $locale)}
						</button>
					{/if}
				</dd>
				<!-- DEV-207(사용자 피드백: "정보 탭에 원격이면 주소, 로컬이면 경로
				     출력하고, 웰컴페이지에서 들어간거면 표시 안하면 되는거 아님?"):
				     별도 '원격 서버' 탭 폐기 — 길드가 열려있을 때만(anyGuildOpen) 한
				     줄로 통합 표시. 연결/해제는 여전히 Welcome 화면(로고 클릭)에서만. -->
				{#if isTauri && anyGuildOpen}
					<dt>{isRemoteActive ? t('settings.remoteServer', $locale) : t('settings.guildPath', $locale)}</dt>
					<dd>
						{#if isRemoteActive}
							<span class="remote-active">{$remoteServerUrl}</span>
						{:else}
							<span>{guildPath ?? '—'}</span>
						{/if}
					</dd>
				{/if}
				<dt>{t('settings.storage', $locale)}</dt>
				<dd><a href={repoUrl} target="_blank" rel="noreferrer noopener">{repoUrl}</a></dd>
			</dl>
		{:else}
			<!-- DEV-101: UI 크기 (rem scale) — 슬라이더 변경 시 즉시 반영. -->
			<h2>{t('settings.displayHeading', $locale)}</h2>
			<dl class="info-grid">
				<dt>{t('settings.uiScale', $locale)}</dt>
				<dd class="ui-scale">
					<div class="scale-row">
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
						<!-- DEV-101 fix4: 직접 숫자 입력. % 단위 (50~200). -->
						<div class="num-input">
							<input
								type="number"
								min={Math.round(MIN_SCALE * 100)}
								max={Math.round(MAX_SCALE * 100)}
								step="1"
								value={Math.round($uiScale * 100)}
								oninput={(e) => {
									const n = Number.parseInt(e.currentTarget.value, 10);
									if (Number.isFinite(n)) setUiScale(n / 100);
								}}
								aria-label={t('settings.uiScalePercentAria', $locale)}
							/>
							<span class="unit">%</span>
						</div>
						<button
							class="btn-reset"
							onclick={resetUiScale}
							disabled={$uiScale === DEFAULT_SCALE}
							title={t('settings.resetTo100', $locale)}>{t('settings.reset', $locale)}</button
						>
					</div>
					<p class="scale-hint">
						{t('settings.uiScaleHintPre', $locale)}{Math.round(
							MIN_SCALE * 100
						)}{t('settings.uiScaleHintPost', $locale)}{Math.round(MAX_SCALE * 100)}{t('settings.uiScaleHintTail', $locale)}
					</p>
				</dd>

				<!-- DEV-101 fix2: 컨텐츠 표시 영역 폭 — UI scale 과 별개. -->
				<dt>{t('settings.contentWidth', $locale)}</dt>
				<dd class="ui-scale">
					<div class="scale-row">
						<CustomSlider
							value={$contentWidth}
							min={MIN_CONTENT_WIDTH}
							max={MAX_CONTENT_WIDTH}
							step={5}
							ariaLabel={t('settings.contentWidth', $locale)}
							onChange={setContentWidth}
						/>
						<!-- DEV-101 fix4: 직접 숫자 입력. px 단위.
						     DEV-275: 최대값은 "화면 전체"(폭 제한 없음) — 숫자 대신
						     라벨을 보여줘야 슬라이더 끝의 의미가 드러난다. -->
						{#if isFullWidth($contentWidth)}
							<div class="num-input full-label">
								<span>{t('settings.contentWidthFull', $locale)}</span>
							</div>
						{:else}
							<div class="num-input">
								<input
									type="number"
									min={MIN_CONTENT_WIDTH}
									max={MAX_CONTENT_WIDTH}
									step="5"
									value={$contentWidth}
									oninput={(e) => {
										const n = Number.parseInt(e.currentTarget.value, 10);
										if (Number.isFinite(n)) setContentWidth(n);
									}}
									aria-label={t('settings.contentWidthPxAria', $locale)}
								/>
								<span class="unit">px</span>
							</div>
						{/if}
						<button
							class="btn-reset"
							onclick={resetContentWidth}
							disabled={$contentWidth === DEFAULT_CONTENT_WIDTH}
							title="{DEFAULT_CONTENT_WIDTH}px{t('settings.resetToDefaultPx', $locale)}">{t('settings.reset', $locale)}</button
						>
					</div>
					<p class="scale-hint">
						{t('settings.contentWidthHintPre', $locale)}{MIN_CONTENT_WIDTH}{t('settings.contentWidthHintMid', $locale)}{MAX_CONTENT_WIDTH}{t('settings.contentWidthHintTail', $locale)}
						<!-- DEV-275: 슬라이더 끝 = 폭 제한 해제. -->
						{t('settings.contentWidthFullHint', $locale)}
					</p>
				</dd>

				<!-- DEV-074: 테마 (Dark / Light / System). DEV-114: 커스텀 프리셋도 옆에 노출. -->
				<dt>{t('settings.theme', $locale)}</dt>
				<dd class="theme-row">
					<div class="theme-toggle" role="group" aria-label={t('settings.theme', $locale)}>
						{#each ['dark', 'light', 'system'] as opt (opt)}
							<button
								class="th-btn"
								class:active={!$activeCustomTheme && $theme === opt}
								onclick={() => pickBaseTheme(opt as ThemeChoice)}
								aria-pressed={!$activeCustomTheme && $theme === opt}
							>
								{opt === 'dark' ? t('settings.themeDark', $locale) : opt === 'light' ? t('settings.themeLight', $locale) : t('settings.themeSystem', $locale)}
							</button>
						{/each}
						{#each $customThemes as p (p.name)}
							<button
								class="th-btn custom"
								class:active={$activeCustomTheme === p.name}
								onclick={() => pickCustomPreset(p.name)}
								aria-pressed={$activeCustomTheme === p.name}
								title={`${t('settings.customBasedOn', $locale)}${p.base === 'dark' ? t('settings.themeDark', $locale) : t('settings.themeLight', $locale)}${t('settings.basedSuffix', $locale)}`}
							>
								{p.name}
							</button>
						{/each}
					</div>
					<p class="scale-hint">{t('settings.scaleHintTokens', $locale)}</p>
				</dd>

				<!-- DEV-114: 커스텀 테마 편집기. -->
				<dt>{t('settings.customTheme', $locale)}</dt>
				<dd class="theme-row">
					<div class="ct-actions">
						{#if !creatingPreset}
							<button class="th-btn" onclick={() => (creatingPreset = true)}>{t('settings.newPreset', $locale)}</button>
						{/if}
						{#if $customThemes.length > 0}
							<button class="th-btn" onclick={exportPresets}>{t('settings.exportJson', $locale)}</button>
						{/if}
						{#if !importing}
							<button class="th-btn" onclick={() => (importing = true)}>{t('settings.import', $locale)}</button>
						{/if}
						{#if activePreset}
							<button
								class="th-btn danger"
								onclick={() => (confirmDeletePresetName = activePreset?.name ?? null)}
							>
								{t('settings.deletePresetBtnPrefix', $locale)}{activePreset.name}{t('settings.deletePresetBtnSuffix', $locale)}
							</button>
						{/if}
					</div>

					{#if creatingPreset}
						<div class="ct-create">
							<input
								class="ct-name"
								type="text"
								placeholder={t('settings.presetNamePlaceholder', $locale)}
								bind:value={newPresetName}
								onkeydown={(e) => e.key === 'Enter' && submitCreatePreset()}
							/>
							<div class="theme-toggle" role="group" aria-label={t('settings.basedOnTheme', $locale)}>
								{#each ['dark', 'light'] as b (b)}
									<button
										class="th-btn"
										class:active={newPresetBase === b}
										onclick={() => (newPresetBase = b as EffectiveTheme)}
									>
										{b === 'dark' ? t('settings.darkBased', $locale) : t('settings.lightBased', $locale)}
									</button>
								{/each}
							</div>
							<button class="th-btn" onclick={submitCreatePreset}>{t('settings.createBtn', $locale)}</button>
							<button
								class="th-btn"
								onclick={() => {
									creatingPreset = false;
									presetError = null;
								}}>{t('common.cancel', $locale)}</button
							>
						</div>
						{#if presetError}<p class="ct-error">{presetError}</p>{/if}
					{/if}

					{#if importing}
						<div class="ct-import">
							<textarea
								class="ct-import-text"
								rows="4"
								placeholder={t('settings.importJsonPlaceholder', $locale)}
								bind:value={importText}
							></textarea>
							<div class="ct-actions">
								<button class="th-btn" onclick={importPresets} disabled={!importText.trim()}>
									{t('settings.import', $locale)}
								</button>
								<button
									class="th-btn"
									onclick={() => {
										importing = false;
										importText = '';
									}}>{t('common.cancel', $locale)}</button
								>
							</div>
						</div>
					{/if}

					{#if activePreset}
						<!-- 토큰 color picker — 활성 프리셋 편집(live 적용). -->
						{#key `${activePreset.name}:${pickerVersion}`}
							<div class="ct-tokens">
								{#each TOKEN_CATALOG.filter((d) => showAdvancedTokens || !d.advanced) as d (d.token)}
									<div class="ct-token" class:overridden={!!activePreset.overrides[d.token]}>
										<input
											type="color"
											value={activePreset.overrides[d.token] ?? computedTokenValue(d.token)}
											oninput={(e) => onTokenInput(d.token, e)}
											aria-label={tokenLabel(d, $locale)}
										/>
										<span class="ct-token-label" title={d.token}>{tokenLabel(d, $locale)}</span>
										{#if activePreset.overrides[d.token]}
											<button
												class="ct-reset"
												title={t('settings.resetToDefaultToken', $locale)}
												onclick={() => resetToken(d.token)}>↺</button
											>
										{/if}
									</div>
								{/each}
							</div>
						{/key}
						<label class="ct-advanced">
							<input type="checkbox" bind:checked={showAdvancedTokens} />
							{t('settings.showAdvancedTokens', $locale)}
						</label>
						<p class="scale-hint">
							{t('settings.tokenHint', $locale)}
						</p>
					{:else}
						<p class="scale-hint">
							{t('settings.presetHint', $locale)}
						</p>
					{/if}
				</dd>

				<!-- DEV-015: 언어 토글 — DEV-205 로 앱 전역 적용됨. -->
				<dt>{t('settings.language', $locale)}</dt>
				<dd class="theme-row">
					<div class="theme-toggle" role="group" aria-label={t('settings.language', $locale)}>
						<!-- 언어 이름 자체는 번역 대상이 아님 — 항상 고정 표기. -->
						{#each [{ value: 'ko', label: '한국어' }, { value: 'en', label: 'English' }] as opt (opt.value)}
							<button
								class="th-btn"
								class:active={$locale === opt.value}
								onclick={() => setLocale(opt.value as Locale)}
								aria-pressed={$locale === opt.value}
							>
								{opt.label}
							</button>
						{/each}
					</div>
					<p class="scale-hint">
						{t('settings.languageHint', $locale)}
					</p>
				</dd>
			</dl>
		{/if}
	</section>
</div>

<!-- DEV-114: 프리셋 삭제 확인 — 인앱 모달 (no-native-dialogs). -->
<ConfirmDialog
	open={confirmDeletePresetName !== null}
	title={t('settings.deletePresetTitle', $locale)}
	message={`${t('settings.deletePresetMsgPrefix', $locale)}${confirmDeletePresetName ?? ''}${t('settings.deletePresetMsgSuffix', $locale)}`}
	confirmLabel={t('detail.delete', $locale)}
	danger
	onconfirm={doDeletePreset}
	oncancel={() => (confirmDeletePresetName = null)}
/>

<style>
	.settings {
		display: flex;
		gap: 1.5rem;
		max-width: var(--content-max-width, 900px);
		margin: 0 auto;
		padding: 1.5rem;
	}
	.side {
		flex: 0 0 160px;
	}
	.side h1 {
		font-size: 1.1rem;
		color: var(--text);
		margin: 0 0 1rem;
	}
	.side nav {
		display: flex;
		flex-direction: column;
		gap: 0.25rem;
	}
	.tab {
		text-align: left;
		padding: 0.5rem 0.75rem;
		border: none;
		background: transparent;
		color: var(--text-muted);
		border-radius: 6px;
		font-size: 0.9rem;
		cursor: pointer;
		transition:
			background 0.15s,
			color 0.15s;
	}
	.tab:hover {
		background: var(--bg-elevated);
		color: var(--text);
	}
	.tab.active {
		background: var(--bg-subtle);
		color: var(--text);
		font-weight: 600;
	}

	.panel {
		flex: 1;
		min-width: 0;
	}
	/* DEV-257(사용자 보고): 375px 급 화면에서 160px 고정 탭 열이 화면의 절반
	   가까이를 먹어 설정 내용이 밀렸다. 좁은 화면에서는 탭을 위로 올려 가로
	   한 줄(넘치면 그 줄만 스크롤)로 두고 본문이 전폭을 쓰게 한다. */
	@media (max-width: 640px) {
		.settings {
			flex-direction: column;
			gap: 1rem;
			padding: 1rem;
		}
		.side {
			flex: none;
		}
		.side h1 {
			margin-bottom: 0.5rem;
		}
		.side nav {
			flex-direction: row;
			overflow-x: auto;
			/* 탭이 세로로 눌리지 않게 — 줄바꿈 대신 그 줄만 스크롤. */
			padding-bottom: 0.25rem;
			scrollbar-width: none;
		}
		.side nav::-webkit-scrollbar {
			display: none;
		}
		.tab {
			flex: none;
			white-space: nowrap;
		}
	}
	.panel h2 {
		font-size: 1rem;
		color: var(--text);
		margin: 0 0 1rem;
	}

	.info-grid {
		display: grid;
		grid-template-columns: 6rem 1fr;
		gap: 0.5rem 1rem;
		margin: 0 0 1rem;
		font-size: 0.875rem;
	}
	.info-grid dt {
		color: var(--text-muted);
	}
	.info-grid dd {
		margin: 0;
		color: var(--text);
		display: flex;
		flex-direction: column;
		gap: 0.35rem;
		align-items: flex-start;
	}
	.info-grid a {
		color: var(--accent);
	}

	/* DEV-086: 버전 아래 '슬쩍' 업데이트 확인 — subtle outline 버튼. */
	.btn-check-upd {
		padding: 0.2rem 0.6rem;
		background: transparent;
		border: 1px solid var(--border);
		border-radius: 6px;
		color: var(--text-muted);
		font-size: 0.75rem;
		cursor: pointer;
		transition:
			background 0.15s,
			color 0.15s;
	}
	.btn-check-upd:hover:not(:disabled) {
		background: var(--bg-subtle);
		color: var(--text);
	}
	.btn-check-upd:disabled {
		opacity: 0.5;
		cursor: not-allowed;
	}

	/* DEV-207: '정보' 탭의 원격 서버 주소 강조. */
	.remote-active {
		color: var(--accent);
		font-weight: 500;
	}

	/* DEV-101 fix6: 탭 분리 후 h2.section 구분선 불필요 — 비워둠. */
	.ui-scale .scale-row {
		display: flex;
		align-items: center;
		gap: 0.75rem;
		width: 100%;
		max-width: 24rem;
	}
	/* DEV-101 fix4: 직접 숫자 입력. */
	.ui-scale .num-input {
		display: inline-flex;
		align-items: center;
		gap: 0.25rem;
		padding: 0.15rem 0.4rem;
		background: var(--bg-elevated);
		border: 1px solid var(--border);
		border-radius: 6px;
	}
	.ui-scale .num-input:focus-within {
		border-color: var(--accent);
	}
	.ui-scale .num-input input[type='number'] {
		min-width: 3ch;
		width: 4.5ch;
		background: transparent;
		border: none;
		color: var(--text);
		font: inherit;
		font-size: 0.875rem;
		font-variant-numeric: tabular-nums;
		text-align: right;
		outline: none;
		-moz-appearance: textfield;
		appearance: textfield;
	}
	.ui-scale .num-input input[type='number']::-webkit-inner-spin-button,
	.ui-scale .num-input input[type='number']::-webkit-outer-spin-button {
		-webkit-appearance: none;
		margin: 0;
	}
	.ui-scale .num-input .unit {
		color: var(--text-muted);
		font-size: 0.8rem;
	}
	/* DEV-275: 최대값("전체")일 때 숫자 입력 대신 표시하는 라벨 — 숫자
	   입력칸과 같은 자리/크기를 차지해 슬라이더가 밀리지 않게. */
	.ui-scale .num-input.full-label {
		justify-content: center;
		min-width: 7ch;
		color: var(--accent);
		font-size: 0.85rem;
		font-weight: 600;
	}
	.ui-scale .btn-reset {
		padding: 0.2rem 0.6rem;
		background: transparent;
		border: 1px solid var(--border);
		border-radius: 6px;
		color: var(--text);
		font-size: 0.8rem;
		cursor: pointer;
	}
	.ui-scale .btn-reset:hover:not(:disabled) {
		background: var(--bg-elevated);
		border-color: var(--text-faint);
	}
	.ui-scale .btn-reset:disabled {
		opacity: 0.4;
		cursor: not-allowed;
	}
	.ui-scale .scale-hint {
		font-size: 0.75rem;
		color: var(--text-faint);
		margin: 0.5rem 0 0;
	}

	/* DEV-074: 테마 토글 — segmented (QuestList 의 view-toggle 과 같은 패턴). */
	.theme-toggle {
		display: inline-flex;
		gap: 0;
		background: var(--bg-elevated);
		border: 1px solid var(--border);
		border-radius: 6px;
		padding: 2px;
	}
	.th-btn {
		padding: 4px 12px;
		background: transparent;
		border: none;
		border-radius: 4px;
		color: var(--text-muted);
		font-size: 0.8rem;
		cursor: pointer;
		transition:
			background 0.1s,
			color 0.1s;
	}
	.th-btn:hover {
		color: var(--text);
	}
	.th-btn.active {
		background: var(--bg-subtle);
		color: var(--text);
	}
	/* DEV-114: 커스텀 프리셋 버튼 — 기본 3개와 구분되는 강조색 톤. */
	.th-btn.custom {
		color: var(--accent);
	}
	.th-btn.custom.active {
		background: var(--bg-subtle);
		color: var(--accent);
		font-weight: 600;
	}
	.th-btn.danger {
		color: var(--danger);
	}
	.th-btn:disabled {
		opacity: 0.5;
		cursor: not-allowed;
	}

	/* DEV-114: 커스텀 테마 편집기. */
	.ct-actions {
		display: flex;
		flex-wrap: wrap;
		gap: 0.4rem;
		align-items: center;
	}
	.ct-actions .th-btn {
		border: 1px solid var(--border);
		background: var(--bg-elevated);
	}
	.ct-create {
		display: flex;
		flex-wrap: wrap;
		gap: 0.4rem;
		align-items: center;
		margin-top: 0.5rem;
	}
	.ct-name {
		padding: 0.3rem 0.6rem;
		background: var(--bg);
		border: 1px solid var(--border);
		border-radius: 6px;
		color: var(--text);
		font-size: 0.85rem;
	}
	.ct-error {
		color: var(--danger);
		font-size: 0.8rem;
		margin: 0.3rem 0 0;
	}
	.ct-import {
		margin-top: 0.5rem;
		display: flex;
		flex-direction: column;
		gap: 0.4rem;
	}
	.ct-import-text {
		width: 100%;
		padding: 0.4rem 0.6rem;
		background: var(--bg);
		border: 1px solid var(--border);
		border-radius: 6px;
		color: var(--text);
		font-size: 0.78rem;
		font-family: 'JetBrains Mono', ui-monospace, monospace;
		resize: vertical;
	}
	.ct-tokens {
		display: grid;
		grid-template-columns: repeat(auto-fill, minmax(180px, 1fr));
		gap: 0.35rem 0.75rem;
		margin-top: 0.6rem;
	}
	.ct-token {
		display: flex;
		align-items: center;
		gap: 0.45rem;
		padding: 0.15rem 0.3rem;
		border-radius: 6px;
	}
	.ct-token.overridden {
		background: color-mix(in srgb, var(--accent) 8%, transparent);
	}
	.ct-token input[type='color'] {
		width: 1.6rem;
		height: 1.6rem;
		padding: 0;
		border: 1px solid var(--border);
		border-radius: 4px;
		background: transparent;
		cursor: pointer;
		flex: none;
	}
	.ct-token-label {
		font-size: 0.78rem;
		color: var(--text);
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.ct-reset {
		margin-left: auto;
		border: none;
		background: none;
		color: var(--text-muted);
		cursor: pointer;
		font-size: 0.85rem;
		flex: none;
	}
	.ct-reset:hover {
		color: var(--text);
	}
	.ct-advanced {
		display: inline-flex;
		align-items: center;
		gap: 0.35rem;
		margin-top: 0.5rem;
		font-size: 0.8rem;
		color: var(--text-muted);
		cursor: pointer;
	}
</style>
