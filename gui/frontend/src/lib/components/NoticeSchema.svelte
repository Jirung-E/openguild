<!--
  길드 DB 가 이 앱보다 새로울 때의 경고 카드 안 할 일 — "최신 버전 받기"(릴리스 페이지를 시스템 브라우저로).
  예전엔 따로 그린 경고 카드였고, 이제 알림 카드 안에 둔다(DEV-486 과 같은 틀). 카드로 누름 · 키를 올려 보내지 않는다.
-->
<script lang="ts">
	import Icon from './Icon.svelte';

	const RELEASE_URL = 'https://github.com/Jirung-E/openguild/releases/latest';

	// button 이라 layout 의 링크 가로채기를 안 타서 직접 연다.
	async function openRelease() {
		try {
			const { openUrl } = await import('@tauri-apps/plugin-opener');
			await openUrl(RELEASE_URL);
		} catch {
			try {
				window.open(RELEASE_URL, '_blank');
			} catch {
				/* ignore */
			}
		}
	}
</script>

<!-- 카드(열기)로 누름 · 키를 올려 보내지 않는 칸일 뿐이라 역할이 없다. -->
<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="ns" onclick={(e) => e.stopPropagation()} onkeydown={(e) => e.stopPropagation()}>
	<button class="btn btn-warning btn-sm" onclick={openRelease}>
		최신 버전 받기 <Icon name="external" size="1em" />
	</button>
</div>

<style>
	.ns {
		display: flex;
		margin-top: var(--sp-6);
		cursor: default;
	}
</style>
