<script lang="ts">
	import { page } from '$app/stores';
	import QuestBoard from '$lib/components/QuestBoard.svelte';
	import QuestList from '$lib/components/QuestList.svelte';

	type View = 'board' | 'list';
	let currentView: View = $derived(
		($page.url.searchParams.get('view') as View | null) ?? 'board'
	);
</script>

{#if currentView === 'board'}
	<QuestBoard />
{:else}
	<QuestList />
{/if}

<style>
</style>
