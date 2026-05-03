<script lang="ts">
	import { page } from '$app/stores';

	let id = $derived(Number($page.params.id));
</script>

<div class="container">
	<a href="/" class="back">← 뒤로</a>
	<div class="placeholder">Quest Detail #{id} — 준비 중</div>
</div>

<style>
	.container {
		padding: 1.5rem;
	}

	.back {
		display: inline-block;
		margin-bottom: 1.5rem;
		font-size: 0.875rem;
		color: #8b949e;
		text-decoration: none;
	}

	.back:hover {
		color: #c9d1d9;
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
