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

	// floating toast 닫기 — updateState 를 idle 로.
	const dismissCheck = () => dismissUpdate();

	type Tab = 'info' | 'update';
	let tab = $state<Tab>('info');

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
			<button class="tab" class:active={tab === 'info'} onclick={() => (tab = 'info')}>
				정보
			</button>
			<button class="tab" class:active={tab === 'update'} onclick={() => (tab = 'update')}>
				업데이트
			</button>
			<!-- 추후: 테마 / 언어 / 길드 규칙 등 여기에 추가. -->
		</nav>
	</aside>

	<section class="panel">
		{#if tab === 'info'}
			<h2>정보</h2>
			<dl class="info-grid">
				<dt>앱 이름</dt>
				<dd>{appName}</dd>
				<dt>버전</dt>
				<dd>{appVersion}</dd>
				<dt>저장소</dt>
				<dd><a href={repoUrl} target="_blank" rel="noreferrer noopener">{repoUrl}</a></dd>
			</dl>
		{:else if tab === 'update'}
			<h2>업데이트</h2>
			{#if !isTauri}
				<p class="note">업데이트 기능은 데스크탑 앱에서만 사용할 수 있습니다.</p>
			{:else}
				<p class="note">현재 버전: <strong>{appVersion}</strong></p>

				<div class="upd-row">
					<button
						class="btn-primary"
						onclick={() => checkForUpdate()}
						disabled={$updateState.status === 'checking' ||
							$updateState.status === 'downloading'}
					>
						{$updateState.status === 'checking' ? '확인 중…' : '업데이트 확인'}
					</button>
				</div>

				<p class="note dim">
					앱은 시작 시 + 실행 중 주기적으로 자동 확인하며, 새 버전이 있으면 상단에
					알림 배너를 표시합니다. 설치는 항상 사용자가 직접 선택합니다.
				</p>
			{/if}
		{/if}
	</section>
</div>

<!-- DEV-085: 업데이트 확인 결과 — floating toast (fixed). 이전엔 인라인이라
     컨텐츠를 아래로 밀어 보기 불편했음. 우하단에 떠서 레이아웃 영향 X. -->
{#if isTauri && tab === 'update' && $updateState.status !== 'idle'}
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
	.info-grid dd { margin: 0; color: #c9d1d9; }
	.info-grid a { color: #58a6ff; }

	.note { font-size: 0.85rem; color: #8b949e; line-height: 1.5; }
	.note.dim { color: #6e7681; margin-top: 1rem; }

	.upd-row { margin: 1rem 0; }
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
</style>
