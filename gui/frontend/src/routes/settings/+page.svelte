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
					<input
						type="range"
						min={MIN_SCALE}
						max={MAX_SCALE}
						step="0.1"
						value={$uiScale}
						oninput={(e) => setUiScale(Number.parseFloat(e.currentTarget.value))}
						aria-label="UI 크기"
					/>
					<span class="scale-val">{Math.round($uiScale * 100)}%</span>
					<button
						class="btn-reset"
						onclick={resetUiScale}
						disabled={$uiScale === DEFAULT_SCALE}
						title="100% 로 초기화"
					>초기화</button>
				</div>
				<p class="scale-hint">전체 UI 의 텍스트 / 여백이 비례 확대·축소됩니다 (50%~200%). 즉시 반영 + 자동 저장.</p>
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
		color: #c9d1d9;
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
		color: #8b949e;
		border-radius: 6px;
		font-size: 0.9rem;
		cursor: pointer;
		transition: background 0.15s, color 0.15s;
	}
	.tab:hover { background: #161b22; color: #c9d1d9; }
	.tab.active { background: #21262d; color: #c9d1d9; font-weight: 600; }

	.panel {
		flex: 1;
		min-width: 0;
	}
	.panel h2 {
		font-size: 1rem;
		color: #c9d1d9;
		margin: 0 0 1rem;
	}

	.info-grid {
		display: grid;
		grid-template-columns: 6rem 1fr;
		gap: 0.5rem 1rem;
		margin: 0 0 1rem;
		font-size: 0.875rem;
	}
	.info-grid dt { color: #8b949e; }
	.info-grid dd { margin: 0; color: #c9d1d9; display: flex; flex-direction: column; gap: 0.35rem; align-items: flex-start; }
	.info-grid a { color: #58a6ff; }

	/* DEV-086: 버전 아래 '슬쩍' 업데이트 확인 — subtle outline 버튼. */
	.btn-check-upd {
		padding: 0.2rem 0.6rem;
		background: transparent;
		border: 1px solid #30363d;
		border-radius: 6px;
		color: #8b949e;
		font-size: 0.75rem;
		cursor: pointer;
		transition: background 0.15s, color 0.15s;
	}
	.btn-check-upd:hover:not(:disabled) { background: #21262d; color: #c9d1d9; }
	.btn-check-upd:disabled { opacity: 0.5; cursor: not-allowed; }

	.btn-primary {
		padding: 0.4rem 0.9rem;
		background: #238636;
		border: 1px solid #2ea043;
		border-radius: 6px;
		color: #fff;
		font-size: 0.85rem;
		cursor: pointer;
	}
	.btn-primary:hover { background: #2ea043; }
	.btn-primary:disabled { opacity: 0.5; cursor: not-allowed; }

	/* DEV-085: 업데이트 결과 floating toast — fixed, 우하단. 레이아웃 안 밀어냄. */
	.upd-toast {
		position: fixed;
		right: 1.5rem;
		bottom: 1.5rem;
		z-index: 60;
		max-width: 360px;
		padding: 0.85rem 2rem 0.85rem 1rem;
		background: #161b22;
		border: 1px solid #30363d;
		border-left: 3px solid #2ea043;
		border-radius: 8px;
		box-shadow: 0 6px 20px rgba(0, 0, 0, 0.5);
		font-size: 0.85rem;
		color: #c9d1d9;
	}
	.upd-toast.err { border-left-color: #f85149; }
	.upd-toast-x {
		position: absolute;
		top: 0.4rem;
		right: 0.5rem;
		background: none;
		border: none;
		color: #6e7681;
		font-size: 1.1rem;
		line-height: 1;
		cursor: pointer;
	}
	.upd-toast-x:hover { color: #c9d1d9; }
	.upd-toast .t-title { margin: 0; font-weight: 600; }
	.upd-toast .t-title.ok { color: #56d364; }
	.upd-toast .t-title.err { color: #f85149; }
	.upd-toast .t-msg { margin: 0.35rem 0 0; color: #8b949e; font-size: 0.8rem; word-break: break-word; }
	.upd-toast details { margin: 0.5rem 0; color: #8b949e; }
	.upd-toast pre {
		white-space: pre-wrap;
		background: #0d1117;
		border: 1px solid #21262d;
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
		border-top: 1px solid #21262d;
	}
	.ui-scale .scale-row {
		display: flex;
		align-items: center;
		gap: 0.75rem;
		width: 100%;
		max-width: 24rem;
	}
	.ui-scale input[type='range'] {
		flex: 1;
		accent-color: #58a6ff;
	}
	.ui-scale .scale-val {
		min-width: 3.5rem;
		font-variant-numeric: tabular-nums;
		color: #c9d1d9;
		font-size: 0.875rem;
	}
	.ui-scale .btn-reset {
		padding: 0.2rem 0.6rem;
		background: transparent;
		border: 1px solid #30363d;
		border-radius: 6px;
		color: #c9d1d9;
		font-size: 0.8rem;
		cursor: pointer;
	}
	.ui-scale .btn-reset:hover:not(:disabled) {
		background: #161b22;
		border-color: #6e7681;
	}
	.ui-scale .btn-reset:disabled {
		opacity: 0.4;
		cursor: not-allowed;
	}
	.ui-scale .scale-hint {
		font-size: 0.75rem;
		color: #6e7681;
		margin: 0.5rem 0 0;
	}
</style>
