<!--
  DEV-084: 설정 페이지.

  좌측 세로 서브탭 (정보 / 업데이트 / 추후 항목) + 우측 패널. 자주 안 쓰는
  비-주요 기능 묶음 — 상단 nav 의 ⚙ 아이콘으로 진입.

  - 정보: 앱 이름 / 버전 / 저장소 링크.
  - 업데이트: 수동 체크 버튼만 — 결과 표시는 전역 UpdateBanner(우하단
    floating toast, DEV-194 후속으로 통합)가 담당. Tauri 전용.
-->
<script lang="ts">
	import { onMount } from 'svelte';
	import { detectEnvironment } from '$lib/api/transport';
	import { updateState, checkForUpdate } from '$lib/api/updater';
	import {
		uiScale,
		setUiScale,
		resetUiScale,
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
		DEFAULT_CONTENT_WIDTH
	} from '$lib/stores/contentWidth';
	import { theme, setTheme, type ThemeChoice } from '$lib/stores/theme';
	// DEV-015 (MVP): 언어 토글 — 설정 페이지에도 노출.
	import { locale, setLocale, type Locale } from '$lib/stores/locale';
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

	// DEV-101 fix3: 즉시 반영 — store 가 source of truth, drag 중에도 매 step 적용.
	// preview / displayScale wrapper 제거.

	// DEV-052 / DEV-101 fix6: 탭 분리 — '정보' / '표시'.
	// DEV-207 후속: '원격 서버' 전용 탭은 폐기 — '정보' 탭에 길드 위치 한
	// 줄로 통합(사용자 피드백: "그냥 정보 탭에 원격이면 주소, 로컬이면 경로
	// 출력해서 보여주고, 웰컴페이지에서 들어간거면 표시 안하면 되는거 아님?").
	type Tab = 'info' | 'display' | 'editor';
	let activeTab = $state<Tab>('info');

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
			appVersion = '개발 (브라우저)';
			return;
		}
		try {
			const { getVersion, getName } = await import('@tauri-apps/api/app');
			appVersion = await getVersion();
			appName = await getName();
		} catch {
			appVersion = '알 수 없음';
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
	const anyGuildOpen = $derived(localGuildOpen || isRemoteActive);
</script>

<div class="settings">
	<aside class="side">
		<h1>설정</h1>
		<nav>
			<!-- DEV-052 / DEV-101 fix6: 탭 분리 — '정보' / '표시'. -->
			<button
				class="tab"
				class:active={activeTab === 'info'}
				onclick={() => (activeTab = 'info')}
				aria-pressed={activeTab === 'info'}>정보</button
			>
			<button
				class="tab"
				class:active={activeTab === 'display'}
				onclick={() => (activeTab = 'display')}
				aria-pressed={activeTab === 'display'}>표시</button
			>
			<!-- DEV-130: 편집기 들여쓰기 설정. -->
			<button
				class="tab"
				class:active={activeTab === 'editor'}
				onclick={() => (activeTab = 'editor')}
				aria-pressed={activeTab === 'editor'}>편집기</button
			>
		</nav>
	</aside>

	<section class="panel">
		{#if activeTab === 'editor'}
			<!-- DEV-130: 본문 편집기 들여쓰기 — 코드 편집기처럼 tab/space + 칸수 선택. -->
			<h2>편집기</h2>
			<dl class="info-grid">
				<dt>Tab 동작</dt>
				<dd class="theme-row">
					<div class="theme-toggle" role="group" aria-label="Tab 동작">
						<button
							class="th-btn"
							class:active={$editorSettings.tabMode === 'tab'}
							onclick={() => setTabMode('tab')}
							aria-pressed={$editorSettings.tabMode === 'tab'}>탭 문자</button
						>
						<button
							class="th-btn"
							class:active={$editorSettings.tabMode === 'space'}
							onclick={() => setTabMode('space')}
							aria-pressed={$editorSettings.tabMode === 'space'}>공백</button
						>
					</div>
					<p class="scale-hint">
						Tab 키를 눌렀을 때 탭 문자(\t)를 넣을지, 공백을 넣을지. 퀘스트 / 캠페인 본문 편집기에
						적용.
					</p>
				</dd>
				<dt>들여쓰기 칸수</dt>
				<dd class="theme-row">
					<div class="theme-toggle" role="group" aria-label="들여쓰기 칸수">
						{#each [2, 4] as n (n)}
							<button
								class="th-btn"
								class:active={$editorSettings.indentSize === n}
								onclick={() => setIndentSize(n as IndentSize)}
								aria-pressed={$editorSettings.indentSize === n}>{n}칸</button
							>
						{/each}
					</div>
					<p class="scale-hint">
						공백 모드에서 Tab 한 번에 넣을 공백 개수 (탭 문자 모드에선 표시 폭). 2 / 4 중 선택.
					</p>
				</dd>
			</dl>
		{:else if activeTab === 'info'}
			<h2>정보</h2>
			<dl class="info-grid">
				<dt>앱 이름</dt>
				<dd>{appName}</dd>
				<dt>버전</dt>
				<dd>
					<span>{appVersion}</span>
					{#if isTauri}
						<!-- DEV-086: 버전 아래 '슬쩍' 업데이트 확인. 결과는 floating toast. -->
						<button
							class="btn-check-upd"
							onclick={() => checkForUpdate()}
							disabled={$updateState.status === 'checking' || $updateState.status === 'downloading'}
						>
							{$updateState.status === 'checking' ? '확인 중…' : '업데이트 확인'}
						</button>
					{/if}
				</dd>
				<!-- DEV-207(사용자 피드백: "정보 탭에 원격이면 주소, 로컬이면 경로
				     출력하고, 웰컴페이지에서 들어간거면 표시 안하면 되는거 아님?"):
				     별도 '원격 서버' 탭 폐기 — 길드가 열려있을 때만(anyGuildOpen) 한
				     줄로 통합 표시. 연결/해제는 여전히 Welcome 화면(로고 클릭)에서만. -->
				{#if isTauri && anyGuildOpen}
					<dt>{isRemoteActive ? '원격 서버' : '길드 경로'}</dt>
					<dd>
						{#if isRemoteActive}
							<span class="remote-active">{$remoteServerUrl}</span>
						{:else}
							<span>{guildPath ?? '—'}</span>
						{/if}
					</dd>
				{/if}
				<dt>저장소</dt>
				<dd><a href={repoUrl} target="_blank" rel="noreferrer noopener">{repoUrl}</a></dd>
			</dl>
		{:else}
			<!-- DEV-101: UI 크기 (rem scale) — 슬라이더 변경 시 즉시 반영. -->
			<h2>표시</h2>
			<dl class="info-grid">
				<dt>UI 크기</dt>
				<dd class="ui-scale">
					<div class="scale-row">
						<CustomSlider
							value={$uiScale}
							min={MIN_SCALE}
							max={MAX_SCALE}
							step={0.01}
							ariaLabel="UI 크기"
							onChange={setUiScale}
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
								aria-label="UI 크기 (퍼센트)"
							/>
							<span class="unit">%</span>
						</div>
						<button
							class="btn-reset"
							onclick={resetUiScale}
							disabled={$uiScale === DEFAULT_SCALE}
							title="100% 로 초기화">초기화</button
						>
					</div>
					<p class="scale-hint">
						전체 UI 의 텍스트 / 여백이 비례 확대·축소됩니다 ({Math.round(
							MIN_SCALE * 100
						)}%~{Math.round(MAX_SCALE * 100)}%, 1% 단위). 슬라이더 / 숫자 입력 모두 즉시 적용.
					</p>
				</dd>

				<!-- DEV-101 fix2: 컨텐츠 표시 영역 폭 — UI scale 과 별개. -->
				<dt>컨텐츠 폭</dt>
				<dd class="ui-scale">
					<div class="scale-row">
						<CustomSlider
							value={$contentWidth}
							min={MIN_CONTENT_WIDTH}
							max={MAX_CONTENT_WIDTH}
							step={5}
							ariaLabel="컨텐츠 폭"
							onChange={setContentWidth}
						/>
						<!-- DEV-101 fix4: 직접 숫자 입력. px 단위. -->
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
								aria-label="컨텐츠 폭 (픽셀)"
							/>
							<span class="unit">px</span>
						</div>
						<button
							class="btn-reset"
							onclick={resetContentWidth}
							disabled={$contentWidth === DEFAULT_CONTENT_WIDTH}
							title="기본 ({DEFAULT_CONTENT_WIDTH}px) 으로 초기화">초기화</button
						>
					</div>
					<p class="scale-hint">
						페이지의 좌우 안전 영역 — 와이드 모니터에서 더 넓게 사용. 범위 {MIN_CONTENT_WIDTH}~{MAX_CONTENT_WIDTH}px,
						5px 단위.
					</p>
				</dd>

				<!-- DEV-074: 테마 (Dark / Light / System). -->
				<dt>테마</dt>
				<dd class="theme-row">
					<div class="theme-toggle" role="group" aria-label="테마">
						{#each ['dark', 'light', 'system'] as opt (opt)}
							<button
								class="th-btn"
								class:active={$theme === opt}
								onclick={() => setTheme(opt as ThemeChoice)}
								aria-pressed={$theme === opt}
							>
								{opt === 'dark' ? '다크' : opt === 'light' ? '라이트' : '시스템'}
							</button>
						{/each}
					</div>
					<p class="scale-hint">CSS 토큰 기반 — 시스템 모드는 OS 설정 따라 자동 전환.</p>
				</dd>

				<!-- DEV-015 (MVP): 언어 토글 — 현재는 이 설정 페이지 라벨 일부에만 적용. -->
				<dt>언어</dt>
				<dd class="theme-row">
					<div class="theme-toggle" role="group" aria-label="언어">
						<!-- DEV-015 후속: 언어 이름 자체는 번역 대상이 아님 — 항상 고정 표기. -->
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
						현재는 MVP — 일부 화면(이 설정 페이지 등)의 라벨만 전환됩니다. 전체 적용은 후속(DEV-205)
						작업입니다.
					</p>
				</dd>
			</dl>
		{/if}
	</section>
</div>

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
</style>
