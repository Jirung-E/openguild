<!--
  커스텀 타이틀바 (vscode 식) — Windows 전용 (tauri.windows.conf.json 의
  decorations:false 와 세트, 호출측 +layout 이 플랫폼 판별).

  배경: 네이티브 타이틀바는 setTheme 동기화가 OS/WebView2 타이밍에 따라
  간헐적으로 어긋났음(admin 보고). HTML/CSS 타이틀바는 테마 토큰을 그대로
  따르므로 다크/라이트/커스텀 테마 전환이 항상 즉시 반영된다.

  - 드래그: data-tauri-drag-region (더블클릭 최대화 토글은 Tauri 가 내장
    처리 — allow-internal-toggle-maximize 권한).
  - 창 컨트롤: 최소화/최대화(복원)/닫기 직접 구현.
  - 알려진 트레이드오프: Windows 11 Snap Layouts(최대화 버튼 호버 팝업)
    미지원 — Win+화살표/가장자리 드래그 스냅은 그대로 동작. 복원은 후속
    (플러그인/Win32 트릭) 검토.
-->
<script lang="ts">
	import { onMount } from 'svelte';

	let maximized = $state(false);

	onMount(() => {
		let disposed = false;
		let unlisten: (() => void) | null = null;
		(async () => {
			try {
				const { getCurrentWindow } = await import('@tauri-apps/api/window');
				const win = getCurrentWindow();
				maximized = await win.isMaximized();
				// 최대화 상태 아이콘(□/❐) 동기화 — 더블클릭/스냅/Win+화살표 등
				// 버튼 밖 경로로 바뀌어도 따라오도록 리사이즈 이벤트 구독.
				const un = await win.onResized(async () => {
					maximized = await win.isMaximized();
				});
				if (disposed) un();
				else unlisten = un;
			} catch {
				/* 브라우저 모드 등 — 호출측이 걸러주지만 방어 */
			}
		})();
		return () => {
			disposed = true;
			unlisten?.();
		};
	});

	async function winCtl(action: 'min' | 'max' | 'close') {
		try {
			const { getCurrentWindow } = await import('@tauri-apps/api/window');
			const win = getCurrentWindow();
			if (action === 'min') await win.minimize();
			else if (action === 'max') await win.toggleMaximize();
			else await win.close();
		} catch {
			/* 방어 */
		}
	}
</script>

<div class="titlebar" data-tauri-drag-region>
	<span class="tb-title" data-tauri-drag-region>openguild</span>
	<div class="tb-controls">
		<button class="tb-btn" onclick={() => winCtl('min')} title="최소화" aria-label="최소화">
			<svg width="10" height="10" viewBox="0 0 10 10" aria-hidden="true">
				<line x1="0" y1="5" x2="10" y2="5" stroke="currentColor" stroke-width="1" />
			</svg>
		</button>
		<button
			class="tb-btn"
			onclick={() => winCtl('max')}
			title={maximized ? '이전 크기로 복원' : '최대화'}
			aria-label={maximized ? '이전 크기로 복원' : '최대화'}
		>
			{#if maximized}
				<svg width="10" height="10" viewBox="0 0 10 10" aria-hidden="true">
					<rect x="0" y="2.5" width="7" height="7" fill="none" stroke="currentColor" stroke-width="1" />
					<path d="M 2.5 2.5 V 0.5 H 9.5 V 7.5 H 7.5" fill="none" stroke="currentColor" stroke-width="1" />
				</svg>
			{:else}
				<svg width="10" height="10" viewBox="0 0 10 10" aria-hidden="true">
					<rect x="0.5" y="0.5" width="9" height="9" fill="none" stroke="currentColor" stroke-width="1" />
				</svg>
			{/if}
		</button>
		<button class="tb-btn tb-close" onclick={() => winCtl('close')} title="닫기" aria-label="닫기">
			<svg width="10" height="10" viewBox="0 0 10 10" aria-hidden="true">
				<line x1="0" y1="0" x2="10" y2="10" stroke="currentColor" stroke-width="1" />
				<line x1="10" y1="0" x2="0" y2="10" stroke="currentColor" stroke-width="1" />
			</svg>
		</button>
	</div>
</div>

<style>
	.titlebar {
		position: sticky;
		top: 0;
		z-index: 1100; /* Nav(100) 위 */
		height: var(--titlebar-h, 32px);
		display: flex;
		align-items: center;
		background: var(--nav-bg);
		border-bottom: 1px solid var(--nav-border);
		user-select: none;
		-webkit-user-select: none;
	}
	.tb-title {
		padding: 0 0.9rem;
		font-size: 0.75rem;
		color: var(--text-muted);
		letter-spacing: 0.02em;
		flex: 1;
		/* pointer-events 는 살려둠 — data-tauri-drag-region 이 mousedown 을
		   받아야 드래그가 됨. 텍스트 선택 방지는 상위 user-select 가 담당. */
	}
	.tb-controls {
		display: flex;
		height: 100%;
		flex: none;
	}
	.tb-btn {
		width: 46px;
		height: 100%;
		border: none;
		background: transparent;
		color: var(--text-muted);
		display: inline-flex;
		align-items: center;
		justify-content: center;
		cursor: default; /* 네이티브 창 버튼 관례 — pointer 아님 */
	}
	.tb-btn:hover {
		background: var(--nav-hover-bg);
		color: var(--text);
	}
	.tb-close:hover {
		background: var(--danger);
		color: var(--btn-primary-text);
	}
</style>
