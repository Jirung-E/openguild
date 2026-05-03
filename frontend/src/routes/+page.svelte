<script lang="ts">
	import { page } from '$app/stores';
	import QuestList from '$lib/components/QuestList.svelte';

	type View = 'board' | 'list';
	let currentView: View = $derived(
		($page.url.searchParams.get('view') as View | null) ?? 'board'
	);
</script>

{#if currentView === 'board'}
	<div class="placeholder">Quest Board — 준비 중</div>
{:else}
	<QuestList />
{/if}

<style>
	.placeholder {
		display: flex;
		align-items: center;
		justify-content: center;
		height: calc(100vh - 52px);
		color: #484f58;
		font-size: 1rem;
	}
</style>
