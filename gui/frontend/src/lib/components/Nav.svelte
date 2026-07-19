<script lang="ts">
	import { onMount, tick } from 'svelte';
	import { page } from '$app/stores';
	import { adminApi } from '$lib/api/admin';
	import { bumpReindex } from '$lib/stores/reindex';
	// DEV-260: Nav overflow → 타이틀바 ☰ 메뉴 공유 스토어.
	import { navOverflowItems, type NavOverflowItem } from '$lib/stores/navOverflow';
	// DEV-205 모듈1: Nav 문자열 i18n.
	import { locale, t } from '$lib/stores/locale';
	// DEV-138: 설정 퀵메뉴 — ⚙ 클릭 시 dropdown (테마/UI크기/폭 + 전체 설정).
	// DEV-125 의 standalone 테마 순환 버튼은 퀵메뉴로 흡수.
	import SettingsQuickMenu from './SettingsQuickMenu.svelte';
	let quickMenuOpen = $state(false);

	// DEV-271: 검색 팔레트(전 문서 검색)와 ☰ 메뉴(캠페인/작업기록/태그)는
	// TitleBar.svelte 안에만 있는데, TitleBar 는 usesCustomTitlebar() 가
	// true 인 Tauri 데스크탑에서만 렌더된다(+layout showTitleBar). 웹
	// 배포(브라우저)에선 TitleBar 자체가 없어 이 두 기능에 접근할 방법이
	// 아예 없었다 — Nav 는 웹/앱 모두 항상 렌더되므로 여기에 웹 전용
	// fallback 을 추가한다(데스크탑엔 이미 TitleBar 로 있으니 중복 방지).
	// `guildContextActive` 로는 가드하지 않는다 — 그 플래그는 +page.svelte
	// onMount 가 `detectEnvironment() !== 'tauri'` 면 즉시 return 해버려서
	// 웹에서는 절대 true 가 안 된다(별도 버그, 범위 밖). 웹 배포는 서버가
	// 이미 특정 길드 하나에 바인딩된 상태라 "Welcome/길드 미선택" 상태
	// 자체가 없으므로 showWebExtras 만으로 충분.
	import SearchPalette from './SearchPalette.svelte';
	import { usesCustomTitlebar } from '$lib/utils/platform';
	const showWebExtras = !usesCustomTitlebar();
	let searchOpen = $state(false);

	// BUG-146: 예전엔 커스텀 타이틀바가 없는 환경(브라우저 dev / 당시엔
	// macOS/Linux 로 오판)에서 Nav 에 "openguild" 로고 + 길드명을 fallback
	// 으로 그렸으나, 이제 로고/길드명은 타이틀바(TitleBar)로 일원화. 앱(모든
	// OS)이든 웹이든 Nav 에는 아예 그리지 않으므로 관련 상태/조회/판별 전부
	// 제거했다.

	// DEV-011: Home 탭. URL `/` 가 ?view 없으면 home 기본.
	type View = 'home' | 'board' | 'list';

	let currentView: View = $derived(($page.url.searchParams.get('view') as View | null) ?? 'home');

	let onAdminPath = $derived($page.url.pathname.startsWith('/admin'));
	let onRootPath = $derived($page.url.pathname === '/');
	let onSettingsPath = $derived($page.url.pathname.startsWith('/settings'));
	// DEV-016: 길드 규칙 페이지.
	let onRulesPath = $derived($page.url.pathname.startsWith('/rules'));
	// DEV-217: 도서관 페이지.
	let onLibraryPath = $derived($page.url.pathname.startsWith('/library'));

	// DEV-260: 창 폭이 좁아 Nav 링크가 가로로 다 안 들어가면 오른쪽(우선순위
	// 낮은) 항목부터 숨기고, 숨긴 항목을 데스크탑에선 타이틀바 ☰ 메뉴로
	// (navOverflowItems 스토어), 웹(타이틀바 없음)에선 Nav 자체의 "…" 버튼
	// 드롭다운으로 옮긴다 — 브라우저 툴바의 priority+ navigation 패턴.
	//
	// 측정: 실제 링크와 동일한 마크업의 숨김 measurer 를 nav 안에 항상 전부
	// 렌더해 자연 폭을 잰다(보이는 목록은 잘려서 전체 폭을 알 수 없음).
	// ResizeObserver 를 nav(가용 폭 변화)와 measurer(라벨/폰트 변화 — 언어
	// 토글·UI 크기 변경 시 measurer 자체 크기가 바뀜) 둘 다에 걸어 반응.
	const navItems = $derived.by((): NavOverflowItem[] => {
		const items: NavOverflowItem[] = [
			{ href: '/', label: t('nav.home', $locale), active: onRootPath && currentView === 'home' },
			{ href: '/?view=board', label: t('nav.board', $locale), active: onRootPath && currentView === 'board' },
			{ href: '/?view=list', label: t('nav.list', $locale), active: onRootPath && currentView === 'list' },
			{ href: '/admin', label: t('nav.admin', $locale), active: onAdminPath },
			{ href: '/rules', label: t('nav.rules', $locale), active: onRulesPath },
			{ href: '/library', label: t('nav.library', $locale), active: onLibraryPath }
		];
		if (showWebExtras) {
			// DEV-271: 웹 전용 항목 — 데스크탑은 ☰ 메뉴에 상시 존재.
			items.push(
				{ href: '/campaigns', label: t('titlebar.menuCampaigns', $locale), active: $page.url.pathname.startsWith('/campaigns') },
				{ href: '/worklog', label: t('titlebar.menuWorklog', $locale), active: $page.url.pathname.startsWith('/worklog') },
				{ href: '/tags', label: t('titlebar.menuTags', $locale), active: $page.url.pathname.startsWith('/tags') }
			);
		}
		return items;
	});
	let navEl = $state<HTMLElement | null>(null);
	let measureEl = $state<HTMLElement | null>(null);
	let visibleCount = $state(99);
	let moreOpen = $state(false);
	const MORE_BTN_W = 34; // 웹 fallback "…" 버튼 예약 폭(px) — 대략치면 충분.

	function recomputeOverflow() {
		if (!navEl || !measureEl) return;
		const kids = Array.from(measureEl.children) as HTMLElement[];
		if (kids.length === 0) return;
		const gap = parseFloat(getComputedStyle(navEl).columnGap || '0') || 0;
		const available = navEl.clientWidth;
		let total = 0;
		const widths = kids.map((k) => k.offsetWidth);
		for (let i = 0; i < widths.length; i++) total += widths[i] + (i > 0 ? gap : 0);
		if (total <= available) {
			visibleCount = widths.length;
			return;
		}
		// 안 들어감 — 웹은 "…" 버튼 폭을 예약(데스크탑은 ☰ 가 타이틀바에 있어 0).
		const reserve = showWebExtras ? MORE_BTN_W + gap : 0;
		let used = 0;
		let fit = 0;
		for (let i = 0; i < widths.length; i++) {
			const w = widths[i] + (i > 0 ? gap : 0);
			if (used + w + reserve > available) break;
			used += w;
			fit = i + 1;
		}
		visibleCount = fit;
	}

	onMount(() => {
		const ro = new ResizeObserver(() => recomputeOverflow());
		if (navEl) ro.observe(navEl);
		if (measureEl) ro.observe(measureEl);
		// 폴백: RO 콜백은 rAF 루프로 전달되는데, 일부 환경(헤드리스/절전
		// 스로틀)에선 안 올 수 있음 — 가장 흔한 트리거(창 리사이즈)만이라도
		// 이벤트로 직접 커버.
		const onResize = () => recomputeOverflow();
		window.addEventListener('resize', onResize);
		return () => {
			ro.disconnect();
			window.removeEventListener('resize', onResize);
			// 언마운트 시 ☰ 쪽 잔여 항목 정리.
			navOverflowItems.set([]);
		};
	});
	// 라벨(언어)/항목 구성이 바뀌면 measurer DOM 반영 후 재측정.
	$effect(() => {
		void navItems;
		tick().then(recomputeOverflow);
	});
	const visibleItems = $derived(navItems.slice(0, visibleCount));
	const overflowItems = $derived(navItems.slice(visibleCount));
	// 데스크탑: 넘친 항목을 타이틀바 ☰ 로 발행. 웹: 로컬 "…" 드롭다운이 소비.
	$effect(() => {
		navOverflowItems.set(showWebExtras ? [] : overflowItems);
	});
	// 다 들어가게 되면 열려있던 "…" 드롭다운 닫기.
	$effect(() => {
		if (overflowItems.length === 0) moreOpen = false;
	});

	// DEV-095: Reindex 버튼 — 사용자 의견 "Admin 페이지 아닌 일반 사용자도
	// 접근 가능". 외부 편집 / `openguild quest new` CLI / git pull 등으로
	// `.guild/quests/*.md` 가 바뀌었을 때 cache 정합 회복.
	type ReindexState =
		| { status: 'idle' }
		| { status: 'running' }
		| { status: 'done'; ts: number }
		| { status: 'error'; message: string };
	let reindexState = $state<ReindexState>({ status: 'idle' });

	async function runReindex() {
		reindexState = { status: 'running' };
		try {
			await adminApi.reindex();
			// DEV-095 fix: invalidateAll() 은 +page.ts 의 load() 만 트리거 — 우리
			// 페이지들은 onMount 직접 fetch 라 안 먹음. store-bump 패턴으로 페이지
			// 가 reactive subscribe.
			bumpReindex();
			reindexState = { status: 'done', ts: Date.now() };
			// 3 초 후 idle 로 — 사용자 noise 최소화.
			setTimeout(() => {
				if (reindexState.status === 'done') {
					reindexState = { status: 'idle' };
				}
			}, 3000);
		} catch (e) {
			reindexState = {
				status: 'error',
				message: e instanceof Error ? e.message : t('nav.reindex.error', $locale)
			};
		}
	}
