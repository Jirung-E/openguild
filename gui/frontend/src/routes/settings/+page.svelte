<!--
  DEV-084: 설정 페이지.

  좌측 세로 서브탭 (정보 / 업데이트 / 추후 항목) + 우측 패널. 자주 안 쓰는
  비-주요 기능 묶음 — 상단 nav 의 ⚙ 아이콘으로 진입.

  - 정보: 앱 이름 / 버전 / 저장소 링크.
  - 업데이트: 수동 체크 버튼만 — 결과 표시는 전역 알림 호스트(ToastHost,
    우하단 통합 스택, DEV-259)가 담당. Tauri 전용.
-->
<script lang="ts">
	import { onMount, tick } from 'svelte';
	import { detectEnvironment } from '$lib/api/transport';
	import { updateState, checkForUpdate } from '$lib/api/updater';
	// DEV-305: 업데이트 자동 확인 on/off.
	import { autoUpdateCheck, setAutoUpdateCheck } from '$lib/stores/updateSettings';
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
	// DEV-335: 첨부 이미지 HDR 표시 제한.
	import { hdrLimit, setHdrLimit, isHdrLimitSupported } from '$lib/stores/hdrSettings';
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
		setAutoFormat,
		type IndentSize
	} from '$lib/stores/editorSettings';
	// DEV-101 fix2: native input[type=range] 의 drag 문제 (값 재바인딩 →
	// thumb 튐, UI scale 의 자기 자신 변형 → 손 놓침) 회피한 델타 기반 슬라이더.
	import CustomSlider from '$lib/components/CustomSlider.svelte';
	// DEV-272: 글꼴 설정.
	import {
		uiFont,
		codeFont,
		setFont,
		UI_FONT_PRESETS,
		CODE_FONT_PRESETS,
		type FontKind
	} from '$lib/stores/fontSettings';
	// DEV-113: 원격 서버 모드 — 연결/해제는 Welcome 화면에서, 여기서는 상태만 읽음.
	// BUG-099: isRemoteSessionActive 도 — remoteServerUrl 만 보면 이전 세션의
	// 잔존 값과 "이번 세션에 실제로 연결함"을 구분 못 함(BUG-095 와 동일 이유).
	import { remoteServerUrl, isRemoteSessionActive } from '$lib/stores/remoteServer';
	// DEV-207 후속(사용자 보고: "길드를 열었다가 welcome으로 돌아가서
	// 확인했을때" 여전히 길드가 열려있는 것처럼 표시됨): launch_mode 는
	// Welcome 재방문으로 안 풀리는 Rust 상태라 stale 할 수 있다 —
	// 보드 마운트/Welcome 마운트가 갱신하는 세션 플래그로 보강.
	import { isGuildContextActive } from '$lib/stores/guildSession';
	// DEV-379/380: 플러그인. 조회는 어디서든(브라우저 포함), 허용/철회는
	// 로컬 길드를 연 데스크톱에서만.
	import { pluginApi, pluginsManageable, type PluginStatus } from '$lib/api/plugins';

	// DEV-101 fix3: 즉시 반영 — store 가 source of truth, drag 중에도 매 step 적용.
	// preview / displayScale wrapper 제거.

	// DEV-052 / DEV-101 fix6: 탭 분리 — '정보' / '표시'.
	// DEV-207 후속: '원격 서버' 전용 탭은 폐기 — '정보' 탭에 길드 위치 한
	// 줄로 통합(사용자 피드백: "그냥 정보 탭에 원격이면 주소, 로컬이면 경로
	// 출력해서 보여주고, 웰컴페이지에서 들어간거면 표시 안하면 되는거 아님?").
	type Tab = 'info' | 'display' | 'editor' | 'plugins';
	let activeTab = $state<Tab>('info');

	// ─── DEV-379: 플러그인 ───
	//
	// 묻는 **시점**을 길드 열 때가 아니라 여기로 잡았다. 열자마자 대화상자로
	// 막으면 플러그인을 안 쓰는 사람까지 방해한다. 대신 안 돌고 있는 것이
	// 목록에서 바로 보인다.
	let pluginStatus = $state<PluginStatus | null>(null);
	let pluginError = $state<string | null>(null);
	// DEV-383: 스칼라 하나로 잠그면 두 번째 클릭이 첫 번째의 잠금을 풀어 버린다.
	let pluginBusy = $state<string[]>([]);
	let openScript = $state<string | null>(null);
	let confirmTrust = $state(false);
	// 관리 가능 여부는 **서버가 답한 값**으로 정한다. 프런트의 환경 감지만
	// 믿으면 원격 길드를 보면서 로컬 동의를 고치게 된다(BUG-255 계열).
	const canManage = $derived(pluginsManageable() && (pluginStatus?.manageable ?? false));
	// 답을 받기 **전에** 판단하면 안 된다 — 로컬 데스크톱에서도 잠깐
	// "조회만 가능합니다" 가 스쳐 지나가고, 실패하면 그 상태로 굳는다.
	const pluginsLoaded = $derived(pluginStatus !== null);

	async function refreshPlugins() {
		try {
			pluginStatus = await pluginApi.status();
			pluginError = null;
		} catch (e) {
			// DEV-383: 직전 목록을 지우지 않는다. 지우면 목록이 통째로 사라지고
			// "다른 기계에서 하세요" 로 바뀌어, 데스크톱 앱 안에서 데스크톱 앱을
			// 쓰라는 화면이 된다.
			pluginError = e instanceof Error ? e.message : String(e);
		}
	}

	// 탭 버튼 onclick 에만 걸어 두면 다른 경로로 들어왔을 때 빈 화면이 된다.
	$effect(() => {
		if (activeTab === 'plugins' && pluginStatus === null && pluginError === null) {
			refreshPlugins();
		}
	});

	// DEV-384: 동작 뒤 목록이 다시 그려지면 눌렀던 버튼이 사라져 포커스가
	// `<body>` 로 떨어진다. 그러면 다음 플러그인까지 앱 맨 위에서부터 Tab 해야
	// 한다. 이름으로 버튼을 기억해 두었다가 되돌린다.
	const pluginBtns: Record<string, HTMLButtonElement | undefined> = {};

	async function togglePlugin(name: string, grant: boolean) {
		pluginBusy = [...pluginBusy, name];
		try {
			await (grant ? pluginApi.allow(name) : pluginApi.revoke(name));
			await refreshPlugins();
			showToast(
				grant
					? t('settings.pluginAllowed', $locale)
					: t('settings.pluginRevoked', $locale),
				'success'
			);
			// 다시 그려진 뒤에 잡아야 새 버튼이 잡힌다.
			await tick();
			pluginBtns[name]?.focus();
		} catch (e) {
			showToast(e instanceof Error ? e.message : String(e), 'error');
		} finally {
			pluginBusy = pluginBusy.filter((n) => n !== name);
		}
	}

	async function setTrusted(on: boolean) {
		confirmTrust = false;
		try {
			await pluginApi.setTrusted(on);
			await refreshPlugins();
		} catch (e) {
			showToast(e instanceof Error ? e.message : String(e), 'error');
		}
	}

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
			showToast(
				`${t('settings.importedPresetsPre', $locale)}${n}${t('settings.importedPresetsPost', $locale)}`,
				'success'
			);
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

	// DEV-335: dynamic-range-limit 지원 여부 — 미지원이면 설정 항목 자체 숨김.
	let hdrSupported = $state(false);
	onMount(() => {
		hdrSupported = isHdrLimitSupported();
	});

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

	// DEV-272: 두 행이 완전히 같은 모양이라 표로 돌린다.
	const FONT_ROWS = [
		{ kind: 'ui' as FontKind, labelKey: 'settings.uiFont', presets: UI_FONT_PRESETS },
		{ kind: 'code' as FontKind, labelKey: 'settings.codeFont', presets: CODE_FONT_PRESETS }
	];
	/** 드롭다운의 '직접 입력' 항목 값. 빈 문자열(시스템 기본)과 겹치면 안 된다. */
	const CUSTOM = '__custom__';
	const FONT_SAMPLE = 'Ag 한글 0O1l';

	/**
	 * '직접 입력' 을 고른 상태. **저장값과 별개로 들고 있어야 한다** — 고른
	 * 순간에는 아직 이름이 없어서 저장값이 그대로이고, 그것만 보면 드롭다운이
	 * 곧바로 원래 항목으로 되돌아가 입력칸이 열리지 않는다.
	 */
	let fontCustomOpen = $state<Record<FontKind, boolean>>({ ui: false, code: false });

	/**
	 * 드롭다운에 표시할 값. 저장값이 목록 밖이면(예전에 직접 입력해 둔 이름)
	 * 열어 둔 적이 없어도 '직접 입력' 으로 보여야 한다.
	 */
	function fontSelectValue(kind: FontKind): string {
		if (fontCustomOpen[kind]) return CUSTOM;
		const cur = kind === 'ui' ? $uiFont : $codeFont;
		const presets = kind === 'ui' ? UI_FONT_PRESETS : CODE_FONT_PRESETS;
		return presets.includes(cur) ? cur : CUSTOM;
	}

	function onFontSelect(kind: FontKind, value: string) {
		if (value === CUSTOM) {
			// 값은 건드리지 않는다 — 여기서 비우면 방금 쓴 이름이 날아간다.
			fontCustomOpen = { ...fontCustomOpen, [kind]: true };
			return;
		}
		fontCustomOpen = { ...fontCustomOpen, [kind]: false };
		setFont(kind, value);
	}
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
			<!-- DEV-379: 플러그인 동의. -->
			<button
				class="tab"
				class:active={activeTab === 'plugins'}
				onclick={() => {
					activeTab = 'plugins';
					refreshPlugins();
				}}
				aria-pressed={activeTab === 'plugins'}>{t('settings.tabPlugins', $locale)}</button
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
							aria-pressed={$editorSettings.tabMode === 'tab'}
							>{t('settings.tabChar', $locale)}</button
						>
						<button
							class="th-btn"
							class:active={$editorSettings.tabMode === 'space'}
							onclick={() => setTabMode('space')}
							aria-pressed={$editorSettings.tabMode === 'space'}
							>{t('settings.tabSpace', $locale)}</button
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
								aria-pressed={$editorSettings.indentSize === n}
								>{n}{t('settings.indentUnit', $locale)}</button
							>
						{/each}
					</div>
					<p class="scale-hint">
						{t('settings.indentHint', $locale)}
					</p>
				</dd>
				<!-- DEV-336: 목록 이어쓰기 / Enter 자동 들여쓰기 / 타이핑 중 재들여쓰기를
				     하나로 묶어 켜고 끔. -->
				<dt>{t('settings.autoFormat', $locale)}</dt>
				<dd class="theme-row">
					<div class="theme-toggle" role="group" aria-label={t('settings.autoFormat', $locale)}>
						<button
							class="th-btn"
							class:active={$editorSettings.autoFormat}
							onclick={() => setAutoFormat(true)}
							aria-pressed={$editorSettings.autoFormat}>{t('settings.on', $locale)}</button
						>
						<button
							class="th-btn"
							class:active={!$editorSettings.autoFormat}
							onclick={() => setAutoFormat(false)}
							aria-pressed={!$editorSettings.autoFormat}>{t('settings.off', $locale)}</button
						>
					</div>
					<p class="scale-hint">
						{t('settings.autoFormatHint', $locale)}
					</p>
				</dd>
			</dl>
		{:else if activeTab === 'plugins'}
			<h2>{t('settings.pluginsHeading', $locale)}</h2>
			<!-- DEV-383: 길드를 안 열었으면 그 사실만 말한다. 예전엔 "플러그인이
			     없습니다. .guild/plugins/… 에 정의하세요" 와 "조회만 가능합니다"
			     가 함께 떴는데, 그 상황에서는 **둘 다 틀린 말**이다. -->
			{#if pluginStatus?.no_guild}
				<p class="scale-hint">{t('settings.pluginsNoGuild', $locale)}</p>
			{:else}
				<!-- DEV-384: 신뢰 중에는 "허용 전에는 안 돕니다" 가 바로 밑의
				     "전부 허용돼 있습니다" 와 정면으로 어긋난다. 위치만 알린다. -->
				<p class="scale-hint">
					{pluginStatus?.trusted
						? t('settings.pluginsIntroTrusted', $locale)
						: t('settings.pluginsIntro', $locale)}
				</p>
				<!-- DEV-380: 조회는 어디서든. 관리 버튼만 로컬 데스크톱에서 나온다.
				     DEV-383: **답을 받은 뒤에만** 판단한다 — 받기 전에 띄우면
				     로컬 데스크톱에서도 잠깐 스쳐 지나가고, 실패하면 굳는다. -->
				{#if pluginsLoaded && !canManage}
					<p class="scale-hint">{t('settings.pluginsReadOnly', $locale)}</p>
				{/if}
			{/if}
			{#if true}
				<!-- DEV-384: 조회 오류와 전달 실패는 **비동기로** 나타난다. 라이브
				     리전이 없으면 스크린리더는 그게 생긴 줄도 모른다. -->
				{#if pluginError}
					<!-- DEV-383: 실패했을 때 다시 시도할 방법이 없으면 탭을 다시
					     누를 생각을 못 한 사람은 그대로 막힌다. -->
					<p class="plugin-error" role="alert">
						{pluginError}
						<button class="btn-plain" onclick={refreshPlugins}
							>{t('settings.pluginRetry', $locale)}</button
						>
					</p>
				{/if}
				{#if pluginStatus?.trusted}
					<p class="plugin-trusted">{t('settings.pluginTrusted', $locale)}</p>
				{/if}
				{#if pluginStatus &&
					!pluginStatus.no_guild &&
					pluginStatus.plugins.length === 0 &&
					pluginStatus.errors.length === 0}
					<p class="scale-hint">{t('settings.pluginsNone', $locale)}</p>
				{/if}
				<ul class="plugin-list" aria-busy={pluginBusy.length > 0}>
					{#each pluginStatus?.plugins ?? [] as p (p.name)}
						<li class="plugin" class:pending={!p.granted}>
							<div class="plugin-head">
								<strong>{p.name}</strong>
								<code class="plugin-target">{p.action} → {p.target}</code>
							</div>
							<!-- REQ-020: 나머지 줄은 전부 기계가 읽는 값이라, 처음 보는
							     플러그인 앞에서 "이게 무슨 일을 하는가" 를 알려주는 것이
							     하나도 없었다. 정의가 안 적었으면 자리도 안 만든다 —
							     빈 줄은 없는 것보다 나쁘다. -->
							{#if p.description}
								<p class="plugin-desc">{p.description}</p>
							{/if}
							<div class="plugin-meta">
								<span>{p.on.join(' ')}</span>
								<span>{t('settings.pluginScope', $locale)}: {p.scope.join(', ')}</span>
							</div>
							<div class="plugin-state">
								{#if !p.granted}
									{t('settings.pluginPending', $locale)}
								{:else if p.runs_here}
									{t('settings.pluginRunning', $locale)}
								{:else}
									{t('settings.pluginOtherScope', $locale)}
								{/if}
							</div>
							{#if p.script_src}
								<!-- DEV-383: 이 화면의 존재 이유가 "무엇에 동의하는지 보여
								     주는 것" 인데, 펼침 상태가 스크린리더에 안 전달되면
								     그 코드는 사실상 안 보이는 것과 같다. -->
								<button
									type="button"
									class="link-btn"
									aria-expanded={openScript === p.name}
									aria-controls={`plugin-script-${p.name}`}
									onclick={() => (openScript = openScript === p.name ? null : p.name)}
									>{openScript === p.name
										? t('settings.pluginHideScript', $locale)
										: t('settings.pluginShowScript', $locale)} — {p.name}</button
								>
								{#if openScript === p.name}
									<pre id={`plugin-script-${p.name}`} class="plugin-script">{p.script_src}</pre>
								{/if}
							{/if}
							{#if canManage}
								<div class="plugin-actions">
									{#if p.granted}
										<!-- 신뢰 중에는 개별 철회가 효과가 없다. 버튼을 그냥
										     흐려 놓으면 왜 못 누르는지 알 수 없다. -->
										<button
											type="button"
											class="btn-plain"
											bind:this={pluginBtns[p.name]}
											disabled={pluginBusy.includes(p.name) || pluginStatus?.trusted}
											title={pluginStatus?.trusted
												? t('settings.pluginRevokeTrustedHint', $locale)
												: undefined}
											onclick={() => togglePlugin(p.name, false)}
											>{t('settings.pluginRevoke', $locale)} — {p.name}</button
										>
									{:else}
										<button
											type="button"
											class="btn-go"
											bind:this={pluginBtns[p.name]}
											disabled={pluginBusy.includes(p.name)}
											onclick={() => togglePlugin(p.name, true)}
											>{t('settings.pluginAllow', $locale)} — {p.name}</button
										>
									{/if}
								</div>
							{/if}
						</li>
					{/each}
				</ul>
				<!-- DEV-381: 전달 실패는 대개 설정 문제라 설정 옆에 있는 편이 낫다.
				     토스트로 띄우면 시끄럽고, 안 보여주면 조용히 실패한다. -->
				{#if (pluginStatus?.problems ?? []).length > 0}
					<h3 class="plugin-broken-h">{t('settings.pluginProblems', $locale)}</h3>
					<ul class="plugin-list" role="status" aria-live="polite">
						{#each pluginStatus?.problems ?? [] as p, i (i)}
							<li class="plugin broken"><span>{p}</span></li>
						{/each}
					</ul>
				{/if}
				{#if (pluginStatus?.errors ?? []).length > 0}
					<h3 class="plugin-broken-h">{t('settings.pluginBroken', $locale)}</h3>
					<ul class="plugin-list">
						{#each pluginStatus?.errors ?? [] as [name, why] (name)}
							<li class="plugin broken"><strong>{name}</strong><span>{why}</span></li>
						{/each}
					</ul>
				{/if}
				{#if canManage}
					{#if pluginStatus?.trusted}
						<!-- DEV-380: 켠 뒤에 끌 수 없으면 개별 철회가 계속 거짓말을 한다. -->
						<button class="btn-plain" onclick={() => setTrusted(false)}
							>{t('settings.pluginUntrust', $locale)}</button
						>
					{:else}
						<button class="btn-plain trust" onclick={() => (confirmTrust = true)}
							>{t('settings.pluginTrust', $locale)}</button
						>
					{/if}
				{/if}
			{/if}
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
							{$updateState.status === 'checking'
								? t('settings.checking', $locale)
								: t('settings.checkUpdate', $locale)}
						</button>
					{/if}
				</dd>
				<!-- DEV-305: 자동 확인 on/off. 끄면 시동/주기 확인을 건너뛴다 —
				     위의 '지금 확인'(수동)은 이 설정과 무관하게 계속 동작한다. -->
				{#if isTauri}
					<dt>{t('settings.autoUpdateCheck', $locale)}</dt>
					<dd class="theme-row">
						<div
							class="theme-toggle"
							role="group"
							aria-label={t('settings.autoUpdateCheck', $locale)}
						>
							<button
								class="th-btn"
								class:active={$autoUpdateCheck}
								onclick={() => setAutoUpdateCheck(true)}
								aria-pressed={$autoUpdateCheck}>{t('settings.on', $locale)}</button
							>
							<button
								class="th-btn"
								class:active={!$autoUpdateCheck}
								onclick={() => setAutoUpdateCheck(false)}
								aria-pressed={!$autoUpdateCheck}>{t('settings.off', $locale)}</button
							>
						</div>
						<span class="hint">{t('settings.autoUpdateCheckHint', $locale)}</span>
					</dd>
				{/if}
				<!-- DEV-207(사용자 피드백: "정보 탭에 원격이면 주소, 로컬이면 경로
				     출력하고, 웰컴페이지에서 들어간거면 표시 안하면 되는거 아님?"):
				     별도 '원격 서버' 탭 폐기 — 길드가 열려있을 때만(anyGuildOpen) 한
				     줄로 통합 표시. 연결/해제는 여전히 Welcome 화면(로고 클릭)에서만. -->
				{#if isTauri && anyGuildOpen}
					<dt>
						{isRemoteActive
							? t('settings.remoteServer', $locale)
							: t('settings.guildPath', $locale)}
					</dt>
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
						{t('settings.uiScaleHintPre', $locale)}{Math.round(MIN_SCALE * 100)}{t(
							'settings.uiScaleHintPost',
							$locale
						)}{Math.round(MAX_SCALE * 100)}{t('settings.uiScaleHintTail', $locale)}
					</p>
				</dd>

				<!-- DEV-272: 글꼴 — UI / 코드 따로. 큐레이션 목록 + 직접 입력.
				     시스템 글꼴 열거는 웹/서버 모드에서 못 쓰므로 채택하지 않았다
				     (자세한 배경은 `stores/fontSettings`). -->
				{#each FONT_ROWS as row (row.kind)}
					<dt>{t(row.labelKey, $locale)}</dt>
					<dd class="ui-scale">
						<div class="scale-row">
							<select
								class="font-select"
								value={fontSelectValue(row.kind)}
								onchange={(e) => onFontSelect(row.kind, e.currentTarget.value)}
								aria-label={t(row.labelKey, $locale)}
							>
								{#each row.presets as f (f)}
									<option value={f}>{f || t('settings.fontSystemDefault', $locale)}</option>
								{/each}
								<option value={CUSTOM}>{t('settings.fontCustom', $locale)}</option>
							</select>
							{#if fontSelectValue(row.kind) === CUSTOM}
								<input
									class="font-custom"
									type="text"
									value={row.kind === 'ui' ? $uiFont : $codeFont}
									placeholder={t('settings.fontCustomPlaceholder', $locale)}
									oninput={(e) => setFont(row.kind, e.currentTarget.value)}
									aria-label={t(row.labelKey, $locale)}
								/>
							{/if}
							<!-- 실제로 그 글꼴이 있는지는 목록으로 알 수 없다 — 눈으로
							     확인하도록 같은 토큰을 쓰는 견본을 옆에 둔다. -->
							<span
								class="font-preview"
								class:mono={row.kind === 'code'}
								title={t('settings.fontPreview', $locale)}>{FONT_SAMPLE}</span
							>
						</div>
					</dd>
				{/each}
				<dt></dt>
				<dd><p class="scale-hint">{t('settings.fontHint', $locale)}</p></dd>

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
							title="{DEFAULT_CONTENT_WIDTH}px{t('settings.resetToDefaultPx', $locale)}"
							>{t('settings.reset', $locale)}</button
						>
					</div>
					<p class="scale-hint">
						{t('settings.contentWidthHintPre', $locale)}{MIN_CONTENT_WIDTH}{t(
							'settings.contentWidthHintMid',
							$locale
						)}{MAX_CONTENT_WIDTH}{t('settings.contentWidthHintTail', $locale)}
						<!-- DEV-275: 슬라이더 끝 = 폭 제한 해제. -->
						{t('settings.contentWidthFullHint', $locale)}
					</p>
				</dd>

				<!-- DEV-335: 첨부 이미지 HDR 표시 제한 — 미지원 브라우저에서는 항목 자체 숨김. -->
				{#if hdrSupported}
					<dt>{t('settings.hdrLimit', $locale)}</dt>
					<dd class="theme-row">
						<div class="theme-toggle" role="group" aria-label={t('settings.hdrLimit', $locale)}>
							<button
								class="th-btn"
								class:active={$hdrLimit === 'no-limit'}
								onclick={() => setHdrLimit('no-limit')}
								aria-pressed={$hdrLimit === 'no-limit'}>{t('settings.hdrLimitOn', $locale)}</button
							>
							<button
								class="th-btn"
								class:active={$hdrLimit === 'constrained'}
								onclick={() => setHdrLimit('constrained')}
								aria-pressed={$hdrLimit === 'constrained'}
								>{t('settings.hdrLimitConstrained', $locale)}</button
							>
							<button
								class="th-btn"
								class:active={$hdrLimit === 'standard'}
								onclick={() => setHdrLimit('standard')}
								aria-pressed={$hdrLimit === 'standard'}
								>{t('settings.hdrLimitOff', $locale)}</button
							>
						</div>
						<p class="scale-hint">
							{t('settings.hdrLimitHint', $locale)}
						</p>
					</dd>
				{/if}

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
								{opt === 'dark'
									? t('settings.themeDark', $locale)
									: opt === 'light'
										? t('settings.themeLight', $locale)
										: t('settings.themeSystem', $locale)}
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
							<button class="th-btn" onclick={() => (creatingPreset = true)}
								>{t('settings.newPreset', $locale)}</button
							>
						{/if}
						{#if $customThemes.length > 0}
							<button class="th-btn" onclick={exportPresets}
								>{t('settings.exportJson', $locale)}</button
							>
						{/if}
						{#if !importing}
							<button class="th-btn" onclick={() => (importing = true)}
								>{t('settings.import', $locale)}</button
							>
						{/if}
						{#if activePreset}
							<button
								class="th-btn danger"
								onclick={() => (confirmDeletePresetName = activePreset?.name ?? null)}
							>
								{t('settings.deletePresetBtnPrefix', $locale)}{activePreset.name}{t(
									'settings.deletePresetBtnSuffix',
									$locale
								)}
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
							<div
								class="theme-toggle"
								role="group"
								aria-label={t('settings.basedOnTheme', $locale)}
							>
								{#each ['dark', 'light'] as b (b)}
									<button
										class="th-btn"
										class:active={newPresetBase === b}
										onclick={() => (newPresetBase = b as EffectiveTheme)}
									>
										{b === 'dark'
											? t('settings.darkBased', $locale)
											: t('settings.lightBased', $locale)}
									</button>
								{/each}
							</div>
							<button class="th-btn" onclick={submitCreatePreset}
								>{t('settings.createBtn', $locale)}</button
							>
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

<!-- DEV-379: 길드 통째 신뢰는 되돌리기 어렵다 — 확인 없이 받지 않는다. -->
<ConfirmDialog
	open={confirmTrust}
	title={t('settings.pluginTrust', $locale)}
	message={t('settings.pluginTrustConfirm', $locale)}
	confirmLabel={t('settings.pluginTrust', $locale)}
	danger
	onconfirm={() => setTrusted(true)}
	oncancel={() => (confirmTrust = false)}
/>

<style>
	/* DEV-379: 플러그인 목록. 동의 전인 것이 눈에 띄어야 한다 — 조용히 안 도는
	   것이 제일 나쁘다. */
	.plugin-list {
		list-style: none;
		margin: 0.75rem 0;
		padding: 0;
		display: flex;
		flex-direction: column;
		gap: 0.5rem;
		/* DEV-383: 설정의 다른 절은 `.info-grid` 안이라 0.875rem 이다. 여기만
		   `.info-grid` 밖이라 1rem 으로 튀었다. */
		font-size: 0.875rem;
	}
	.plugin {
		border: var(--bw) solid var(--border);
		border-radius: var(--r-md);
		padding: 0.6rem 0.75rem;
		display: flex;
		flex-direction: column;
		gap: 0.3rem;
	}
	.plugin.pending {
		border-color: var(--warning);
	}
	.plugin.broken {
		border-color: var(--danger);
		flex-direction: row;
		/* DEV-383: 전달 실패 메시지에 URL 이 들어가면 줄이 카드를 넘어가 잘렸다 —
		   정작 이유가 URL **뒤에** 오는데 그게 안 보였다. */
		flex-wrap: wrap;
		gap: 0.75rem;
		align-items: baseline;
	}
	.plugin.broken span {
		min-width: 0;
		overflow-wrap: anywhere;
	}
	.plugin-head {
		display: flex;
		gap: 0.5rem;
		align-items: baseline;
		flex-wrap: wrap;
	}
	.plugin-target {
		/* DEV-383: 코드 글꼴 설정을 따른다 — 앱의 모든 mono 표면이 그렇다. */
		font-family: var(--font-mono);
		font-size: 0.8rem;
		color: var(--text-muted);
		word-break: break-all;
	}
	.plugin-desc {
		/* 사람이 읽는 유일한 줄이다 — 목적지·패턴보다 눈에 먼저 들어와야 하므로
		   muted 로 죽이지 않는다. */
		margin: 0;
		font-size: 0.85rem;
		color: var(--text);
		line-height: 1.45;
		/* 500자까지 허용하는데 그 안에 공백 없는 긴 토큰(주소 등)이 있으면 상자를
		   넘는다 — `.plugin-target` 이 같은 이유로 break-all 을 쓴다. 이쪽은 산문
		   이므로 어절을 먼저 지키는 anywhere 를 쓴다. */
		overflow-wrap: anywhere;
	}
	.plugin-meta {
		display: flex;
		gap: 1rem;
		font-size: 0.78rem;
		color: var(--text-muted);
		flex-wrap: wrap;
	}
	.plugin-state {
		font-size: 0.8rem;
	}
	.plugin.pending .plugin-state {
		color: var(--warning);
	}
	.plugin-script {
		margin: 0.25rem 0 0;
		padding: 0.5rem;
		background: var(--bg-subtle);
		border-radius: var(--r-sm);
		font-family: var(--font-mono);
		font-size: 0.78rem;
		overflow-x: auto;
		/* DEV-383: 상한이 없으면 긴 스크립트가 허용 버튼을 화면 밖으로 밀어낸다
		   — 읽고 나서 동의하려면 스크립트 전체를 지나쳐 내려가야 했다.
		   rem 이라 UI 배율을 따라간다. */
		max-height: 14rem;
		overflow-y: auto;
		white-space: pre;
	}
	.plugin-actions {
		display: flex;
		gap: 0.4rem;
		margin-top: 0.15rem;
	}
	/* DEV-382: 앱의 버튼 관용구를 따른다. 처음엔 `ghost` / `primary` 라고 썼는데
	   이 저장소에 그런 클래스가 **없어서** 브라우저 기본 버튼이 그대로 나왔다 —
	   회색 덩어리 하나만 다른 UI 와 따로 놀았다. 같은 페이지의 `.btn-check-upd`
	   와 전역 `--btn-*` 토큰에 맞춘다. */
	.btn-plain,
	.btn-go {
		padding: 0.25rem 0.7rem;
		border-radius: var(--r-md);
		font-size: 0.78rem;
		cursor: pointer;
		transition:
			background 0.15s,
			color 0.15s,
			border-color 0.15s;
	}
	.btn-plain {
		background: transparent;
		border: var(--bw) solid var(--border);
		color: var(--text-muted);
	}
	.btn-plain:hover:not(:disabled) {
		background: var(--bg-subtle);
		color: var(--text);
	}
	/* 길드 통째 허용은 되돌리기 어렵다 — 평범한 버튼처럼 보이면 안 된다. */
	.btn-plain.trust {
		border-color: var(--btn-warning-border);
		color: var(--btn-warning-text);
	}
	.btn-plain.trust:hover:not(:disabled) {
		background: var(--btn-warning-bg-hover);
		color: var(--btn-warning-text);
	}
	.btn-go {
		background: var(--btn-primary-bg);
		border: var(--bw) solid var(--btn-primary-border);
		color: var(--btn-primary-text);
	}
	.btn-go:hover:not(:disabled) {
		background: var(--btn-primary-bg-hover);
		border-color: var(--btn-primary-border-hover);
	}
	.btn-plain:disabled,
	.btn-go:disabled {
		opacity: 0.5;
		cursor: not-allowed;
	}
	.plugin-error {
		color: var(--danger);
		font-size: 0.8rem;
		margin: 0.3rem 0 0;
		display: flex;
		gap: 0.5rem;
		align-items: center;
		flex-wrap: wrap;
	}
	.plugin-trusted {
		font-size: 0.85rem;
		color: var(--text-muted);
	}
	.plugin-broken-h {
		font-size: 0.9rem;
		margin: 1rem 0 0;
	}
	.link-btn {
		align-self: flex-start;
		background: none;
		border: none;
		padding: 0;
		font-size: 0.8rem;
		color: var(--accent);
		cursor: pointer;
		text-decoration: underline;
	}

	.settings {
		display: flex;
		gap: 1.5rem;
		max-width: var(--content-max-width, 900px);
		margin: 0 auto;
		padding: 1.5rem;
	}
	.side {
		flex: 0 0 10rem;
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
		border-radius: var(--r-md);
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
		border: var(--bw) solid var(--border);
		border-radius: var(--r-md);
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
	/* DEV-272: 글꼴 선택 — 드롭다운 + (직접 입력일 때) 이름 칸 + 견본. */
	.ui-scale .font-select,
	.ui-scale .font-custom {
		padding: 0.25rem 0.4rem;
		background: var(--bg-elevated);
		border: var(--bw) solid var(--border);
		border-radius: var(--r-md);
		color: var(--text);
		font: inherit;
		font-size: 0.875rem;
		outline: none;
	}
	.ui-scale .font-select:focus-visible,
	.ui-scale .font-custom:focus-visible {
		border-color: var(--accent);
	}
	.ui-scale .font-custom {
		flex: 1;
		min-width: 8rem;
	}
	/* 목록에 있다고 그 글꼴이 설치돼 있다는 뜻은 아니다 — 견본을 옆에 둬서
	   실제로 적용됐는지 눈으로 바로 확인하게 한다. 토큰을 그대로 쓰므로
	   화면 나머지와 같은 글꼴로 그려진다. */
	.ui-scale .font-preview {
		flex: none;
		color: var(--text-muted);
		font-size: 0.875rem;
		white-space: nowrap;
	}
	.ui-scale .font-preview.mono {
		font-family: var(--font-mono);
	}

	/* DEV-101 fix4: 직접 숫자 입력. */
	.ui-scale .num-input {
		display: inline-flex;
		align-items: center;
		gap: 0.25rem;
		padding: 0.15rem 0.4rem;
		background: var(--bg-elevated);
		border: var(--bw) solid var(--border);
		border-radius: var(--r-md);
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
		border: var(--bw) solid var(--border);
		border-radius: var(--r-md);
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
	/* DEV-383: 예전엔 `.ui-scale .scale-hint` 하나뿐이라 그 조상이 없는 탭
	   (플러그인)에서는 **아무것도 안 먹었다** — 안내문이 본문 크기 그대로에
	   여백 없이 붙어 나왔다. 정의가 있긴 있어서 `check:classes` 도 못 잡는다.
	   기본 모양은 조건 없이 준다. */
	.scale-hint {
		font-size: 0.75rem;
		color: var(--text-faint);
		margin: 0.5rem 0 0;
	}

	/* DEV-074: 테마 토글 — segmented (QuestList 의 view-toggle 과 같은 패턴). */
	.theme-toggle {
		/* BUG-254 계열(admin 보고): 글자는 rem 인데 여백만 px 이라 배율을
		   올리면 **주변 컴포넌트와 다른 속도로** 커졌다. 여백도 rem 으로.
		   16px 기준 환산이라 기본 배율에서 치수는 그대로다. */
		display: inline-flex;
		gap: 0;
		background: var(--bg-elevated);
		border: var(--bw) solid var(--border);
		border-radius: var(--r-md);
		padding: 0.125rem;
	}
	.th-btn {
		padding: 0.25rem 0.75rem;
		background: transparent;
		border: none;
		border-radius: var(--r-sm);
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
		border: var(--bw) solid var(--border);
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
		border: var(--bw) solid var(--border);
		border-radius: var(--r-md);
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
		border: var(--bw) solid var(--border);
		border-radius: var(--r-md);
		color: var(--text);
		font-size: 0.78rem;
		font-family: var(--font-mono);
		resize: vertical;
	}
	.ct-tokens {
		display: grid;
		grid-template-columns: repeat(auto-fill, minmax(11.25rem, 1fr));
		gap: 0.35rem 0.75rem;
		margin-top: 0.6rem;
	}
	.ct-token {
		display: flex;
		align-items: center;
		gap: 0.45rem;
		padding: 0.15rem 0.3rem;
		border-radius: var(--r-md);
	}
	.ct-token.overridden {
		background: color-mix(in srgb, var(--accent) 8%, transparent);
	}
	.ct-token input[type='color'] {
		width: 1.6rem;
		height: 1.6rem;
		padding: 0;
		border: var(--bw) solid var(--border);
		border-radius: var(--r-sm);
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

	/* BUG-200 (재수정, admin 제안): 설정 페이지 전체를 좁은 화면에서
	   `제목 / 내용` **세로 2단**으로.
	   
	   1차 시도는 같은 override 를 위쪽 @media 블록에 넣었는데, 기본
	   `.info-grid { grid-template-columns: 6rem 1fr }` 이 파일에서 **더 뒤에**
	   있어 같은 특이성에서 나중 규칙이 이겼다(실측: 좁은 화면에서도
	   `96px 231px` 그대로). 그래서 스타일 맨 끝으로 옮긴다.
	   
	   라벨 열(6rem) + 값 열의 컨트롤(슬라이더·버튼 묶음) 최소 폭이 겹쳐
	   그리드가 화면보다 넓어지던 것도 이걸로 사라진다. */
	@media (max-width: 640px) {
		.info-grid {
			grid-template-columns: 1fr;
			gap: 0.15rem 0;
		}
		.info-grid dt {
			margin-top: 0.6rem;
		}
		.info-grid dt:first-child {
			margin-top: 0;
		}
		/* 값 쪽은 남는 폭 안에서 줄어들 수 있어야 한다(슬라이더·버튼 묶음). */
		.info-grid dd {
			min-width: 0;
			max-width: 100%;
		}
		.info-grid dd > * {
			max-width: 100%;
		}
	}
</style>
