<script lang="ts">
	import { page } from '$app/stores';

	type View = 'board' | 'list';

	let currentView: View = $derived(
		($page.url.searchParams.get('view') as View | null) ?? 'board'
	);
</script>

<header>
	<a href="/" class="logo">OpenGuild</a>

	<nav>
		<a href="/?view=board" class:active={currentView === 'board'}>Quest Board</a>
		<a href="/?view=list" class:active={currentView === 'list'}>Quest List</a>
	</nav>
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
		color: #ffffff;
		font-weight: 500;
	}
</style>
