<script lang="ts">
	import { page } from '$app/stores';

	// DEV-011: Home 탭. URL `/` 가 ?view 없으면 home 기본.
	type View = 'home' | 'board' | 'list';

	let currentView: View = $derived(
		($page.url.searchParams.get('view') as View | null) ?? 'home'
	);

	let onAdminPath = $derived($page.url.pathname.startsWith('/admin'));
	let onRootPath = $derived($page.url.pathname === '/');
	let onSettingsPath = $derived($page.url.pathname.startsWith('/settings'));
</script>

<header>
	<!-- DEV-052 후속 (4회차): 로고 클릭 → Welcome (다른 길드로 전환 / recent 관리). -->
	<a href="/welcome" class="logo">openguild</a>

	<nav>
		<a href="/" class:active={onRootPath && currentView === 'home'}>Home</a>
		<a href="/?view=board" class:active={onRootPath && currentView === 'board'}>Quest Board</a>
		<a href="/?view=list" class:active={onRootPath && currentView === 'list'}>Quest List</a>
		<a href="/admin" class:active={onAdminPath}>Admin</a>
	</nav>

	<div class="nav-right">
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
	header {
		display: flex;
		align-items: center;
		gap: 2rem;
		padding: 0 1.5rem;
		height: 52px;
		background: #1a1a2e;
		border-bottom: 1px solid #2a2a4a;
		position: sticky;
		top: 0;
		z-index: 100;
	}

	.logo {
		font-size: 1.1rem;
		font-weight: 700;
		color: #c9d1d9;
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
		color: #8b949e;
		text-decoration: none;
		transition: background 0.15s, color 0.15s;
	}

	nav a:hover {
		background: #2a2a4a;
		color: #c9d1d9;
	}

	nav a.active {
		background: #2a2a4a;
		color: #c9d1d9;
	}

	.nav-right {
		display: flex;
		align-items: center;
		gap: 0.5rem;
		margin-left: auto;
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
		color: #8b949e;
		text-decoration: none;
		transition: background 0.15s, color 0.15s, transform 0.2s;
	}
	.btn-settings:hover {
		background: #2a2a4a;
		color: #c9d1d9;
		transform: rotate(45deg);
	}
	.btn-settings.active {
		background: #2a2a4a;
		color: #c9d1d9;
	}
</style>
