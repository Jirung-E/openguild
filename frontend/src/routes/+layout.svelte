<script lang="ts">
	import Nav from '$lib/components/Nav.svelte';
	import '$lib/styles/global.css';

	let { children } = $props();
</script>

<Nav />
<main>
	{@render children()}
</main>

<style>
	main {
		min-height: calc(100vh - 52px);
		background: #0d1117;
	}
</style>