</script>

<header>
	<!-- BUG-146: 로고/길드명은 타이틀바로 일원화 — Nav 에는 그리지 않음. -->
	<!-- DEV-260: 링크 목록은 navItems 로 일원화(홈~도서관 + 웹 전용 DEV-271
	     항목) — visibleCount 만큼만 보이고 나머지는 ☰(데스크탑)/…(웹)으로. -->
	<nav bind:this={navEl}>
		{#each visibleItems as it (it.href)}
			<a href={it.href} class:active={it.active}>{it.label}</a>
		{/each}
		{#if showWebExtras && overflowItems.length > 0}
			<div class="more-wrap">
				<button
					class="btn-more"
					class:open={moreOpen}
					onclick={() => (moreOpen = !moreOpen)}
					title={t('nav.more', $locale)}
					aria-label={t('nav.more', $locale)}
					aria-expanded={moreOpen}
				>⋯</button>
				{#if moreOpen}
					<div class="more-menu">
						{#each overflowItems as it (it.href)}
							<a href={it.href} class:active={it.active} onclick={() => (moreOpen = false)}>{it.label}</a>
						{/each}
					</div>
				{/if}
			</div>
		{/if}
		<!-- 측정 전용 사본 — 항상 전부 렌더(숨김). 보이는 목록과 동일한
		     <a> 마크업/스코프 스타일이라 자연 폭이 정확히 일치. -->
		<div class="nav-measure" bind:this={measureEl} aria-hidden="true">
			{#each navItems as it (it.href)}
				<a href={it.href} tabindex="-1">{it.label}</a>
			{/each}
		</div>
	</nav>

	<div class="nav-right">
		<!-- DEV-271: 검색 팔레트 — 웹에서만(데스크탑은 TitleBar 중앙 pill로 이미 있음). -->
		{#if showWebExtras}
			<button
				class="btn-search"
				onclick={() => (searchOpen = true)}
				title={t('titlebar.search', $locale)}
				aria-label={t('titlebar.search', $locale)}
			>
				<svg width="15" height="15" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.3" stroke-linecap="round" aria-hidden="true">
					<circle cx="7" cy="7" r="4.3" />
					<path d="M10.2 10.2 14 14" />
				</svg>
			</button>
		{/if}
		<!-- DEV-095: 외부 편집 후 cache 정합 회복 — 일반 사용자도 한 클릭으로. -->
		<button
			class="btn-reindex"
			class:running={reindexState.status === 'running'}
			class:done={reindexState.status === 'done'}
			class:error={reindexState.status === 'error'}
			onclick={runReindex}
			disabled={reindexState.status === 'running'}
			title={reindexState.status === 'error'
				? `${t('nav.reindex.failed', $locale)}: ${reindexState.message}`
				: reindexState.status === 'done'
					? t('nav.reindex.done', $locale)
					: t('nav.reindex.hint', $locale)}
			aria-label="Reindex"
		>
			{#if reindexState.status === 'running'}
				⟳
			{:else if reindexState.status === 'done'}
				✓
			{:else if reindexState.status === 'error'}
				⚠
			{:else}
				⟲
			{/if}
		</button>
		<!-- DEV-084 → DEV-138: ⚙ 가 바로 페이지 이동 대신 퀵메뉴 dropdown.
		     자주 쓰는 표시 설정은 메뉴에서 즉시, 전체 설정은 링크로. -->
		<div class="settings-wrap">
			<button
				class="btn-settings"
				class:active={onSettingsPath || quickMenuOpen}
				onclick={() => (quickMenuOpen = !quickMenuOpen)}
				title={t('nav.settings', $locale)}
				aria-label={t('nav.settings', $locale)}
				aria-expanded={quickMenuOpen}>⚙</button
			>
			{#if quickMenuOpen}
				<SettingsQuickMenu onclose={() => (quickMenuOpen = false)} />
			{/if}
		</div>
	</div>
</header>

{#if searchOpen}
	<SearchPalette onclose={() => (searchOpen = false)} />
{/if}

<style>
	/* DEV-074: hardcoded color → var() 마이그레이션. */
	/* DEV-101 fix5: height 52px → 3.25rem — UI scale 반영. 안 그러면 버튼 (rem) 만
	   확대돼 nav 가 자식보다 작아짐. */
	header {
		display: flex;
		align-items: center;
		gap: 2rem;
		padding: 0 1.5rem;
		height: var(--nav-h, 3.25rem);
		background: var(--nav-bg);
		border-bottom: 1px solid var(--nav-border);
		position: sticky;
		/* 커스텀 타이틀바(Windows Tauri) 아래에 붙도록 — 없으면 0px. */
		top: var(--titlebar-h, 0px);
		z-index: 100;
	}

	nav {
		display: flex;
		gap: 0.25rem;
		flex: 1;
		/* DEV-260: 측정 사본(absolute) 기준점 + 계산 어긋난 순간의 삐져나옴 방지. */
		position: relative;
		min-width: 0;
		overflow: hidden;
	}

	nav a {
		padding: 0.35rem 0.85rem;
		border-radius: 6px;
		font-size: 0.875rem;
		color: var(--text-muted);
		text-decoration: none;
		/* DEV-260: overflow 계산은 항목 자연 폭 기준 — 줄바꿈/수축 금지. */
		flex: none;
		white-space: nowrap;
		transition:
			background 0.15s,
			color 0.15s;
	}

	/* DEV-260: 측정 전용 숨김 사본 — 흐름/화면 밖, 폭만 정확하게. */
	.nav-measure {
		position: absolute;
		visibility: hidden;
		pointer-events: none;
		display: flex;
		gap: inherit;
		left: -9999px;
		top: 0;
	}

	/* DEV-260: 웹 fallback "…" overflow 버튼 + 드롭다운. */
	.more-wrap {
		position: relative;
		flex: none;
		display: flex;
		align-items: center;
	}
	.btn-more {
		display: inline-flex;
		align-items: center;
		justify-content: center;
		width: 1.9rem;
		height: 1.9rem;
		border-radius: 6px;
		font-size: 1rem;
		line-height: 1;
		color: var(--text-muted);
		background: transparent;
		border: none;
		cursor: pointer;
	}
	.btn-more:hover,
	.btn-more.open {
		background: var(--nav-hover-bg);
		color: var(--text);
	}
	.more-menu {
		position: absolute;
		top: calc(100% + 6px);
		right: 0;
		min-width: 10rem;
		display: flex;
		flex-direction: column;
		padding: 0.3rem;
		background: var(--bg-elevated);
		border: 1px solid var(--border);
		border-radius: 8px;
		box-shadow: 0 8px 24px rgba(0, 0, 0, 0.35);
		z-index: 300;
	}
	.more-menu a {
		padding: 0.4rem 0.7rem;
		border-radius: 6px;
		font-size: 0.85rem;
		color: var(--text);
		text-decoration: none;
	}
	.more-menu a:hover {
		background: var(--nav-hover-bg);
	}
	.more-menu a.active {
		background: var(--nav-hover-bg);
		color: var(--text-strong);
	}

	nav a:hover {
		background: var(--nav-hover-bg);
		color: var(--text);
	}

	nav a.active {
		background: var(--nav-hover-bg);
		color: var(--text);
	}

	.nav-right {
		display: flex;
		align-items: center;
		gap: 0.5rem;
		margin-left: auto;
	}

	/* DEV-138: 퀵메뉴 anchor — dropdown 의 position:absolute 기준점. */
	.settings-wrap {
		position: relative;
	}

	/* DEV-084: 설정 진입 — 톱니바퀴 아이콘. DEV-138 부터 button (퀵메뉴 토글). */
	.btn-settings {
		display: inline-flex;
		align-items: center;
		justify-content: center;
		width: 2rem;
		height: 2rem;
		border-radius: 6px;
		font-size: 1.1rem;
		line-height: 1;
		color: var(--text-muted);
		background: transparent;
		border: none;
		cursor: pointer;
		text-decoration: none;
		transition:
			background 0.15s,
			color 0.15s,
			transform 0.2s;
	}
	.btn-settings:hover {
		background: var(--nav-hover-bg);
		color: var(--text);
		transform: rotate(45deg);
	}
	.btn-settings.active {
		background: var(--nav-hover-bg);
		color: var(--text);
	}

	/* DEV-271: 검색 버튼(웹 전용) — btn-settings 와 동일 규격. */
	.btn-search {
		display: inline-flex;
		align-items: center;
		justify-content: center;
		width: 2rem;
		height: 2rem;
		border-radius: 6px;
		color: var(--text-muted);
		background: transparent;
		border: none;
		cursor: pointer;
		transition:
			background 0.15s,
			color 0.15s;
	}
	.btn-search:hover {
		background: var(--nav-hover-bg);
		color: var(--text);
	}

	/* DEV-095: Reindex 버튼 — 설정 옆. */
	.btn-reindex {
		display: inline-flex;
		align-items: center;
		justify-content: center;
		width: 2rem;
		height: 2rem;
		border-radius: 6px;
		font-size: 1.05rem;
		line-height: 1;
		color: var(--text-muted);
		background: transparent;
		border: none;
		cursor: pointer;
		transition:
			background 0.15s,
			color 0.15s,
			transform 0.4s;
	}
	.btn-reindex:hover:not(:disabled) {
		background: var(--nav-hover-bg);
		color: var(--text);
	}
	.btn-reindex:disabled {
		cursor: wait;
	}
	.btn-reindex.running {
		color: var(--accent);
		animation: spin 1.2s linear infinite;
	}
	.btn-reindex.done {
		color: var(--success);
	}
	.btn-reindex.error {
		color: var(--danger);
	}
	@keyframes spin {
		from {
			transform: rotate(0deg);
		}
		to {
			transform: rotate(360deg);
		}
	}
</style>
