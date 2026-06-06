<!--
  DEV-084: 설정 페이지.

  좌측 세로 서브탭 (정보 / 업데이트 / 추후 항목) + 우측 패널. 자주 안 쓰는
  비-주요 기능 묶음 — 상단 nav 의 ⚙ 아이콘으로 진입.

  - 정보: 앱 이름 / 버전 / 저장소 링크.
  - 업데이트: 수동 체크 + 결과 floating toast (DEV-063 updater 재사용). Tauri 전용.
-->
<script lang="ts">
	import { onMount } from 'svelte';
	import { detectEnvironment } from '$lib/api/transport';
	import {
		updateState,
		checkForUpdate,
		downloadAndRelaunch,
		dismissUpdate
	} from '$lib/api/updater';
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
	// DEV-101 fix2: native input[type=range] 의 drag 문제 (값 재바인딩 →
	// thumb 튐, UI scale 의 자기 자신 변형 → 손 놓침) 회피한 델타 기반 슬라이더.
	import CustomSlider from '$lib/components/CustomSlider.svelte';

	// DEV-101 fix2: 슬라이더는 이제 CustomSlider 가 내부에서 preview 관리.
	// onInput 마다 미리보기 % 만 갱신, onChange (pointerup) 에서 store 반영.
	let previewScale = $state<number | null>(null);
	function displayScale(): number {
		return previewScale ?? $uiScale;
	}

	let previewContentWidth = $state<number | null>(null);
	function displayContentWidth(): number {
		return previewContentWidth ?? $contentWidth;
	}

	// floating toast 닫기 — updateState 를 idle 로.
	const dismissCheck = () => dismissUpdate();

	const isTauri = detectEnvironment() === 'tauri';

	// 앱 메타 — Tauri 에서는 실제 버전 (tauri.conf.json), 브라우저는 placeholder.
	let appVersion = $state('—');
	let appName = $state('openguild');
	const repoUrl = 'https://github.com/Jirung-E/openguild';

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
	});
</script>

<div class="settings">
	<aside class="side">
		<h1>설정</h1>
		<nav>
			<!-- DEV-086: '업데이트' 탭 제거 — 버튼은 정보 탭의 버전 아래로. 현재는
			     '정보' 만. 추후 테마 / 언어 / 길드 규칙 등 추가 시 여기 나열.
			     DEV-101: 단일 페이지 내 섹션 (탭 분리 X). -->
			<button class="tab active">정보 / 표시</button>
		</nav>
	</aside>

	<section class="panel">
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
						disabled={$updateState.status === 'checking' ||
							$updateState.status === 'downloading'}
					>
						{$updateState.status === 'checking' ? '확인 중…' : '업데이트 확인'}
					</button>
				{/if}
			</dd>
			<dt>저장소</dt>
			<dd><a href={repoUrl} target="_blank" rel="noreferrer noopener">{repoUrl}</a></dd>
		</dl>

		<!-- DEV-101: UI 크기 (rem scale) — 슬라이더 변경 시 즉시 반영. -->
		<h2 class="section">표시</h2>
		<dl class="info-grid">
			<dt>UI 크기</dt>
			<dd class="ui-scale">
				<div class="scale-row">
					<CustomSlider
						value={$uiScale}
						min={MIN_SCALE}
						max={MAX_SCALE}
						step={0.05}
						ariaLabel="UI 크기"
						onInput={(v) => (previewScale = v)}
						onChange={(v) => {
							previewScale = null;
							setUiScale(v);
						}}
					/>
					<span class="scale-val">{Math.round(displayScale() * 100)}%</span>
					<button
						class="btn-reset"
						onclick={resetUiScale}
						disabled={$uiScale === DEFAULT_SCALE}
						title="100% 로 초기화"
					>초기화</button>
				</div>
				<p class="scale-hint">전체 UI 의 텍스트 / 여백이 비례 확대·축소됩니다 (50%~200%). drag 중에는 미리보기만, 손 떼면 적용.</p>
			</dd>

			<!-- DEV-101 fix2: 컨텐츠 표시 영역 폭 — UI scale 과 별개. -->
			<dt>컨텐츠 폭</dt>
			<dd class="ui-scale">
				<div class="scale-row">
					<CustomSlider
						value={$contentWidth}
						min={MIN_CONTENT_WIDTH}
						max={MAX_CONTENT_WIDTH}
						step={20}
						ariaLabel="컨텐츠 폭"
						onInput={(v) => (previewContentWidth = v)}
						onChange={(v) => {
							previewContentWidth = null;
							setContentWidth(v);
						}}
					/>
					<span class="scale-val">{displayContentWidth()} px</span>
					<button
						class="btn-reset"
						onclick={resetContentWidth}
						disabled={$contentWidth === DEFAULT_CONTENT_WIDTH}
						title="기본 ({DEFAULT_CONTENT_WIDTH}px) 으로 초기화"
					>초기화</button>
				</div>
				<p class="scale-hint">
					페이지의 좌우 안전 영역 — 와이드 모니터에서 더 넓게 사용. 범위 {MIN_CONTENT_WIDTH}~{MAX_CONTENT_WIDTH}px.
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
				<p class="scale-hint">
					CSS 토큰 기반 — 일부 컴포넌트는 hardcoded 색이라 점진 마이그레이션 중. 시스템 모드는 OS 설정 따라 자동 전환.
				</p>
			</dd>
		</dl>
	</section>
</div>

