<script lang="ts">
	import { onMount } from 'svelte';
	import { recentsApi, type Recent } from '$lib/api/recents';
	import { detectEnvironment } from '$lib/api/transport';

	let recents: Recent[] = $state([]);
	let loading = $state(true);
	let err: string | null = $state(null);
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

	async function handleClear() {
		if (!confirm('Recent 목록을 모두 비울까요?')) return;
		await recentsApi.clear();
		recents = [];
	}

	function fmtDate(iso: string): string {
		// ISO → "YYYY-MM-DD HH:MM" 로컬 표시.
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
					<div class="name">{r.name}</div>
					<div class="path">{r.path}</div>
					<div class="last">{fmtDate(r.last_opened)}</div>
				</li>
			{/each}
		</ul>
		<button class="clear" onclick={handleClear}>전체 비우기</button>
	{/if}

	<footer class="hint">
		<p>
			길드를 클릭해서 여는 기능은 아직 미구현 — 현재는 <code>openguild-gui &lt;path&gt;</code>로 재실행하세요.
			(<a href="https://example.invalid/dev-006-followup">runtime swap</a> 후속 quest 예정.)
		</p>
	</footer>
</main>

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
	.recent-list li {
		padding: 0.75rem 1rem;
		background: #161b22;
		border: 1px solid #30363d;
		border-radius: 6px;
		display: grid;
		grid-template-columns: 1fr auto;
		gap: 0.25rem;
		align-items: baseline;
	}
	.recent-list .name {
		font-weight: 600;
		font-size: 1.05rem;
	}
	.recent-list .last {
		color: #8b95a1;
		font-size: 0.85rem;
		font-family: 'SFMono-Regular', Consolas, monospace;
	}
	.recent-list .path {
		grid-column: 1 / -1;
		color: #8b95a1;
		font-size: 0.85rem;
		font-family: 'SFMono-Regular', Consolas, monospace;
		word-break: break-all;
	}
	.clear {
		margin-top: 1rem;
		padding: 0.4rem 0.9rem;
		background: transparent;
		color: #e94f4f;
		border: 1px solid #30363d;
		border-radius: 4px;
		font-size: 0.85rem;
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
</style>
