<script lang="ts">
	import { onMount } from 'svelte';
	import { recentsApi, type Recent } from '$lib/api/recents';
	import { detectEnvironment } from '$lib/api/transport';

	let recents: Recent[] = $state([]);
	let loading = $state(true);
	let err: string | null = $state(null);
	let confirmOpen = $state(false); // DEV-052 후속: 브라우저 confirm 대신 커스텀 모달.
	let opening: string | null = $state(null); // 현재 spawn 중인 path (UI 깜빡임 방지).
	let openErr: string | null = $state(null);
	const env = detectEnvironment();

	onMount(async () => {
		try {
			recents = await recentsApi.list();
		} catch (e) {
			err = e instanceof Error ? e.message : String(e);
		} finally {
			loading = false;
		}
	});

	async function openRecent(path: string) {
		// DEV-052 후속: recent 항목 클릭 → 새 openguild-gui 프로세스를 그 path 로 spawn.
		// Tauri 환경에서만 동작.
		if (env !== 'tauri') return;
		if (opening) return;
		opening = path;
		openErr = null;
		try {
			const { invoke } = await import('@tauri-apps/api/core');
			await invoke('open_guild_in_new_window', { path });
			// 성공: 새 창이 떴음. 현재 welcome 창은 사용자가 닫게 둠.
			//   spawn 후 자동 종료를 원하면 tauri-plugin-process 의 exit() 호출,
			//   현재는 한 화면에 머무를 수 있도록 자동 종료 안 함.
		} catch (e) {
			openErr = e instanceof Error ? e.message : String(e);
		} finally {
			opening = null;
		}
	}

	function askClear() {
		// DEV-052 후속: 브라우저 confirm() 은 OS 스타일 — 어울리지 않음. 커스텀 모달.
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
				<li>
					<button
						class="recent-btn"
						type="button"
						onclick={() => openRecent(r.path)}
						disabled={opening !== null}
						title="새 창에서 이 길드를 엽니다"
					>
						<div class="row">
							<span class="name">{r.name}</span>
							<span class="last">{fmtDate(r.last_opened)}</span>
						</div>
						<div class="path">{r.path}</div>
						{#if opening === r.path}
							<div class="opening">새 창에서 여는 중…</div>
						{/if}
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
		<p>
			항목을 클릭하면 새 OpenGuild 창이 그 길드로 열립니다. (현재 welcome 창은 직접 닫아 주세요.)
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
	.recent-list li { display: contents; }
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
	.recent-btn:hover:not(:disabled) {
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
