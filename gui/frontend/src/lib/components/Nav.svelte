<script lang="ts">
	import { page } from '$app/stores';
	import { adminApi } from '$lib/api/admin';
	import { bumpReindex } from '$lib/stores/reindex';
	// DEV-125: Settings 외 빠른 접근 — Nav 에서 한 클릭으로 다음 모드 순환.
	import { theme, resolveTheme, type ThemeChoice } from '$lib/stores/theme';

	const THEME_CYCLE: ThemeChoice[] = ['system', 'light', 'dark'];
	function cycleTheme() {
		const cur = $theme;
		const idx = THEME_CYCLE.indexOf(cur);
		const next = THEME_CYCLE[(idx + 1) % THEME_CYCLE.length];
		theme.set(next);
	}
	let themeIcon = $derived.by(() => {
		if ($theme === 'light') return '☀';
		if ($theme === 'dark') return '☾';
		// system 모드 — 현재 effective 에 따라 표시.
		return resolveTheme('system') === 'light' ? '☀' : '☾';
	});
	let themeTitle = $derived.by(() => {
		const cur = $theme === 'system' ? `시스템 (${resolveTheme('system')})` : $theme;
		return `테마: ${cur} — 클릭 시 다음 (system → light → dark → system) 으로 순환`;
	});

	// DEV-011: Home 탭. URL `/` 가 ?view 없으면 home 기본.
	type View = 'home' | 'board' | 'list';

	let currentView: View = $derived(
		($page.url.searchParams.get('view') as View | null) ?? 'home'
	);

	let onAdminPath = $derived($page.url.pathname.startsWith('/admin'));
	let onRootPath = $derived($page.url.pathname === '/');
	let onSettingsPath = $derived($page.url.pathname.startsWith('/settings'));
	// DEV-016: 길드 규칙 페이지.
	let onRulesPath = $derived($page.url.pathname.startsWith('/rules'));

	// DEV-095: Reindex 버튼 — 사용자 의견 "Admin 페이지 아닌 일반 사용자도
	// 접근 가능". 외부 편집 / `openguild quest new` CLI / git pull 등으로
	// `.guild/quests/*.md` 가 바뀌었을 때 cache 정합 회복.
	type ReindexState =
		| { status: 'idle' }
		| { status: 'running' }
		| { status: 'done'; ts: number }
		| { status: 'error'; message: string };
	let reindexState = $state<ReindexState>({ status: 'idle' });

	async function runReindex() {
		reindexState = { status: 'running' };
		try {
			await adminApi.reindex();
			// DEV-095 fix: invalidateAll() 은 +page.ts 의 load() 만 트리거 — 우리
			// 페이지들은 onMount 직접 fetch 라 안 먹음. store-bump 패턴으로 페이지
			// 가 reactive subscribe.
			bumpReindex();
			reindexState = { status: 'done', ts: Date.now() };
			// 3 초 후 idle 로 — 사용자 noise 최소화.
			setTimeout(() => {
				if (reindexState.status === 'done') {
					reindexState = { status: 'idle' };
				}
			}, 3000);
		} catch (e) {
			reindexState = {
				status: 'error',
				message: e instanceof Error ? e.message : 'reindex 실패'
			};
		}
	}
</script>

