<!--
  커스텀 타이틀바 (vscode 식) — Windows 전용 (tauri.windows.conf.json 의
  decorations:false 와 세트, 호출측 +layout 이 플랫폼 판별).

  배경: 네이티브 타이틀바는 setTheme 동기화가 OS/WebView2 타이밍에 따라
  간헐적으로 어긋났음(admin 보고). HTML/CSS 타이틀바는 테마 토큰을 그대로
  따르므로 다크/라이트/커스텀 테마 전환이 항상 즉시 반영된다.

  구성 (DEV-253 후속 — 사용자 디자인 확정):
   - 왼쪽: 앱 아이콘(장식) · 홈(Welcome) · 뒤로/앞으로 · 메뉴(☰)
   - 중앙: 길드 이름 pill = 전 문서 검색 팔레트 열기(+ 원격 배지)
   - 오른쪽: 최소화 / 최대화(복원) / 닫기
   - 나머지 빈 영역: data-tauri-drag-region (창 드래그, 더블클릭 최대화)

  메뉴(☰)에는 메뉴바(Nav)에 노출하지 않은 페이지를 모음:
  캠페인 목록 / 작업기록 / 태그 목록.
-->
<script lang="ts">
	import { onMount } from 'svelte';
	import { goto, afterNavigate } from '$app/navigation';
	import { metaApi } from '$lib/api/meta';
	import { guildContextActive } from '$lib/stores/guildSession';
	// DEV-205: 타이틀바 툴팁/메뉴 i18n.
	import { locale, t } from '$lib/stores/locale';
	import SearchPalette from './SearchPalette.svelte';
	// DEV-255: 자식윈도우(검색 팔레트 "새 창으로 열기")는 단일 문서 보기라
	// 뒤로/앞으로·☰메뉴·검색 팔레트가 필요 없음 — 판정에 따라 숨김.
	import { isChildWindow } from '$lib/stores/windowKind';

	let maximized = $state(false);

	// 길드 이름 / 원격 여부 — Nav 와 동일 소스(guildContextActive 구독).
	let guildName = $state('');
	let isRemote = $state(false);
	$effect(() => {
		if (!$guildContextActive) {
			guildName = '';
			isRemote = false;
			return;
		}
		metaApi
			.getGuildDisplayInfo()
			.then((info) => {
				guildName = info.name;
				isRemote = info.remote;
			})
			.catch(() => {
				guildName = '';
				isRemote = false;
			});
	});

	let menuOpen = $state(false);
	let searchOpen = $state(false);

	// 라우트 이동 시 메뉴 닫기.
	afterNavigate(() => {
		menuOpen = false;
	});

	onMount(() => {
		let disposed = false;
		let unlisten: (() => void) | null = null;
		(async () => {
			try {
				const { getCurrentWindow } = await import('@tauri-apps/api/window');
				const win = getCurrentWindow();
				maximized = await win.isMaximized();
				// 더블클릭/스냅/Win+화살표 등 버튼 밖 경로로 바뀌어도 아이콘이
				// 따라오도록 리사이즈 이벤트 구독.
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

	// 메뉴 바깥 클릭 시 닫기.
	function onWindowClick(e: MouseEvent) {
		if (!menuOpen) return;
		const t = e.target as HTMLElement;
		if (!t.closest('.tb-menu-wrap')) menuOpen = false;
	}
</script>

<svelte:window onclick={onWindowClick} />

<div class="titlebar" data-tauri-drag-region>
	<!-- 앱 아이콘 — 장식(클릭 무동작). 드래그 영역의 일부. -->
	<img class="tb-appicon" src="/title-icon.png" alt="" data-tauri-drag-region />

	<!-- DEV-255: 자식윈도우(단일 문서 보기)는 홈/뒤로·앞으로/☰메뉴 전부 불필요
	     — 다른 곳으로 이동할 일이 없는 창이라 통째로 숨김. -->
	{#if !$isChildWindow}
	<div class="tb-left">
		<button class="tb-icon-btn" onclick={() => goto('/welcome')} title={t('titlebar.welcome', $locale)} aria-label={t('titlebar.welcome', $locale)}>
			<svg width="15" height="15" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.2" stroke-linejoin="round" aria-hidden="true">
				<path d="M2 8.2 L8 2.6 L14 8.2" />
				<path d="M3.6 7 V13.4 H12.4 V7" />
			</svg>
		</button>
		<button class="tb-icon-btn" onclick={() => history.back()} title={t('titlebar.back', $locale)} aria-label={t('titlebar.back', $locale)}>
			<svg width="15" height="15" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
				<path d="M10 3 L5.5 8 L10 13" />
			</svg>
		</button>
		<button class="tb-icon-btn" onclick={() => history.forward()} title={t('titlebar.forward', $locale)} aria-label={t('titlebar.forward', $locale)}>
			<svg width="15" height="15" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
				<path d="M6 3 L10.5 8 L6 13" />
			</svg>
		</button>
		{#if $guildContextActive}
		<div class="tb-menu-wrap">
			<button
				class="tb-icon-btn"
				class:active={menuOpen}
				onclick={() => (menuOpen = !menuOpen)}
				title={t('titlebar.menu', $locale)}
				aria-label={t('titlebar.menu', $locale)}
				aria-expanded={menuOpen}
			>
				<svg width="15" height="15" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.3" stroke-linecap="round" aria-hidden="true">
					<path d="M2.5 4.5h11M2.5 8h11M2.5 11.5h11" />
				</svg>
			</button>
			{#if menuOpen}
				<div class="tb-menu">
					<button onclick={() => goto('/campaigns')}>
						<svg width="15" height="15" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
							<rect x="2.5" y="2.5" width="11" height="11" rx="1.5" />
							<path d="M5 6h6M5 8.5h6M5 11h3.5" />
						</svg>
						{t('titlebar.menuCampaigns', $locale)}
					</button>
					<button onclick={() => goto('/worklog')}>
						<svg width="15" height="15" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
							<circle cx="8" cy="8" r="5.5" />
							<path d="M8 4.8V8l2.2 1.6" />
						</svg>
						{t('titlebar.menuWorklog', $locale)}
					</button>
					<button onclick={() => goto('/tags')}>
						<svg width="15" height="15" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
							<path d="M8.3 2.5H3.2A.7.7 0 0 0 2.5 3.2v5.1a1 1 0 0 0 .3.7l4.9 4.9a1 1 0 0 0 1.4 0l4.4-4.4a1 1 0 0 0 0-1.4L9 2.8a1 1 0 0 0-.7-.3Z" />
							<circle cx="5.4" cy="5.4" r=".9" />
						</svg>
						{t('titlebar.menuTags', $locale)}
					</button>
				</div>
			{/if}
		</div>
		{/if}
	</div>
	{/if}

	<!-- 중앙: 길드 이름 pill = 검색 팔레트. 길드 컨텍스트 있을 때만, 자식윈도우
	     에선 숨김(단일 문서 보기 창엔 전체 검색이 무의미). -->
	{#if $guildContextActive && guildName && !$isChildWindow}
		<button class="tb-search" class:open={searchOpen} onclick={() => (searchOpen = true)} title={t('titlebar.search', $locale)}>
			<span class="tb-search-name">{guildName}</span>
			{#if isRemote}
				<span class="tb-remote" title={t('nav.remoteConnected', $locale)}>
					<svg width="13" height="13" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.2" aria-hidden="true">
						<circle cx="8" cy="8" r="5.7" />
						<ellipse cx="8" cy="8" rx="2.4" ry="5.7" />
						<path d="M2.5 8h11" />
					</svg>
				</span>
			{/if}
		</button>
	{/if}

	<div class="tb-spacer" data-tauri-drag-region></div>

	<div class="tb-controls">
		<button class="tb-btn" onclick={() => winCtl('min')} title={t('titlebar.minimize', $locale)} aria-label={t('titlebar.minimize', $locale)}>
			<svg width="10" height="10" viewBox="0 0 10 10" aria-hidden="true">
				<line x1="0" y1="5" x2="10" y2="5" stroke="currentColor" stroke-width="1" />
			</svg>
		</button>
		<button
			class="tb-btn"
			onclick={() => winCtl('max')}
			title={maximized ? t('titlebar.restore', $locale) : t('titlebar.maximize', $locale)}
			aria-label={maximized ? t('titlebar.restore', $locale) : t('titlebar.maximize', $locale)}
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
		<button class="tb-btn tb-close" onclick={() => winCtl('close')} title={t('titlebar.close', $locale)} aria-label={t('titlebar.close', $locale)}>
			<svg width="10" height="10" viewBox="0 0 10 10" aria-hidden="true">
				<line x1="0" y1="0" x2="10" y2="10" stroke="currentColor" stroke-width="1" />
				<line x1="10" y1="0" x2="0" y2="10" stroke="currentColor" stroke-width="1" />
			</svg>
		</button>
	</div>
</div>

{#if searchOpen}
	<SearchPalette onclose={() => (searchOpen = false)} />
{/if}

<style>
	.titlebar {
		position: sticky;
		top: 0;
		z-index: 1100; /* Nav(100) 위 */
		height: var(--titlebar-h, 32px);
		display: flex;
		align-items: center;
		background: var(--nav-bg);
		/* 타이틀바-메뉴바 구분선 없음 — 같은 배경으로 한 면처럼 이어짐. */
		user-select: none;
		-webkit-user-select: none;
	}
	.tb-appicon {
		width: 16px;
		height: 16px;
		margin: 0 6px 0 10px;
		flex: none;
		-webkit-user-drag: none;
		user-select: none;
	}
	.tb-left {
		display: flex;
		align-items: center;
		gap: 2px;
		flex: none;
	}
	.tb-icon-btn {
		width: 30px;
		height: 26px;
		display: inline-flex;
		align-items: center;
		justify-content: center;
		color: var(--text-muted);
		background: transparent;
		border: none;
		border-radius: 5px;
		cursor: pointer;
	}
	.tb-icon-btn:hover {
		background: var(--nav-hover-bg);
		color: var(--text);
	}
	.tb-icon-btn.active {
		background: var(--nav-hover-bg);
		color: var(--text);
	}
	/* ── ☰ 메뉴 드롭다운 ── */
	.tb-menu-wrap {
		position: relative;
	}
	.tb-menu {
		position: absolute;
		top: 30px;
		left: 0;
		width: 172px;
		background: var(--bg-elevated);
		border: 1px solid var(--border);
		border-radius: 8px;
		box-shadow: 0 10px 34px rgba(0, 0, 0, 0.45);
		padding: 0.3rem;
		z-index: 1200;
	}
	.tb-menu button {
		display: flex;
		align-items: center;
		gap: 0.55rem;
		width: 100%;
		padding: 0.4rem 0.6rem;
		border-radius: 5px;
		font-size: 0.82rem;
		color: var(--text);
		background: transparent;
		border: none;
		cursor: pointer;
		text-align: left;
	}
	.tb-menu button svg {
		flex: none;
		color: var(--text-muted);
	}
	.tb-menu button:hover {
		background: var(--nav-hover-bg);
	}
	.tb-menu button:hover svg {
		color: var(--text);
	}
	/* ── 중앙 검색 pill ── */
	.tb-search {
		position: absolute;
		left: 50%;
		transform: translateX(-50%);
		display: inline-flex;
		align-items: center;
		justify-content: center;
		height: 22px;
		min-width: 260px;
		max-width: 42%;
		padding: 0 12px;
		background: rgba(255, 255, 255, 0.05);
		border: 1px solid var(--nav-border);
		border-radius: 6px;
		color: var(--text-muted);
		cursor: pointer;
	}
	.tb-search:hover,
	.tb-search.open {
		background: rgba(255, 255, 255, 0.09);
		color: var(--text);
	}
	.tb-search-name {
		font-size: 0.72rem;
		letter-spacing: 0.02em;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.tb-remote {
		position: absolute;
		right: 8px;
		display: inline-flex;
		color: var(--text-muted);
	}
	.tb-spacer {
		flex: 1;
		height: 100%;
	}
	/* ── 창 컨트롤 ── */
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