<!-- DEV-085: 업데이트 확인 결과 — floating toast (fixed). 우하단에 떠서 레이아웃 영향 X. -->
{#if isTauri && $updateState.status !== 'idle'}
	<div class="upd-toast" class:err={$updateState.status === 'error'} role="status">
		<button class="upd-toast-x" title="닫기" onclick={dismissCheck}>×</button>
		{#if $updateState.status === 'checking'}
			<p class="t-title">업데이트 확인 중…</p>
		{:else if $updateState.status === 'uptodate'}
			<p class="t-title ok">최신 버전입니다.</p>
		{:else if $updateState.status === 'available'}
			<p class="t-title">새 버전 <strong>{$updateState.version}</strong> 사용 가능</p>
			{#if $updateState.notes}
				<details><summary>릴리즈 노트</summary><pre>{$updateState.notes}</pre></details>
			{/if}
			<button class="btn-primary" onclick={() => downloadAndRelaunch()}>
				지금 업데이트 (다운로드 + 재시작)
			</button>
		{:else if $updateState.status === 'downloading'}
			<p class="t-title">다운로드 중… {$updateState.pct !== null ? `${$updateState.pct}%` : ''}</p>
		{:else if $updateState.status === 'ready'}
			<p class="t-title ok">설치 완료 — 재시작 중…</p>
		{:else if $updateState.status === 'error'}
			<p class="t-title err">확인 실패</p>
			<p class="t-msg">{$updateState.message}</p>
		{/if}
	</div>
{/if}

<style>
	.settings {
		display: flex;
		gap: 1.5rem;
		max-width: 900px;
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
		transition: background 0.15s, color 0.15s;
	}
	.tab:hover { background: var(--bg-elevated); color: var(--text); }
	.tab.active { background: var(--bg-subtle); color: var(--text); font-weight: 600; }

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
	.info-grid dt { color: var(--text-muted); }
	.info-grid dd { margin: 0; color: var(--text); display: flex; flex-direction: column; gap: 0.35rem; align-items: flex-start; }
	.info-grid a { color: var(--accent); }

	/* DEV-086: 버전 아래 '슬쩍' 업데이트 확인 — subtle outline 버튼. */
	.btn-check-upd {
		padding: 0.2rem 0.6rem;
		background: transparent;
		border: 1px solid var(--border);
		border-radius: 6px;
		color: var(--text-muted);
		font-size: 0.75rem;
		cursor: pointer;
		transition: background 0.15s, color 0.15s;
	}
	.btn-check-upd:hover:not(:disabled) { background: var(--bg-subtle); color: var(--text); }
	.btn-check-upd:disabled { opacity: 0.5; cursor: not-allowed; }

	.btn-primary {
		padding: 0.4rem 0.9rem;
		background: var(--btn-primary-bg);
		border: 1px solid var(--btn-primary-border);
		border-radius: 6px;
		color: var(--btn-primary-text);
		font-size: 0.85rem;
		cursor: pointer;
	}
	.btn-primary:hover { background: var(--btn-primary-bg-hover); border-color: var(--btn-primary-border-hover); }
	.btn-primary:disabled { opacity: 0.5; cursor: not-allowed; }

	/* DEV-085: 업데이트 결과 floating toast — fixed, 우하단. 레이아웃 안 밀어냄. */
	.upd-toast {
		position: fixed;
		right: 1.5rem;
		bottom: 1.5rem;
		z-index: 60;
		max-width: 360px;
		padding: 0.85rem 2rem 0.85rem 1rem;
		background: var(--bg-elevated);
		border: 1px solid var(--border);
		border-left: 3px solid var(--success-strong);
		border-radius: 8px;
		box-shadow: 0 6px 20px rgba(0, 0, 0, 0.5);
		font-size: 0.85rem;
		color: var(--text);
	}
	.upd-toast.err { border-left-color: var(--danger); }
	.upd-toast-x {
		position: absolute;
		top: 0.4rem;
		right: 0.5rem;
		background: none;
		border: none;
		color: var(--text-faint);
		font-size: 1.1rem;
		line-height: 1;
		cursor: pointer;
	}
	.upd-toast-x:hover { color: var(--text); }
	.upd-toast .t-title { margin: 0; font-weight: 600; }
	.upd-toast .t-title.ok { color: var(--success); }
	.upd-toast .t-title.err { color: var(--danger); }
	.upd-toast .t-msg { margin: 0.35rem 0 0; color: var(--text-muted); font-size: 0.8rem; word-break: break-word; }
	.upd-toast details { margin: 0.5rem 0; color: var(--text-muted); }
	.upd-toast pre {
		white-space: pre-wrap;
		background: var(--bg);
		border: 1px solid var(--bg-subtle);
		border-radius: 6px;
		padding: 0.5rem 0.75rem;
		max-height: 8rem;
		overflow-y: auto;
		margin: 0.4rem 0 0;
	}
	.upd-toast .btn-primary { margin-top: 0.5rem; }

	/* DEV-101: 표시 섹션 — UI 크기 슬라이더. */
	.panel h2.section {
		margin: 1.75rem 0 1rem;
		padding-top: 1rem;
		border-top: 1px solid var(--bg-subtle);
	}
	.ui-scale .scale-row {
		display: flex;
		align-items: center;
		gap: 0.75rem;
		width: 100%;
		max-width: 24rem;
	}
	.ui-scale .scale-val {
		min-width: 3.5rem;
		font-variant-numeric: tabular-nums;
		color: var(--text);
		font-size: 0.875rem;
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
		transition: background 0.1s, color 0.1s;
	}
	.th-btn:hover { color: var(--text); }
	.th-btn.active {
		background: var(--bg-subtle);
		color: var(--text);
	}
</style>
