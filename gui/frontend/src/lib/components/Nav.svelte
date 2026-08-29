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
	// DEV-276: 최근 본 문서 — 버튼만 여기, 목록은 검색 팔레트의 recent 모드
	// (DEV-294). 자체 드롭다운이 없어졌으므로 제목 조회(recentDocTitle /
	// questIndexNs)는 팔레트가 담당한다 — 여기서는 인덱스 선로드만.
	import { goto } from '$app/navigation';
	import { recentDocs } from '$lib/stores/recentDocs';
	import { loadQuestIndex } from '$lib/stores/questIndex';
	let recentOpen = $state(false);
	// 라우트가 바뀌면 닫기 — $page 구독으로 자동 반응.
	$effect(() => {
		void $page.url.pathname;
		void $page.url.search;
		recentOpen = false;
	});

	// DEV-271(사용자 피드백): 웹의 검색/추가/최근 버튼을 데스크탑 TitleBar 와
	// **같은 배치**로 — 검색 pill 은 항상 화면 중앙, 그 좌우 바깥에 '퀘스트
	// 추가'(왼쪽)와 '최근 본 문서'(오른쪽) 아이콘 버튼. 이전엔 오른쪽 끝에
	// 작은 돋보기 버튼 + 자체 드롭다운이라 데스크탑과 완전히 달랐다.
	import NewQuestModal from './NewQuestModal.svelte';
	import { metaApi } from '$lib/api/meta';
	let newQuestOpen = $state(false);
	// pill 라벨 = 길드 이름. TitleBar 는 `$guildContextActive` 로 가드하지만
	// 웹에선 그 플래그가 절대 true 가 안 되므로(위 주석) 무조건 조회한다.
	let guildName = $state('');
	let isRemote = $state(false);
	$effect(() => {
		if (!showWebExtras) return;
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
	// BUG-186: 너비에 따라 접히는 "핵심" 항목만 — 캠페인/작업기록/태그는
	// 여기 안 넣는다(아래 alwaysOverflowItems). 측정(measureEl)도 이
	// 배열만 대상으로 하므로, 창이 아무리 넓어도 그 3개는 폭 계산에
	// 끼지 않고 항상 접힌 메뉴로만 간다 — TitleBar.svelte(데스크탑) 의
	// "하드코딩 3개 + 응답형 나머지" 패턴과 동일하게 맞춤.
	const navItems = $derived.by((): NavOverflowItem[] => [
		{ href: '/', label: t('nav.home', $locale), active: onRootPath && currentView === 'home' },
		{
			href: '/?view=board',
			label: t('nav.board', $locale),
			active: onRootPath && currentView === 'board'
		},
		{
			href: '/?view=list',
			label: t('nav.list', $locale),
			active: onRootPath && currentView === 'list'
		},
		{ href: '/rules', label: t('nav.rules', $locale), active: onRulesPath },
		{ href: '/library', label: t('nav.library', $locale), active: onLibraryPath }
	]);
	// DEV-271: 웹 전용 — 데스크탑은 TitleBar.svelte 의 ☰ 메뉴에 상시 존재
	// (하드코딩). 웹은 타이틀바가 없어 이 Nav 의 접힌 메뉴에 상시 존재.
	const alwaysOverflowItems = $derived.by((): NavOverflowItem[] =>
		showWebExtras
			? [
					// BUG-201(admin 지시): 어드민도 캠페인/작업기록/태그처럼 창 폭과
					// 무관하게 항상 접힌 메뉴 안에 둔다 — 자주 쓰는 곳이 아니다.
					{ href: '/admin', label: t('nav.admin', $locale), active: onAdminPath },
					{
						href: '/campaigns',
						label: t('titlebar.menuCampaigns', $locale),
						active: $page.url.pathname.startsWith('/campaigns')
					},
					{
						href: '/worklog',
						label: t('titlebar.menuWorklog', $locale),
						active: $page.url.pathname.startsWith('/worklog')
					},
					{
						href: '/tags',
						label: t('titlebar.menuTags', $locale),
						active: $page.url.pathname.startsWith('/tags')
					}
				]
			: []
	);
	let navEl = $state<HTMLElement | null>(null);
	let measureEl = $state<HTMLElement | null>(null);
	let visibleCount = $state(99);
	let moreOpen = $state(false);
	const MORE_BTN_W = 34; // 웹 fallback ☰ 버튼 예약 폭(px) — 대략치면 충분.
	// 중앙 pill 그룹의 왼쪽 끝을 재기 위한 참조 — 절대 배치라 레이아웃에는
	// 영향을 주지 않으므로 nav 의 clientWidth 로는 겹침을 알 수 없다.
	let addWrapEl = $state<HTMLElement | null>(null);
	let pillEl = $state<HTMLElement | null>(null);

	/**
	 * nav 링크가 실제로 쓸 수 있는 가로 폭.
	 *
	 * DEV-271(사용자 피드백 "검색팔레트랑 위치가 겹쳐지는 메뉴는 접기"):
	 * 검색 pill 은 화면 중앙에 **절대 배치**돼 흐름에서 빠져 있어 nav 의
	 * clientWidth 를 줄이지 않는다. 그래서 링크가 pill 아래로 파고들어 겹쳤다.
	 * pill 그룹의 실측 왼쪽 끝까지를 가용 폭으로 삼아, 겹칠 항목이 자연히
	 * ☰ 로 접히게 한다(CSS 상수를 JS 에 복제하지 않으려고 rect 로 측정).
	 */
	function availableNavWidth(gap: number): number {
		if (!navEl) return 0;
		const navRect = navEl.getBoundingClientRect();
		let limit = navRect.right;
		for (const el of [addWrapEl, pillEl]) {
			if (!el) continue;
			const r = el.getBoundingClientRect();
			// 폭 0(미렌더/숨김) 요소는 좌표가 의미 없으므로 제외.
			if (r.width > 0) limit = Math.min(limit, r.left);
		}
		return Math.max(0, Math.min(navEl.clientWidth, limit - navRect.left - gap));
	}

	function recomputeOverflow() {
		if (!navEl || !measureEl) return;
		const kids = Array.from(measureEl.children) as HTMLElement[];
		if (kids.length === 0) return;
		const gap = parseFloat(getComputedStyle(navEl).columnGap || '0') || 0;
		const available = availableNavWidth(gap);
		let total = 0;
		const widths = kids.map((k) => k.offsetWidth);
		for (let i = 0; i < widths.length; i++) total += widths[i] + (i > 0 ? gap : 0);
		if (total <= available) {
			visibleCount = widths.length;
			return;
		}
		// 안 들어감 — 웹은 ☰ 버튼 폭을 예약(데스크탑은 ☰ 가 타이틀바에 있어 0).
		// 버튼이 nav **밖**(header 자식)이라 nav 폭에 이미 반영되지만, 버튼은
		// overflowItems 가 생긴 뒤에 나타나므로 첫 계산 시점엔 아직 없다 —
		// 그 폭을 미리 빼 두지 않으면 나타났다 사라졌다 하며 진동한다.
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
	// BUG-185(사용자 보고, 웹): "…" 접힌 메뉴가 열린 채로 바깥을 클릭해도
	// 안 닫힘 — 드롭다운 안 링크 클릭 시 닫는 핸들러만 있고 바깥 클릭
	// 핸들러 자체가 없었음. TitleBar.svelte 의 ☰ 메뉴와 동일 패턴.
	function onWindowMouseDown(e: MouseEvent) {
		if (!moreOpen) return;
		const t = e.target as HTMLElement;
		if (!t.closest('.more-wrap')) moreOpen = false;
	}
	onMount(() => {
		window.addEventListener('mousedown', onWindowMouseDown, { capture: true });
		return () => window.removeEventListener('mousedown', onWindowMouseDown, { capture: true });
	});
	// 라벨(언어)/항목 구성이 바뀌면 measurer DOM 반영 후 재측정.
	$effect(() => {
		void navItems;
		tick().then(recomputeOverflow);
	});
	// 중앙 pill 그룹은 **절대 배치**라 등장하거나 폭이 바뀌어도 nav/measurer 의
	// 크기가 변하지 않는다 → onMount 의 ResizeObserver 로는 절대 안 잡힌다.
	// 게다가 pill 은 길드 이름을 받아온 뒤에야 렌더되므로 `bind:this` 가
	// onMount 시점엔 null 이다(BUG-157 과 같은 함정). 실측했더니 최초 계산이
	// pill 없는 상태로 굳어 링크가 전부 ☰ 로 접힌 채였다 — 창을 리사이즈해야
	// 정상이 됐다. refs/길드명을 의존성으로 추적해 재계산하고, pill 자체에도
	// RO 를 걸어 폭 변화(긴 이름·vw 변화)에 반응한다.
	// BUG-187(사용자 보고: "좁은 창에서 퀘스트 추가/최근 문서 버튼이 검색
	// pill 과 너무 가까워지다 못해 pill 안으로 들어감"): 버튼 오프셋을
	// pill 실제 폭과 무관한 `min(21vw, 134px)` 근사치로 계산했었음 — pill
	// 은 `min-width:260px` 라 좁은 뷰포트에서도 260px 밑으로 안 줄어드는데,
	// `21vw` 는 계속 줄어들어(뷰포트 620px 미만이면 130px 밑으로) 버튼이
	// pill 의 실제 절반 폭보다 안쪽으로 파고들었다. pill 을 실측해 그 절반
	// 폭을 CSS 변수로 흘려보내 항상 정확히 pill 가장자리 밖에 위치하게 함.
	let pillHalfWidth = $state(134);
	function updatePillHalfWidth() {
		if (pillEl) pillHalfWidth = pillEl.offsetWidth / 2;
	}
	$effect(() => {
		void guildName;
		const els = [addWrapEl, pillEl].filter((e): e is HTMLElement => !!e);
		tick().then(() => {
			recomputeOverflow();
			updatePillHalfWidth();
		});
		if (els.length === 0) return;
		const ro = new ResizeObserver(() => {
			recomputeOverflow();
			updatePillHalfWidth();
		});
		for (const el of els) ro.observe(el);
		return () => ro.disconnect();
	});
	const visibleItems = $derived(navItems.slice(0, visibleCount));
	// BUG-186: 너비 때문에 넘친 핵심 항목 + 항상 접히는 3개(웹 전용, 데스크탑은
	// 빈 배열) — 순서상 항상-접힘 항목이 뒤에 오도록(핵심 항목의 접힘 순서를
	// 안 흔들기 위해).
	const overflowItems = $derived([...navItems.slice(visibleCount), ...alwaysOverflowItems]);
	// 데스크탑: 넘친 핵심 항목만 타이틀바 ☰ 로 발행(항상-접힘 3개는 거기서
	// 이미 하드코딩돼 있어 중복 발행 금지). 웹: 로컬 "…" 드롭다운이 overflowItems 전체 소비.
	$effect(() => {
		navOverflowItems.set(showWebExtras ? [] : navItems.slice(visibleCount));
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
	<!-- BUG-186(사용자 요청): 웹은 접힌 메뉴 버튼을 header 맨 왼쪽에 — 데스크탑
	     타이틀바의 ☰(항상 좌측 고정) 위치와 맞춤. 원래 <nav> 뒤에 있었는데
	     그러면 링크 목록 오른쪽에 붙어 "맨 왼쪽" 요구와 안 맞았음.
	     DEV-271(당시 이유였던 "클릭해도 안펼쳐짐"): nav 의 overflow:hidden
	     클리핑을 피해야 하므로 여전히 nav 형제(밖)로 유지 — 대신 별도
	     wrapper(.nav-group)로 묶어 header 의 큰 gap(2rem, 다음 섹션과의
	     간격용) 대신 좁은 자체 gap 을 준다(사용자 지적: "간격이 너무 넓음").
	-->
	<div class="nav-group">
		{#if showWebExtras && overflowItems.length > 0}
			<div class="more-wrap">
				<button
					class="btn-more"
					class:open={moreOpen}
					onclick={() => (moreOpen = !moreOpen)}
					title={t('nav.more', $locale)}
					aria-label={t('nav.more', $locale)}
					aria-expanded={moreOpen}>☰</button
				>
				{#if moreOpen}
					<div class="more-menu">
						{#each overflowItems as it (it.href)}
							<a href={it.href} class:active={it.active} onclick={() => (moreOpen = false)}
								>{it.label}</a
							>
						{/each}
					</div>
				{/if}
			</div>
		{/if}

		<!-- DEV-260: 링크 목록은 navItems 로 일원화(홈~도서관) — visibleCount
		     만큼만 보이고 나머지는 ☰(데스크탑)/…(웹)으로. BUG-186: 캠페인/
		     작업기록/태그는 여기 안 들어감 — 항상 접힌 메뉴(위 more-wrap). -->
		<nav bind:this={navEl}>
			{#each visibleItems as it (it.href)}
				<a href={it.href} class:active={it.active}>{it.label}</a>
			{/each}
			<!-- 측정 전용 사본 — 항상 전부 렌더(숨김). 보이는 목록과 동일한
			     <a> 마크업/스코프 스타일이라 자연 폭이 정확히 일치. -->
			<div class="nav-measure" bind:this={measureEl} aria-hidden="true">
				{#each navItems as it (it.href)}
					<a href={it.href} tabindex="-1">{it.label}</a>
				{/each}
			</div>
		</nav>
	</div>

	<!-- DEV-271(사용자 피드백): 데스크탑 TitleBar 의 중앙 배치를 그대로 —
	     폭 0 앵커를 화면 중앙에 두고 pill 은 그 중앙, 좌우 버튼은 pill 바깥에
	     절대 배치. 버튼 유무가 pill 위치를 흔들지 않는다(BUG-158 과 동일 이유). -->
	{#if showWebExtras && guildName}
		<div class="nav-center" style="--pill-half-w: {pillHalfWidth}px">
			<!-- DEV-286: 검색 pill 왼쪽 바깥 '퀘스트 추가'. -->
			<div class="nav-add-wrap" bind:this={addWrapEl}>
				<button
					class="nav-icon-btn"
					onclick={() => (newQuestOpen = true)}
					title={t('titlebar.newQuest', $locale)}
					aria-label={t('titlebar.newQuest', $locale)}
				>
					<svg class="tb-ico"
						width="15"
						height="15"
						viewBox="0 0 16 16"
						fill="none"
						stroke="currentColor"
						stroke-width="1.5"
						stroke-linecap="round"
						aria-hidden="true"
					>
						<path d="M8 3.3v9.4M3.3 8h9.4" />
					</svg>
				</button>
			</div>
			<button
				class="nav-pill"
				class:open={searchOpen}
				bind:this={pillEl}
				onclick={() => (searchOpen = true)}
				title={t('titlebar.search', $locale)}
			>
				<span class="nav-pill-name">{guildName}</span>
				{#if isRemote}
					<span class="nav-remote" title={t('nav.remoteConnected', $locale)}>
						<svg class="tb-ico-sm"
							width="13"
							height="13"
							viewBox="0 0 16 16"
							fill="none"
							stroke="currentColor"
							stroke-width="1.2"
							aria-hidden="true"
						>
							<circle cx="8" cy="8" r="5.7" />
							<ellipse cx="8" cy="8" rx="2.4" ry="5.7" />
							<path d="M2.5 8h11" />
						</svg>
					</span>
				{/if}
			</button>
			<!-- DEV-276/294: 최근 본 문서 — 전용 드롭다운 대신 검색 팔레트의
			     recent 모드(데스크탑과 동일). -->
			{#if $recentDocs.length > 0}
				<div class="nav-recent-wrap">
					<button
						class="nav-icon-btn"
						class:active={recentOpen}
						onclick={() => {
							recentOpen = true;
							void loadQuestIndex();
						}}
						title={t('titlebar.recent', $locale)}
						aria-label={t('titlebar.recent', $locale)}
					>
						<svg class="tb-ico"
							width="15"
							height="15"
							viewBox="0 0 16 16"
							fill="none"
							stroke="currentColor"
							stroke-width="1.3"
							stroke-linecap="round"
							stroke-linejoin="round"
							aria-hidden="true"
						>
							<circle cx="8" cy="8" r="5.6" />
							<path d="M8 4.6V8l2.4 1.5" />
						</svg>
					</button>
				</div>
			{/if}
		</div>
	{/if}

	<div class="nav-right">
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

<!-- DEV-294: 최근 본 문서 = 같은 팔레트의 recent 모드(데스크탑과 동일). -->
{#if recentOpen}
	<SearchPalette mode="recent" onclose={() => (recentOpen = false)} />
{/if}

<!-- DEV-286: 웹에서도 어느 화면에서든 퀘스트 생성. -->
{#if newQuestOpen}
	<NewQuestModal
		onclose={() => (newQuestOpen = false)}
		oncreated={(quest) => goto('/quests/' + quest.quest_id)}
	/>
{/if}

<style>
	/* BUG-246 후속: 아이콘 크기.
	   SVG 의 `width` / `height` **속성**은 px 단위라 UI 배율(root font-size)을
	   따라가지 않는다 — 버튼만 커지고 안의 아이콘은 그대로였다. 속성은 CSS 보다
	   우선순위가 낮으므로(presentation attribute) 여기서 덮으면 된다. 속성은
	   그대로 두어 viewBox 비율과 CSS 미적용 시 폴백을 유지한다. */
	.tb-ico {
		width: 0.9375rem;
		height: 0.9375rem;
	}
	.tb-ico-sm {
		width: 0.8125rem;
		height: 0.8125rem;
	}

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
		border-bottom: var(--bw) solid var(--nav-border);
		/* 타이틀바와 함께 뷰포트에 고정한다(BUG: macOS rubber-band).
		   sticky 면 맨 위에서 위로 더 스크롤할 때 문서와 같이 내려오고,
		   타이틀바만 fixed 로 두면 그 사이가 벌어져 띠가 생긴다 — 크롬은
		   한 덩어리로 붙어 있어야 한다. 본문만 이 아래에서 출렁인다. */
		position: fixed;
		/* 커스텀 타이틀바(Windows Tauri) 아래에 붙도록 — 없으면 0px. */
		top: var(--titlebar-h, 0px);
		left: 0;
		right: 0;
		z-index: 100;
	}

	/* BUG-186: more-wrap + nav 를 묶어서 header 의 큰 gap(2rem, 다음 섹션과의
	   간격) 대신 좁은 자체 gap 사용. header 안에서의 flex-grow 몫은 nav 가
	   아니라 이 wrapper 가 가져간다(이전엔 nav 자체가 flex:1). */
	.nav-group {
		display: flex;
		align-items: center;
		gap: 0.5rem;
		flex: 1;
		min-width: 0;
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
		border-radius: var(--r-md);
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
		border-radius: var(--r-md);
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
		/* BUG-186: 버튼이 header 맨 왼쪽으로 이동 — right:0 이면 팝업이 화면
		   밖(왼쪽)으로 나갈 수 있어 left:0 으로 오른쪽으로 펼침. */
		left: 0;
		min-width: 10rem;
		display: flex;
		flex-direction: column;
		padding: 0.3rem;
		background: var(--bg-elevated);
		border: var(--bw) solid var(--border);
		border-radius: var(--r-lg);
		box-shadow: 0 8px 24px rgba(0, 0, 0, 0.35);
		z-index: 300;
	}
	.more-menu a {
		padding: 0.4rem 0.7rem;
		border-radius: var(--r-md);
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
		border-radius: var(--r-md);
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

	/* DEV-271(사용자 피드백): 중앙 검색 pill + 좌우 아이콘 버튼 — 데스크탑
	   TitleBar 의 .tb-center / .tb-search / .tb-icon-btn 과 같은 배치.
	   폭 0 앵커를 중앙에 두고 pill 은 그 중앙, 버튼은 pill 바깥에 절대 배치해
	   버튼 유무가 pill 위치를 흔들지 않게 한다(BUG-158). */
	.nav-center {
		position: absolute;
		left: 50%;
		transform: translateX(-50%);
		display: flex;
		align-items: center;
		justify-content: center;
		width: 0;
	}
	.nav-add-wrap {
		position: absolute;
		left: 50%;
		/* BUG-187: vw 근사 대신 JS 로 실측한 pill 절반 폭(--pill-half-w) 사용 —
		   pill 의 min-width/max-width/내용 길이와 무관하게 항상 가장자리 밖. */
		/* 간격 4px = 0.25rem — pill 과 버튼 사이 여백도 배율을 따라간다. */
		transform: translateX(calc(-1 * var(--pill-half-w, 134px) - 100% - 0.25rem));
		display: inline-flex;
		align-items: center;
		flex: none;
	}
	.nav-recent-wrap {
		position: absolute;
		left: 50%;
		transform: translateX(var(--pill-half-w, 134px));
		display: inline-flex;
		align-items: center;
		flex: none;
	}
	/* BUG-244: 이 줄(길드 pill + 좌우 아이콘 버튼)은 UI 크기 조절(DEV-101 —
	   root font-size 배율)을 따라야 한다. 높이는 rem 인데 패딩·모서리·아이콘이
	   px 라 배율을 올리면 글자만 커지고 상자는 그대로였다. 아래 규칙들의 px 는
	   전부 rem 환산(16px 기준): 12px=0.75rem / 6px=0.375rem / 13px=0.8125rem /
	   15px=0.9375rem. 보드 노드 칩처럼 자체 확대/축소가 있는 요소는 대상이 아니다. */
	.nav-pill {
		display: inline-flex;
		align-items: center;
		justify-content: center;
		position: relative;
		height: 1.55rem;
		/* 260px = 16.25rem — 배율을 올리면 이름 글자는 커지는데 pill 최소 폭만
		   그대로라 여백이 줄어 보였다. */
		min-width: 16.25rem;
		max-width: 42vw;
		padding: 0 0.75rem;

		/* BUG-192: 좁은 화면에서 pill 의 min-width(260px) 가 양옆 공간을 다 먹어,
		   pill 바깥에 절대 배치되는 ＋(퀘스트 추가)·시계(최근 문서)가 ☰·⚙ 위로
		   겹쳐 그려졌다. 필요한 자리(양옆 아이콘 3개 + 여백 ≈ 12rem)를 남기고
		   pill 이 줄어들게 한다. */
		@media (max-width: 640px) {
			min-width: 0;
			max-width: calc(100vw - 12rem);
		}
		flex: none;
		background: var(--bg-subtle);
		border: var(--bw) solid var(--nav-border);
		border-radius: var(--r-md);
		color: var(--text-muted);
		cursor: pointer;
	}
	.nav-pill:hover,
	.nav-pill.open {
		border-color: var(--accent);
		color: var(--text);
	}
	.nav-pill-name {
		font-size: 0.72rem;
		letter-spacing: 0.02em;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.nav-remote {
		position: absolute;
		right: 0.5rem;
		display: inline-flex;
		color: var(--text-muted);
	}
	.nav-icon-btn {
		display: inline-flex;
		align-items: center;
		justify-content: center;
		width: 1.7rem;
		height: 1.55rem;
		border-radius: var(--r-md);
		color: var(--text-muted);
		background: transparent;
		border: none;
		cursor: pointer;
		transition:
			background 0.15s,
			color 0.15s;
	}
	/* 아이콘 크기는 마크업의 width/height 속성(px)이라 배율을 안 탄다 — CSS 로 덮는다. */
	.nav-icon-btn svg {
		width: 0.9375rem;
		height: 0.9375rem;
	}
	.nav-pill svg {
		width: 0.8125rem;
		height: 0.8125rem;
	}
	.nav-icon-btn:hover,
	.nav-icon-btn.active {
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
		border-radius: var(--r-md);
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
