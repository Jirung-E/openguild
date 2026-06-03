<!--
  BUG-041: DB schema 가 현재 binary 보다 새로운 경우 사용자에게 알림.

  배경 — 신규 빌드 (mig N+1) 가 사용자의 길드 DB 에 적용된 뒤, 이전 release
  binary 가 같은 DB 를 열려고 하면 schema mismatch. `set_ignore_missing(true)`
  로 panic 자체는 막지만, 사용자가 모르고 계속 옛 GUI 를 쓸 위험 → 본 banner
  로 명시 알림.

  Tauri 환경에서만 의미 있음 — HTTP 모드는 backend 가 알아서 처리.
-->
<script lang="ts">
	import { onMount } from 'svelte';
	import { detectEnvironment } from '$lib/api/transport';

	let isAhead = $state(false);
	let aheadVersions = $state<number[]>([]);
	let dismissed = $state(false);

	const DISMISS_KEY = 'openguild.schemaAheadBannerDismissed';

	async function check() {
		if (detectEnvironment() !== 'tauri') return;
		try {
			const { invoke } = await import('@tauri-apps/api/core');
			const status = (await invoke('get_db_schema_status')) as {
				is_ahead: boolean;
				ahead_versions: number[];
			};
			isAhead = status.is_ahead;
			aheadVersions = status.ahead_versions ?? [];
			// 같은 ahead 셋에 대해 한 번 닫으면 그 세션 동안 안 다시 보임 — localStorage 의 hash 비교.
			const sig = aheadVersions.join(',');
			const seen = localStorage.getItem(DISMISS_KEY);
			if (seen === sig) {
				dismissed = true;
			}
		} catch {
			// 명령 미존재 (구 backend) — 조용히 무시.
		}
	}

	function dismiss() {
		dismissed = true;
		try {
			localStorage.setItem(DISMISS_KEY, aheadVersions.join(','));
		} catch {
			/* ignore */
		}
	}

	onMount(() => {
		check();
	});
</script>

{#if isAhead && !dismissed}
	<div class="banner" role="alert">
		<div class="msg">
			<strong>⚠ DB schema 가 현재 GUI 버전보다 새롭습니다</strong>
			<span class="detail">
				이 길드 DB 에는 현재 binary 가 모르는 migration ({aheadVersions.join(', ')}) 이
				적용되어 있습니다. 최신 openguild 로 업데이트하지 않으면 일부 데이터가 보이지
				않을 수 있습니다.
			</span>
		</div>
		<button class="x" onclick={dismiss} aria-label="닫기" title="닫기">×</button>
	</div>
{/if}

<style>
	.banner {
		display: flex;
		align-items: center;
		gap: 0.75rem;
		padding: 0.5rem 1rem;
		background: color-mix(in srgb, #f0883e 18%, transparent);
		border-bottom: 1px solid #f0883e;
		color: #f0883e;
		font-size: 0.85rem;
	}
	.msg {
		display: flex;
		flex-direction: column;
		gap: 0.15rem;
		flex: 1;
	}
	.msg strong {
		font-weight: 600;
	}
	.detail {
		font-size: 0.78rem;
		color: #d29363;
	}
	.x {
		background: none;
		border: none;
		color: #f0883e;
		cursor: pointer;
		font-size: 1.1rem;
		line-height: 1;
		padding: 0 0.3rem;
	}
	.x:hover {
		color: #ffb070;
	}
</style>
