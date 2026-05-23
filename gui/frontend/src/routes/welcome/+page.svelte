<script lang="ts">
	import { onMount } from 'svelte';
	import { goto } from '$app/navigation';
	import { recentsApi, type Recent } from '$lib/api/recents';
	import { detectEnvironment } from '$lib/api/transport';

	let recents: Recent[] = $state([]);
	let loading = $state(true);
	let err: string | null = $state(null);
	let confirmOpen = $state(false); // 브라우저 confirm 대신 커스텀 모달.
	let opening: string | null = $state(null); // 진행 중인 path (UI 비활성화).
	let openErr: string | null = $state(null);

	// DEV-052 후속 (2회차): 길드 미초기화 디렉토리에서 시작했을 때의 prompt.
	let uninitPath: string | null = $state(null);
	let initRunning = $state(false);
	let initErr: string | null = $state(null);
	// DEV-052 후속 (4회차): 길드 이름 input. 기본값은 디렉토리명.
	let initName = $state('');

	const env = detectEnvironment();

	onMount(async () => {
		// recents 먼저 로드.
		try {
			recents = await recentsApi.list();
		} catch (e) {
			err = e instanceof Error ? e.message : String(e);
		} finally {
			loading = false;
		}
		// launch_mode 가 'uninit' 이면 prompt 활성화.
		if (env === 'tauri') {
			try {
				const { invoke } = await import('@tauri-apps/api/core');
				const info = await invoke<{ mode: string; uninit_path: string | null }>(
					'launch_mode'
				);
				if (info.mode === 'uninit' && info.uninit_path) {
					uninitPath = info.uninit_path;
					// 기본 길드 이름 = path 의 마지막 component (디렉토리명).
					// Windows / POSIX 모두에서 동작하도록 둘 다 split.
					const parts = info.uninit_path.split(/[\\/]/).filter(Boolean);
					initName = parts[parts.length - 1] ?? 'guild';
				}
			} catch {
				// invoke 실패 시 무시.
			}
		}
	});

	async function openRecent(path: string) {
		// DEV-052 후속 (2회차): 현재 창에서 store swap (새 창 spawn 안 함).
		// Tauri 환경 외에서 호출되면 에러 표시.
		if (opening) return;
		opening = path;
		openErr = null;
		try {
			if (env !== 'tauri') {
				throw new Error('Tauri 데스크톱 앱에서만 동작합니다.');
			}
			const { invoke } = await import('@tauri-apps/api/core');
			await invoke('open_guild_in_current_window', { path });
			// 성공: 현재 process 의 Store 가 swap 됐음. 보드로 이동.
			goto('/');
		} catch (e) {
			openErr = e instanceof Error ? e.message : String(e);
		} finally {
			opening = null;
		}
	}

	async function initUninit() {
		if (!uninitPath || initRunning) return;
		const name = initName.trim();
		if (!name) {
			initErr = '길드 이름을 입력하세요.';
			return;
		}
		initRunning = true;
		initErr = null;
		try {
			const { invoke } = await import('@tauri-apps/api/core');
			await invoke('init_and_open_guild', { path: uninitPath, name });
			// 성공: store swap 됨. 보드로.
			goto('/');
		} catch (e) {
			initErr = e instanceof Error ? e.message : String(e);
		} finally {
			initRunning = false;
		}
	}

	async function removeRecent(path: string) {
		try {
			await recentsApi.remove(path);
			recents = recents.filter((r) => r.path !== path);
		} catch (e) {
			openErr = e instanceof Error ? e.message : String(e);
		}
	}

	function declineUninit() {
		// 사용자가 "초기화 안 함" 선택 → uninitPath 만 비워 prompt 닫음 (welcome 으로 머묾).
		uninitPath = null;
	}

	function askClear() {
		confirmOpen = true;
	}

	async function doClear() {
		confirmOpen = false;
		await recentsApi.clear();
		recents = [];
	}

	function cancelClear() {
		confirmOpen = false;
	}

	function fmtDate(iso: string): string {
		const d = new Date(iso);
		if (Number.isNaN(d.getTime())) return iso;
		const pad = (n: number) => String(n).padStart(2, '0');
		return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())} ${pad(d.getHours())}:${pad(d.getMinutes())}`;
	}
</script>

<svelte:head>
	<title>Welcome — OpenGuild</title>
</svelte:head>

<main class="welcome">
	<header>
		<h1>OpenGuild</h1>
		<p class="sub">최근 작업한 길드</p>
	</header>

	{#if uninitPath}
		<!-- DEV-052 후속: 길드 마커 없는 디렉토리에서 시작 → 초기화 prompt. -->
		<section class="uninit">
			<h2>이 위치를 길드로 초기화할까요?</h2>
			<p class="uninit-path">{uninitPath}</p>
			<p class="uninit-desc">
				지정한 디렉토리에 OpenGuild 마커 (<code>.guild/</code> 폴더 + 시드)가 없습니다.
				초기화하면 빈 길드가 생성되어 바로 작업할 수 있습니다.
			</p>
			<label class="uninit-name">
				<span>길드 이름</span>
				<input
					type="text"
					bind:value={initName}
					placeholder="guild"
					disabled={initRunning}
				/>
			</label>
			{#if initErr}
				<p class="err">{initErr}</p>
			{/if}
			<div class="uninit-actions">
				<button class="btn-yes" onclick={initUninit} disabled={initRunning}>
					{initRunning ? '초기화 중…' : '초기화하고 열기'}
				</button>
				<button class="btn-no" onclick={declineUninit} disabled={initRunning}>아니요</button>
			</div>
		</section>
	{/if}

	{#if loading}
		<p class="loading">불러오는 중...</p>
	{:else if err}
		<p class="err">{err}</p>
	{:else if env !== 'tauri'}
		<p class="info">
			Recent guild 목록은 desktop 앱 (Tauri) 에서만 동작합니다.<br />
			브라우저 모드에선 현재 server 가 호스팅한 길드만 표시됩니다.
		</p>
	{:else if recents.length === 0}
		<p class="empty">
			아직 열어본 길드가 없습니다.<br />
			<code>openguild init</code> 으로 새 길드를 만들거나, <code>openguild-gui &lt;path&gt;</code>
			로 기존 길드를 열어 보세요.
		</p>
	{:else}
		<ul class="recent-list">
			{#each recents as r (r.path)}
				<li class="recent-row" class:missing={r.missing}>
					<button
						class="recent-btn"
						type="button"
						onclick={() => openRecent(r.path)}
						disabled={opening !== null || r.missing}
						title={r.missing
							? '경로를 찾을 수 없습니다 — 이동 / 삭제됐을 수 있음'
							: '현재 창에서 이 길드를 엽니다'}
					>
						<div class="row">
							<span class="name">{r.name}</span>
							<span class="last">{fmtDate(r.last_opened)}</span>
						</div>
						<div class="path">{r.path}</div>
						{#if r.missing}
							<div class="missing-label">⚠ 경로를 찾을 수 없습니다</div>
						{/if}
						{#if opening === r.path}
							<div class="opening">길드 여는 중…</div>
						{/if}
					</button>
					{#if r.missing}
						<button
							class="recent-remove"
							type="button"
							onclick={() => removeRecent(r.path)}
							title="목록에서 제거 (디스크 데이터는 건드리지 않음)"
							aria-label="목록에서 제거"
						>
							×
						</button>
					{/if}
				</li>
			{/each}
		</ul>
		{#if openErr}
			<p class="err">{openErr}</p>
		{/if}
		<button class="clear" onclick={askClear}>전체 비우기</button>
	{/if}

	<footer class="hint">
		<p>
			항목을 클릭하면 현재 창에서 그 길드를 엽니다.
		</p>
	</footer>
</main>

{#if confirmOpen}
	<!-- DEV-052 후속: 브라우저 confirm() 대신 인앱 스타일 모달. -->
	<div class="ov" role="presentation">
		<div class="modal" role="dialog" aria-modal="true" tabindex="-1">
			<h3 class="modal-title">최근 길드 목록 비우기</h3>
			<p class="modal-msg">Recent 목록을 모두 비울까요? 되돌릴 수 없습니다.</p>
			<div class="modal-actions">
				<button class="btn-yes" onclick={doClear}>비우기</button>
				<button class="btn-no" onclick={cancelClear}>취소</button>
			</div>
		</div>
	</div>
{/if}

<style>
	.welcome {
		max-width: 720px;
		margin: 0 auto;
		padding: 2rem 1.5rem;
		color: #c9d1d9;
	}
	header h1 {
		margin: 0;
		font-size: 2rem;
		color: #4a90d9;
	}
	header .sub {
		margin: 0.25rem 0 1.5rem;
		color: #8b95a1;
	}
	.recent-list {
		list-style: none;
		padding: 0;
		margin: 0;
		display: flex;
		flex-direction: column;
		gap: 0.5rem;
	}
	.recent-list .recent-row {
		display: flex;
		gap: 0.5rem;
		align-items: stretch;
	}
	.recent-list .recent-row.missing .recent-btn {
		opacity: 0.55;
		cursor: not-allowed;
	}
	.recent-remove {
		flex: 0 0 auto;
		padding: 0 0.85rem;
		background: transparent;
		border: 1px solid #30363d;
		border-radius: 6px;
		color: #8b95a1;
		font-size: 1.2rem;
		line-height: 1;
		cursor: pointer;
		transition: border-color 0.12s, color 0.12s, background 0.12s;
	}
	.recent-remove:hover {
		border-color: #e94f4f;
		color: #e94f4f;
		background: rgba(233, 79, 79, 0.08);
	}
	.recent-btn { flex: 1 1 auto; }
	.missing-label {
		color: #e9a04f;
		font-size: 0.8rem;
	}
	.recent-btn {
		width: 100%;
		text-align: left;
		padding: 0.75rem 1rem;
		background: #161b22;
		border: 1px solid #30363d;
		border-radius: 6px;
		color: inherit;
		font: inherit;
		cursor: pointer;
		display: grid;
		grid-template-columns: 1fr;
		gap: 0.25rem;
		transition: border-color 0.12s, background 0.12s;
	}
	.recent-list .recent-row:not(.missing) .recent-btn:hover:not(:disabled) {
		border-color: #58a6ff;
		background: #1a212a;
	}
	.recent-btn:disabled {
		opacity: 0.6;
		cursor: default;
	}
	.recent-btn .row {
		display: flex;
		justify-content: space-between;
		align-items: baseline;
		gap: 1rem;
	}
	.recent-btn .name {
		font-weight: 600;
		font-size: 1.05rem;
	}
	.recent-btn .last {
		color: #8b95a1;
		font-size: 0.85rem;
		font-family: 'SFMono-Regular', Consolas, monospace;
	}
	.recent-btn .path {
		color: #8b95a1;
		font-size: 0.85rem;
		font-family: 'SFMono-Regular', Consolas, monospace;
		word-break: break-all;
	}
	.recent-btn .opening {
		color: #58a6ff;
		font-size: 0.8rem;
	}
	.clear {
		margin-top: 1rem;
		padding: 0.4rem 0.9rem;
		background: transparent;
		color: #e94f4f;
		border: 1px solid #30363d;
		border-radius: 4px;
		font-size: 0.85rem;
		cursor: pointer;
	}
	.clear:hover {
		border-color: #e94f4f;
	}
	.loading, .empty, .info, .err {
		padding: 1rem;
		background: #161b22;
		border-radius: 6px;
		color: #8b95a1;
	}
	.err {
		color: #e94f4f;
		margin-top: 1rem;
	}
	code {
		background: #0d1117;
		padding: 0.1rem 0.4rem;
		border-radius: 3px;
		font-family: 'SFMono-Regular', Consolas, monospace;
		font-size: 0.9em;
	}
	.hint {
		margin-top: 2rem;
		padding-top: 1rem;
		border-top: 1px solid #30363d;
		color: #8b95a1;
		font-size: 0.85rem;
	}

	/* --- uninit prompt (DEV-052 후속 2회차) --- */
	.uninit {
		margin-bottom: 1.5rem;
		padding: 1rem 1.25rem;
		background: #1a212a;
		border: 1px solid #58a6ff;
		border-radius: 8px;
	}
	.uninit h2 {
		margin: 0 0 0.5rem;
		font-size: 1.05rem;
		color: #58a6ff;
	}
	.uninit-path {
		margin: 0 0 0.5rem;
		padding: 0.4rem 0.6rem;
		background: #0d1117;
		border-radius: 4px;
		font-family: 'SFMono-Regular', Consolas, monospace;
		font-size: 0.85rem;
		color: #c9d1d9;
		word-break: break-all;
	}
	.uninit-desc {
		margin: 0 0 0.85rem;
		color: #c9d1d9;
		font-size: 0.875rem;
	}
	.uninit-name {
		display: flex;
		align-items: center;
		gap: 0.6rem;
		margin: 0 0 0.85rem;
		font-size: 0.875rem;
		color: #c9d1d9;
	}
	.uninit-name > span {
		flex: 0 0 auto;
		color: #8b95a1;
	}
	.uninit-name input {
		flex: 1 1 auto;
		padding: 0.4rem 0.6rem;
		background: #0d1117;
		border: 1px solid #30363d;
		border-radius: 4px;
		color: #c9d1d9;
		font: inherit;
		font-family: 'SFMono-Regular', Consolas, monospace;
	}
	.uninit-name input:focus {
		outline: none;
		border-color: #58a6ff;
	}
	.uninit-actions {
		display: flex;
		gap: 0.5rem;
		justify-content: flex-end;
	}

	/* --- 커스텀 confirm 모달 --- */
	.ov {
		position: fixed; inset: 0;
		background: rgba(0, 0, 0, 0.6);
		z-index: 100;
		display: flex; align-items: center; justify-content: center;
		padding: 1rem;
	}
	.modal {
		background: #161b22;
		border: 1px solid #30363d; border-radius: 10px;
		width: 100%; max-width: 420px;
		padding: 1.2rem 1.4rem;
		box-shadow: 0 12px 36px rgba(0, 0, 0, 0.6);
		color: #c9d1d9;
	}
	.modal-title {
		margin: 0 0 0.5rem;
		font-size: 1rem; font-weight: 600; color: #e6edf3;
	}
	.modal-msg {
		margin: 0 0 1rem;
		font-size: 0.875rem; color: #c9d1d9;
	}
	.modal-actions {
		display: flex; gap: 0.5rem; justify-content: flex-end;
	}
	.btn-yes {
		padding: 0.4rem 1.1rem;
		background: rgba(233, 79, 79, 0.15);
		border: 1px solid #e94f4f; border-radius: 6px;
		color: #e94f4f; font-size: 0.875rem; cursor: pointer;
	}
	.btn-yes:hover { background: rgba(233, 79, 79, 0.25); }
	.btn-no {
		padding: 0.4rem 1rem;
		background: transparent;
		border: 1px solid #30363d; border-radius: 6px;
		color: #8b949e; font-size: 0.875rem; cursor: pointer;
	}
	.btn-no:hover { background: #21262d; }
</style>
