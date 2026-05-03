<script lang="ts">
	import { page } from '$app/stores';

	type View = 'board' | 'list';
	let currentView: View = $derived(
		($page.url.searchParams.get('view') as View | null) ?? 'board'
	);
</script>

<div class="container">
	{#if currentView === 'board'}
		<div class="placeholder">Quest Board — 준비 중</div>
	{:else}
		<div class="placeholder">Quest List — 준비 중</div>
	{/if}
</div>

<style>
	.container {
		padding: 1.5rem;
	}

	.placeholder {
		display: flex;
		align-items: center;
		justify-content: center;
		height: 60vh;
		color: #484f58;
		font-size: 1rem;
	}
</style>