<header>
	<!-- DEV-052 후속 (4회차): 로고 클릭 → Welcome (다른 길드로 전환 / recent 관리). -->
	<a href="/welcome" class="logo">openguild</a>

	<nav>
		<a href="/" class:active={onRootPath && currentView === 'home'}>Home</a>
		<a href="/?view=board" class:active={onRootPath && currentView === 'board'}>Quest Board</a>
		<a href="/?view=list" class:active={onRootPath && currentView === 'list'}>Quest List</a>
		<a href="/admin" class:active={onAdminPath}>Admin</a>
		<!-- DEV-016: 길드 규칙 — 팀 컨벤션 / 그라운드 룰. -->
		<a href="/rules" class:active={onRulesPath}>Rules</a>
	</nav>

	<div class="nav-right">
		<!-- DEV-095: 외부 편집 후 cache 정합 회복 — 일반 사용자도 한 클릭으로. -->
		<button
			class="btn-reindex"
			class:running={reindexState.status === 'running'}
			class:done={reindexState.status === 'done'}
			class:error={reindexState.status === 'error'}
			onclick={runReindex}
			disabled={reindexState.status === 'running'}
			title={reindexState.status === 'error'
				? `Reindex 실패: ${reindexState.message}`
				: reindexState.status === 'done'
					? '✓ Reindex 완료'
					: '캐시 정합 — 외부 편집 / git pull 후 한 번 클릭'}
			aria-label="Reindex"
		>
			{#if reindexState.status === 'running'}
				⟳
			{:else if reindexState.status === 'done'}
				✓
			{:else if reindexState.status === 'error'}
				⚠
			{:else}
				⟲
			{/if}
		</button>
		<!-- DEV-125: 테마 빠른 토글 — Settings 의 라디오는 그대로 유지. -->
		<button
			class="btn-theme"
			class:system={$theme === 'system'}
			onclick={cycleTheme}
			title={themeTitle}
			aria-label="테마 전환"
		>{themeIcon}{#if $theme === 'system'}<span class="sys-dot" aria-hidden="true"></span>{/if}</button>
		<!-- DEV-084: New Quest / 업데이트 버튼은 본문 / 설정 페이지로 이동.
		     우상단엔 ⚙ 설정 진입만 (정보 / 업데이트 등 비자주 기능 묶음). -->
		<a
			href="/settings"
			class="btn-settings"
			class:active={onSettingsPath}
			title="설정"
			aria-label="설정"
		>⚙</a>
	</div>
</header>

<style>
	/* DEV-074: hardcoded color → var() 마이그레이션. */
	/* DEV-101 fix5: height 52px → 3.25rem — UI scale 반영. 안 그러면 버튼 (rem) 만
	   확대돼 nav 가 자식보다 작아짐. */
	header {
		display: flex;
		align-items: center;
		gap: 2rem;
		padding: 0 1.5rem;
		height: 3.25rem;
		background: var(--nav-bg);
		border-bottom: 1px solid var(--nav-border);
		position: sticky;
		top: 0;
		z-index: 100;
	}

	.logo {
		font-size: 1.1rem;
		font-weight: 700;
		color: var(--text);
		text-decoration: none;
		letter-spacing: 0.02em;
	}

	nav {
		display: flex;
		gap: 0.25rem;
		flex: 1;
	}

	nav a {
		padding: 0.35rem 0.85rem;
		border-radius: 6px;
		font-size: 0.875rem;
		color: var(--text-muted);
		text-decoration: none;
		transition: background 0.15s, color 0.15s;
	}

	nav a:hover {
		background: var(--nav-hover-bg);
		color: var(--text);
	}

	nav a.active {
		background: var(--nav-hover-bg);
		color: var(--text);
	}

	.nav-right {
		display: flex;
		align-items: center;
		gap: 0.5rem;
		margin-left: auto;
	}

	/* DEV-125: 테마 빠른 토글. .btn-settings 와 동일 사이즈. */
	.btn-theme {
		display: inline-flex;
		align-items: center;
		justify-content: center;
		position: relative;
		width: 2rem;
		height: 2rem;
		border-radius: 6px;
		font-size: 1.05rem;
		line-height: 1;
		color: var(--text-muted);
		background: transparent;
		border: none;
		cursor: pointer;
		transition: background 0.15s, color 0.15s;
	}
	.btn-theme:hover {
		background: var(--nav-hover-bg);
		color: var(--text);
	}
	/* system 모드 표시 — 아이콘 우하단 작은 도트. */
	.sys-dot {
		position: absolute;
		right: 4px;
		bottom: 4px;
		width: 4px;
		height: 4px;
		border-radius: 50%;
		background: var(--accent);
	}

	/* DEV-084: 설정 진입 — 톱니바퀴 아이콘. 우상단 (New Quest 가 있던 자리). */
	.btn-settings {
		display: inline-flex;
		align-items: center;
		justify-content: center;
		width: 2rem;
		height: 2rem;
		border-radius: 6px;
		font-size: 1.1rem;
		line-height: 1;
		color: var(--text-muted);
		text-decoration: none;
		transition: background 0.15s, color 0.15s, transform 0.2s;
	}
	.btn-settings:hover {
		background: var(--nav-hover-bg);
		color: var(--text);
		transform: rotate(45deg);
	}
	.btn-settings.active {
		background: var(--nav-hover-bg);
		color: var(--text);
	}

	/* DEV-095: Reindex 버튼 — 설정 옆. */
	.btn-reindex {
		display: inline-flex;
		align-items: center;
		justify-content: center;
		width: 2rem;
		height: 2rem;
		border-radius: 6px;
		font-size: 1.05rem;
		line-height: 1;
		color: var(--text-muted);
		background: transparent;
		border: none;
		cursor: pointer;
		transition: background 0.15s, color 0.15s, transform 0.4s;
	}
	.btn-reindex:hover:not(:disabled) {
		background: var(--nav-hover-bg);
		color: var(--text);
	}
	.btn-reindex:disabled {
		cursor: wait;
	}
	.btn-reindex.running {
		color: var(--accent);
		animation: spin 1.2s linear infinite;
	}
	.btn-reindex.done {
		color: var(--success);
	}
	.btn-reindex.error {
		color: var(--danger);
	}
	@keyframes spin {
		from { transform: rotate(0deg); }
		to   { transform: rotate(360deg); }
	}
</style>
