<script lang="ts">
	import { onMount } from 'svelte';
	import { goto } from '$app/navigation';
	import { recentsApi, type Recent } from '$lib/api/recents';
	import { detectEnvironment } from '$lib/api/transport';
	// DEV-138: welcome 에서도 ⚙ 퀵메뉴 (Nav 와 동일 컴포넌트).
	import SettingsQuickMenu from '$lib/components/SettingsQuickMenu.svelte';
	// DEV-154: 호환 안 되는 길드(더 새 schema) 전용 안내 + 업데이트 확인.
	import ConfirmDialog from '$lib/components/ConfirmDialog.svelte';
	// DEV-154: UpdateBanner 는 available/downloading/ready 만 표시(DEV-085) — checking/
	// uptodate/error 는 전역 배너에 안 떠서 welcome 의 '업데이트 확인' 이 무반응처럼
	// 보였음. 여기선 그 결과를 인라인으로 직접 표시.
	import { checkForUpdate, updateState, downloadAndRelaunch } from '$lib/api/updater';
	let quickMenuOpen = $state(false);
	// '업데이트 확인' 을 눌렀는지 — 눌렀을 때만 결과 토스트 표시.
	let updateRequested = $state(false);
	function runUpdateCheck() {
		incompatibleMsg = null;
		updateRequested = true;
		void checkForUpdate();
	}

	// 더 새 schema 길드 열기 시도 시 메시지 (commands.rs 의 INCOMPATIBLE_GUILD_TAG).
	const INCOMPAT_TAG = 'INCOMPATIBLE_GUILD::';
	let incompatibleMsg = $state<string | null>(null);
	// 길드 열기/초기화 에러 공통 처리 — IncompatibleGuild 면 전용 모달, 그 외는
	// 일반 메시지 반환(호출 측이 openErr/initErr 등에 대입).
	function handleOpenError(e: unknown): string | null {
		const msg = e instanceof Error ? e.message : String(e);
		if (msg.startsWith(INCOMPAT_TAG)) {
			incompatibleMsg = msg.slice(INCOMPAT_TAG.length).trim();
			return null;
		}
		return msg;
	}

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
	// DEV-053: "폴더 열기" 진행 상태.
	let pickRunning = $state(false);
	let pickErr: string | null = $state(null);

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
				const info = await invoke<{ mode: string; uninit_path: string | null }>('launch_mode');
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
			openErr = handleOpenError(e);
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
			initErr = handleOpenError(e);
		} finally {
			initRunning = false;
		}
	}

	// DEV-204: `.guild` 마커 파일을 직접 선택. 네이티브 폴더 다이얼로그는 파일을
	// 안 보여줘 "이 위치에 길드가 있나?" 판단이 어려운 문제 — 파일 피커는 `.guild`
	// 파일을 보여주므로 길드 존재가 명확히 보인다. 선택한 파일의 부모 = 길드 루트.
	async function pickGuildFile() {
		if (pickRunning) return;
		if (env !== 'tauri') {
			pickErr = 'Tauri 데스크톱 앱에서만 동작합니다.';
			return;
		}
		pickRunning = true;
		pickErr = null;
		try {
			const { open } = await import('@tauri-apps/plugin-dialog');
			const selected = await open({
				directory: false,
				multiple: false,
				title: '길드 파일(.guild) 선택',
				filters: [{ name: '길드 마커', extensions: ['guild'] }]
			});
			if (!selected) return; // 취소.
			const file = typeof selected === 'string' ? selected : selected[0];
			if (!file) return;
			// 마커 파일의 부모 디렉토리 = 길드 루트.
			const dir = file.replace(/[\\/][^\\/]+$/, '');

			const { invoke } = await import('@tauri-apps/api/core');
			const info = await invoke<{
				exists: boolean;
				is_dir: boolean;
				has_marker: boolean;
				resolved_path: string;
			}>('inspect_guild_path', { path: dir });

			if (!info.has_marker) {
				pickErr = `선택한 .guild 파일의 폴더에서 길드를 찾지 못했습니다: ${dir}`;
				return;
			}
			await invoke('open_guild_in_current_window', { path: info.resolved_path });
			goto('/');
		} catch (e) {
			pickErr = handleOpenError(e);
		} finally {
			pickRunning = false;
		}
	}

	// DEV-052 후속 (5회차): 단일 항목 제거 — 모든 항목에 × 버튼.
	// 확인 모달 거쳐서 실수 방지.
	let confirmRemove: Recent | null = $state(null);

	function askRemove(r: Recent) {
		confirmRemove = r;
	}

	function cancelRemove() {
		confirmRemove = null;
	}

	async function doRemove() {
		const target = confirmRemove;
		if (!target) return;
		confirmRemove = null;
		try {
			await recentsApi.remove(target.path);
			recents = recents.filter((r) => r.path !== target.path);
		} catch (e) {
			openErr = handleOpenError(e);
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
	<title>Welcome — openguild</title>
</svelte:head>

<main class="welcome">
	<header>
		<div class="title-row">
			<div>
				<h1>openguild</h1>
				<p class="sub">최근 작업한 길드</p>
			</div>
			<!-- DEV-052 fix → DEV-138: welcome 에서도 ⚙ 가 퀵메뉴 (Nav 와 동일).
				 Nav 가 가려져 있으므로 페이지 자체에 톱니바퀴. -->
			<div class="settings-wrap">
				<button
					class="settings-link"
					class:active={quickMenuOpen}
					onclick={() => (quickMenuOpen = !quickMenuOpen)}
					title="설정"
					aria-label="설정"
					aria-expanded={quickMenuOpen}>⚙</button
				>
				{#if quickMenuOpen}
					<SettingsQuickMenu onclose={() => (quickMenuOpen = false)} />
				{/if}
			</div>
		</div>
	</header>

	{#if env === 'tauri'}
		<!-- DEV-053: 파일 탐색기로 임의 위치의 길드 열기. -->
		<section class="picker">
			<button class="btn-pick" onclick={pickGuildFile} disabled={pickRunning}>
				{pickRunning ? '여는 중…' : '📂 길드 열기'}
			</button>
			<span class="picker-hint">
				길드 파일(<code>.guild</code>)을 고르면 그 길드를 엽니다. 길드가 있는 위치엔 탐색기에
				<code>.guild</code> 파일이 보이므로 더블클릭으로 바로 열 수 있습니다.
			</span>
			{#if pickErr}
				<p class="err">{pickErr}</p>
			{/if}
		</section>
	{/if}

	{#if uninitPath}
		<!-- DEV-052 후속: 길드 마커 없는 디렉토리에서 시작 → 초기화 prompt. -->
		<section class="uninit">
			<h2>이 위치를 길드로 초기화할까요?</h2>
			<p class="uninit-path">{uninitPath}</p>
			<p class="uninit-desc">
				지정한 디렉토리에 openguild 마커 (<code>.guild/</code> 폴더 + 시드)가 없습니다. 초기화하면 빈
				길드가 생성되어 바로 작업할 수 있습니다.
			</p>
			<label class="uninit-name">
				<span>길드 이름</span>
				<input type="text" bind:value={initName} placeholder="guild" disabled={initRunning} />
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
					<!-- DEV-052 후속 (5회차): 모든 항목에 × — 단일 삭제 + 확인 모달. -->
					<button
						class="recent-remove"
						type="button"
						onclick={() => askRemove(r)}
						title="목록에서 제거 (디스크 데이터는 건드리지 않음)"
						aria-label="목록에서 제거"
					>
						×
					</button>
				</li>
			{/each}
		</ul>
		{#if openErr}
			<p class="err">{openErr}</p>
		{/if}
		<button class="clear" onclick={askClear}>전체 비우기</button>
	{/if}

	<footer class="hint">
		<p>항목을 클릭하면 현재 창에서 그 길드를 엽니다.</p>
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

{#if confirmRemove}
	<!-- DEV-052 후속 (5회차): 단일 항목 제거 확인. -->
	<div class="ov" role="presentation">
		<div class="modal" role="dialog" aria-modal="true" tabindex="-1">
			<h3 class="modal-title">최근 길드에서 제거</h3>
			<p class="modal-msg">
				<strong>{confirmRemove.name}</strong> 을 최근 목록에서 제거할까요?
			</p>
			<p class="modal-path">{confirmRemove.path}</p>
			<p class="modal-msg modal-note">
				디스크의 길드 파일은 그대로 두고, Recent 목록에서만 빠집니다.
			</p>
			<div class="modal-actions">
				<button class="btn-yes" onclick={doRemove}>제거</button>
				<button class="btn-no" onclick={cancelRemove}>취소</button>
			</div>
		</div>
	</div>
{/if}

<!-- DEV-154: 더 새 schema 길드 — 전용 안내 + 업데이트 확인 (DEV-063). -->
<ConfirmDialog
	open={incompatibleMsg !== null}
	title="호환되지 않는 길드"
	message={incompatibleMsg ?? ''}
	confirmLabel="업데이트 확인"
	oncancel={() => (incompatibleMsg = null)}
	onconfirm={runUpdateCheck}
/>

<!-- DEV-154: 업데이트 확인 결과를 인라인으로 표시 (전역 배너가 안 덮는 상태들). -->
{#if updateRequested}
	<div class="upd-toast" role="status">
		{#if $updateState.status === 'checking'}
			<span>업데이트 확인 중…</span>
		{:else if $updateState.status === 'available'}
			<span>새 버전 {$updateState.version} 사용 가능</span>
			<button class="upd-go" onclick={() => downloadAndRelaunch()}>지금 업데이트</button>
		{:else if $updateState.status === 'downloading'}
			<span>다운로드 중… {$updateState.pct ?? ''}{$updateState.pct != null ? '%' : ''}</span>
		{:else if $updateState.status === 'ready'}
			<span>설치 완료 — 재시작 중…</span>
		{:else if $updateState.status === 'uptodate'}
			<span>이미 최신 버전입니다. (호환되는 새 버전이 아직 없을 수 있어요.)</span>
		{:else if $updateState.status === 'error'}
			<span class="upd-err">업데이트 확인 실패: {$updateState.message}</span>
		{/if}
		<button class="upd-close" onclick={() => (updateRequested = false)} title="닫기">✕</button>
	</div>
{/if}

<style>
	/* DEV-154: 업데이트 확인 결과 토스트 (하단 고정). */
	.upd-toast {
		position: fixed;
		left: 50%;
		bottom: 1.25rem;
		transform: translateX(-50%);
		z-index: 60;
		display: flex;
		align-items: center;
		gap: 0.6rem;
		max-width: min(90vw, 36rem);
		padding: 0.55rem 0.9rem;
		background: var(--bg-elevated);
		border: 1px solid var(--border);
		border-radius: 8px;
		box-shadow: 0 6px 20px var(--shadow);
		color: var(--text);
		font-size: 0.85rem;
	}
	.upd-toast .upd-err {
		color: var(--danger);
	}
	.upd-go {
		padding: 0.25rem 0.7rem;
		border-radius: 6px;
		border: 1px solid var(--btn-primary-border);
		background: var(--btn-primary-bg);
		color: var(--btn-primary-text);
		font-size: 0.8rem;
		cursor: pointer;
	}
	.upd-go:hover {
		background: var(--btn-primary-bg-hover);
	}
	.upd-close {
		margin-left: auto;
		padding: 0.1rem 0.4rem;
		background: transparent;
		border: none;
		color: var(--text-muted);
		cursor: pointer;
		font-size: 0.85rem;
	}
	.upd-close:hover {
		color: var(--text);
	}

	.welcome {
		max-width: var(--content-max-width, 720px);
		margin: 0 auto;
		padding: 2rem 1.5rem;
		color: var(--text);
	}
	header h1 {
		margin: 0;
		font-size: 2rem;
		color: var(--accent);
	}
	header .sub {
		margin: 0.25rem 0 1.5rem;
		color: var(--text-muted);
	}
	/* DEV-052 fix: welcome 의 설정 진입 — 우상단 톱니바퀴. */
	header .title-row {
		display: flex;
		align-items: flex-start;
		justify-content: space-between;
		gap: 1rem;
	}
	/* DEV-138: 퀵메뉴 anchor. */
	.settings-wrap {
		position: relative;
	}
	.settings-link {
		display: inline-flex;
		align-items: center;
		justify-content: center;
		width: 2.25rem;
		height: 2.25rem;
		border-radius: 8px;
		background: transparent;
		border: none;
		cursor: pointer;
		text-decoration: none;
		color: var(--text-muted);
		background: var(--bg-elevated);
		border: 1px solid var(--border);
		font-size: 1.1rem;
		transition:
			background 0.1s,
			color 0.1s,
			border-color 0.1s;
	}
	.settings-link:hover {
		color: var(--text);
		background: var(--bg-subtle);
		border-color: var(--text-faint);
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
		border: 1px solid var(--border);
		border-radius: 6px;
		color: var(--text-muted);
		font-size: 1.2rem;
		line-height: 1;
		cursor: pointer;
		transition:
			border-color 0.12s,
			color 0.12s,
			background 0.12s;
	}
	.recent-remove:hover {
		border-color: var(--danger);
		color: var(--danger);
		background: rgba(233, 79, 79, 0.08);
	}
	.recent-btn {
		flex: 1 1 auto;
	}
	.missing-label {
		color: var(--warning);
		font-size: 0.8rem;
	}
	.recent-btn {
		width: 100%;
		text-align: left;
		padding: 0.75rem 1rem;
		background: var(--bg-elevated);
		border: 1px solid var(--border);
		border-radius: 6px;
		color: inherit;
		font: inherit;
		cursor: pointer;
		display: grid;
		grid-template-columns: 1fr;
		gap: 0.25rem;
		transition:
			border-color 0.12s,
			background 0.12s;
	}
	.recent-list .recent-row:not(.missing) .recent-btn:hover:not(:disabled) {
		border-color: var(--accent);
		background: var(--bg-subtle);
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
		color: var(--text-muted);
		font-size: 0.85rem;
		font-family: 'SFMono-Regular', Consolas, monospace;
	}
	.recent-btn .path {
		color: var(--text-muted);
		font-size: 0.85rem;
		font-family: 'SFMono-Regular', Consolas, monospace;
		word-break: break-all;
	}
	.recent-btn .opening {
		color: var(--accent);
		font-size: 0.8rem;
	}
	.clear {
		margin-top: 1rem;
		padding: 0.4rem 0.9rem;
		background: transparent;
		color: var(--danger);
		border: 1px solid var(--border);
		border-radius: 4px;
		font-size: 0.85rem;
		cursor: pointer;
	}
	.clear:hover {
		border-color: var(--danger);
	}
	.loading,
	.empty,
	.info,
	.err {
		padding: 1rem;
		background: var(--bg-elevated);
		border-radius: 6px;
		color: var(--text-muted);
	}
	.err {
		color: var(--danger);
		margin-top: 1rem;
	}
	code {
		background: var(--bg);
		padding: 0.1rem 0.4rem;
		border-radius: 3px;
		font-family: 'SFMono-Regular', Consolas, monospace;
		font-size: 0.9em;
	}
	.hint {
		margin-top: 2rem;
		padding-top: 1rem;
		border-top: 1px solid var(--border);
		color: var(--text-muted);
		font-size: 0.85rem;
	}

	/* --- DEV-053: 폴더 열기 picker --- */
	.picker {
		display: flex;
		align-items: center;
		flex-wrap: wrap;
		gap: 0.85rem;
		margin: 0 0 1.25rem;
		padding: 0.85rem 1rem;
		background: var(--bg-elevated);
		border: 1px solid var(--border);
		border-radius: 8px;
	}
	.btn-pick {
		padding: 0.5rem 1rem;
		background: var(--accent-strong);
		border: 1px solid var(--accent);
		border-radius: 6px;
		color: var(--btn-primary-text);
		font-size: 0.9rem;
		font-weight: 500;
		cursor: pointer;
		transition: background 0.12s;
		flex: 0 0 auto;
	}
	.btn-pick:hover:not(:disabled) {
		background: var(--accent);
	}
	.btn-pick:disabled {
		opacity: 0.6;
		cursor: default;
	}
	.picker-hint {
		flex: 1 1 auto;
		min-width: 12.5rem; /* BUG-064 */
		color: var(--text-muted);
		font-size: 0.825rem;
	}
	.picker .err {
		flex: 1 0 100%;
		margin: 0;
		padding: 0.5rem 0.75rem;
		font-size: 0.85rem;
	}

	/* --- uninit prompt (DEV-052 후속 2회차) --- */
	.uninit {
		margin-bottom: 1.5rem;
		padding: 1rem 1.25rem;
		background: var(--bg-subtle);
		border: 1px solid var(--accent);
		border-radius: 8px;
	}
	.uninit h2 {
		margin: 0 0 0.5rem;
		font-size: 1.05rem;
		color: var(--accent);
	}
	.uninit-path {
		margin: 0 0 0.5rem;
		padding: 0.4rem 0.6rem;
		background: var(--bg);
		border-radius: 4px;
		font-family: 'SFMono-Regular', Consolas, monospace;
		font-size: 0.85rem;
		color: var(--text);
		word-break: break-all;
	}
	.uninit-desc {
		margin: 0 0 0.85rem;
		color: var(--text);
		font-size: 0.875rem;
	}
	.uninit-name {
		display: flex;
		align-items: center;
		gap: 0.6rem;
		margin: 0 0 0.85rem;
		font-size: 0.875rem;
		color: var(--text);
	}
	.uninit-name > span {
		flex: 0 0 auto;
		color: var(--text-muted);
	}
	.uninit-name input {
		flex: 1 1 auto;
		padding: 0.4rem 0.6rem;
		background: var(--bg);
		border: 1px solid var(--border);
		border-radius: 4px;
		color: var(--text);
		font: inherit;
		font-family: 'SFMono-Regular', Consolas, monospace;
	}
	.uninit-name input:focus {
		outline: none;
		border-color: var(--accent);
	}
	.uninit-actions {
		display: flex;
		gap: 0.5rem;
		justify-content: flex-end;
	}

	/* --- 커스텀 confirm 모달 --- */
	.ov {
		position: fixed;
		inset: 0;
		background: rgba(0, 0, 0, 0.6);
		z-index: 100;
		display: flex;
		align-items: center;
		justify-content: center;
		padding: 1rem;
	}
	.modal {
		background: var(--bg-elevated);
		border: 1px solid var(--border);
		border-radius: 10px;
		width: 100%;
		max-width: calc(26.25rem * var(--popup-scale, 1)); /* BUG-064 */
		padding: 1.2rem 1.4rem;
		box-shadow: 0 12px 36px rgba(0, 0, 0, 0.6);
		color: var(--text);
	}
	.modal-title {
		margin: 0 0 0.5rem;
		font-size: 1rem;
		font-weight: 600;
		color: var(--text-strong);
	}
	.modal-msg {
		margin: 0 0 1rem;
		font-size: 0.875rem;
		color: var(--text);
	}
	.modal-msg strong {
		color: var(--text-strong);
	}
	.modal-path {
		margin: -0.5rem 0 0.85rem;
		padding: 0.4rem 0.6rem;
		background: var(--bg);
		border-radius: 4px;
		font-family: 'SFMono-Regular', Consolas, monospace;
		font-size: 0.8rem;
		color: var(--text-muted);
		word-break: break-all;
	}
	.modal-note {
		font-size: 0.8rem;
		color: var(--text-muted);
	}
	.modal-actions {
		display: flex;
		gap: 0.5rem;
		justify-content: flex-end;
	}
	.btn-yes {
		padding: 0.4rem 1.1rem;
		background: rgba(233, 79, 79, 0.15);
		border: 1px solid var(--danger);
		border-radius: 6px;
		color: var(--danger);
		font-size: 0.875rem;
		cursor: pointer;
	}
	.btn-yes:hover {
		background: rgba(233, 79, 79, 0.25);
	}
	.btn-no {
		padding: 0.4rem 1rem;
		background: transparent;
		border: 1px solid var(--border);
		border-radius: 6px;
		color: var(--text-muted);
		font-size: 0.875rem;
		cursor: pointer;
	}
	.btn-no:hover {
		background: var(--bg-subtle);
	}
</style>
