<!--
  커스텀 타이틀바 (vscode 식) — Windows/Linux 는 각 tauri.{platform}.conf.json
  의 decorations:false 와 세트, 호출측 +layout 이 usesCustomTitlebar() 로
  판별. BUG-140: Linux 도 커스텀 사용.
  DEV-265: macOS 는 tauri.macos.conf.json 의 titleBarStyle:"Overlay" 로
  네이티브 traffic light 는 그대로 두고 이 컴포넌트가 옆/뒤까지 확장 —
  창 컨트롤 버튼 마크업은 렌더링하지 않음(traffic light 가 이미 있음).

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
	import NewQuestModal from './NewQuestModal.svelte';
	// DEV-255: 자식윈도우(검색 팔레트 "새 창으로 열기")는 단일 문서 보기라
	// 뒤로/앞으로·☰메뉴·검색 팔레트가 필요 없음 — 판정에 따라 숨김.
	import { isChildWindow } from '$lib/stores/windowKind';
	// DEV-265: macOS 는 tauri.macos.conf.json 의 titleBarStyle:"Overlay" 로
	// 네이티브 traffic light 를 그대로 두고 이 컴포넌트가 그 옆/뒤까지 확장
	// — 유일하게 "버튼은 네이티브, 나머지는 커스텀"이 성립하는 플랫폼이라
	// 창 컨트롤 버튼 마크업 자체를 렌더링하지 않는다(traffic light 가 이미
	// 그 자리에 있음).
	import { isMacOverlay } from '$lib/utils/platform';
	// DEV-260: 창 폭이 좁아 메뉴바(Nav)에서 넘친 페이지 링크 — ☰ 메뉴 상단에
	// 구분선과 함께 렌더(priority+ navigation). Nav 가 폭 변화에 반응해 발행.
	import { navOverflowItems } from '$lib/stores/navOverflow';
	// DEV-276: 최근 본 문서 목록 — 검색 pill 옆 시계 버튼.
	// BUG-159: 제목은 cross-link 인덱스에서 조회(저장값 아님) — 목록을 열 때
	// 인덱스가 아직 없으면 조용히 적재(이미 있으면 memo 라 비용 0).
	import { recentDocs } from '$lib/stores/recentDocs';
	import { loadQuestIndex } from '$lib/stores/questIndex';

	let maximized = $state(false);
	const isMac = isMacOverlay();

	// BUG-187: 검색 pill 좌우 버튼 오프셋을 pill 실제 폭과 무관한
	// `min(21vw, 134px)` 근사치로 계산했음 — pill 은 min-width:260px 라
	// 좁은 창에서도 안 줄어드는데 21vw 항은 계속 줄어들어 버튼이 pill
	// 가장자리 안쪽으로 파고든다(Nav.svelte 웹 메뉴바에서 먼저 발견·수정된
	// 것과 동일 버그). 데스크탑은 tauri.*.conf.json 의 minWidth:800 때문에
	// 지금은 발현되지 않지만, 나중에 최소폭 제약이 바뀌면 똑같이 터지므로
	// 선제적으로 같은 방식(pill 실측 + CSS 변수)으로 고쳐둔다.
	let tbSearchEl = $state<HTMLElement | null>(null);
	let pillHalfWidth = $state(134);

	// DEV-265: 창 컨트롤 버튼을 OS 관례에 맞춤. decorations:false 상태에선
	// 버튼 픽셀 자체를 OS(DWM/GTK)가 그려주는 API 가 없음 — WinUI3
	// ExtendsContentIntoTitleBar 조차 "프레임 제거 후 자체적으로 캡션 버튼을
	// 그린다"(MS 공식 문서 확인). Windows Terminal/VSCode 도 동일 한계.
	// 그래서:
	// - Windows: 아이콘 "모양"만큼은 OS가 실제로 쓰는 폰트 글리프(Segoe Fluent
	//   Icons / Segoe MDL2 Assets)를 그대로 사용 — 손으로 그린 SVG 근사가
	//   아니라 OS 네이티브 캡션 버튼과 동일한 글리프. 간격/폭도 네이티브
	//   캡션 버튼 규격(간격 0, 고정 폭)에 맞춤.
	// - Linux: 실행 중인 GTK 아이콘 테마에서 실제 아이콘(data URL)과 버튼
	//   순서/간격을 백엔드(get_native_titlebar_style)로 조회해 그대로 렌더 —
	//   조회 실패 시(비-GNOME 세션 등) 기존 Adwaita 근사 CSS로 폴백.
	const isLinuxControls =
		typeof navigator !== 'undefined' &&
		navigator.userAgent.includes('Linux') &&
		!navigator.userAgent.includes('Android');

	// DEV-265: 네이티브 GNOME 타이틀버튼 자연 크기는 ~34px 인데, 커스텀
	// 타이틀바 높이(--titlebar-h)보다 크면 안 들어가므로 상한. 아이콘/간격은
	// 사용자 피드백에 맞춰 조정(원 크기는 유지, 아이콘 키우고 간격 넓힘).
	const LINUX_BTN_CAP = 24; // 원형 버튼 지름 상한(px)
	const LINUX_ICON_RATIO = 0.65; // 버튼 대비 아이콘 크기 비율
	const LINUX_BTN_GAP = 13; // 버튼 사이 간격(px)

	// Windows 네이티브 캡션 버튼과 동일한 Segoe Fluent Icons/Segoe MDL2 Assets
	// 코드포인트(ChromeMinimize/ChromeMaximize/ChromeRestore/ChromeClose).
	// String.fromCharCode 로 만들어 소스 파일에 눈에 보이지 않는 PUA
	// (Private Use Area) 문자가 그대로 박히는 걸 피한다.
	const winIcon = {
		min: String.fromCharCode(0xe921),
		max: String.fromCharCode(0xe922),
		restore: String.fromCharCode(0xe923),
		close: String.fromCharCode(0xe8bb)
	};

	// DEV-265(리눅스): 실제 GTK 아이콘 테마 조회 결과. null 이면 아직 못
	// 받아왔거나 조회 실패 — CSS 근사(Adwaita 흉내)로 폴백.
	type LinuxTitlebarStyle = {
		minIcon: string | null; // data:image/... URL
		maxIcon: string | null;
		restoreIcon: string | null;
		closeIcon: string | null;
		side: 'left' | 'right'; // gsettings button-layout 의 배치 쪽
		order: Array<'min' | 'max' | 'close'>; // 왼쪽→오른쪽
		gapPx: number | null;
		buttonSizePx: number | null;
	};
	let linuxStyle = $state<LinuxTitlebarStyle | null>(null);

	// DEV-265: 리눅스에서 시스템 설정(button-layout)이 창 컨트롤을 왼쪽에
	// 두면 우리도 왼쪽에 그린다. 기본/조회 실패 시 오른쪽.
	const controlsOnLeft = $derived(isLinuxControls && linuxStyle?.side === 'left');

	if (isLinuxControls && typeof window !== 'undefined') {
		import('@tauri-apps/api/core')
			.then(({ invoke }) => invoke<LinuxTitlebarStyle>('get_native_titlebar_style'))
			.then((s) => (linuxStyle = s))
			.catch(() => {
				/* 폴백: 기존 CSS 근사 유지 */
			});
	}

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
	// DEV-286: 타이틀바에서 전역 퀘스트 생성.
	let newQuestOpen = $state(false);
	// DEV-276: 최근 본 문서 드롭다운.
	let recentOpen = $state(false);

	// 라우트 이동 시 메뉴 닫기.
	afterNavigate(() => {
		menuOpen = false;
		recentOpen = false;
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
					await reportMaxButtonRect();
				});
				if (disposed) un();
				else unlisten = un;
			} catch {
				/* 브라우저 모드 등 — 호출측이 걸러주지만 방어 */
			}
		})();
		// DEV-265(Windows): 마운트 시 1 회 최대화 버튼 위치를 등록해둬야
		// 리사이즈 전에도 Snap Layout 호버가 바로 동작.
		void reportMaxButtonRect();
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

	// DEV-265(Windows): 최대화 버튼의 화면 좌표를 Rust 쪽에 알려줘
	// WM_NCHITTEST 에서 HTMAXBUTTON 을 리턴하게 함 — 진짜 OS Snap Layout
	// 호버가 뜨도록. Linux/macOS 는 no-op(백엔드에 해당 command 없음).
	let maxBtnEl = $state<HTMLButtonElement | null>(null);
	async function reportMaxButtonRect() {
		if (isLinuxControls || !maxBtnEl) return;
		try {
			const r = maxBtnEl.getBoundingClientRect();
			// Rust 쪽은 ScreenToClient 로 얻은 물리 픽셀 클라이언트 좌표와
			// 비교하므로 devicePixelRatio 를 반영해 물리 픽셀로 변환해서 보낸다.
			const dpr = window.devicePixelRatio || 1;
			const { invoke } = await import('@tauri-apps/api/core');
			await invoke('set_maximize_hit_rect', {
				x: Math.round(r.left * dpr),
				y: Math.round(r.top * dpr),
				width: Math.round(r.width * dpr),
				height: Math.round(r.height * dpr)
			});
		} catch {
			/* 방어 — Windows 아닌 빌드엔 이 command 자체가 없음 */
		}
	}

	// DEV-255 후속(사용자 요청): 자식윈도우 전용 pin — Always on top 토글.
	// 문서 창을 다른 작업 위에 띄워놓고 참조하는 사용 흐름.
	let pinned = $state(false);
	async function togglePin() {
		try {
			const { getCurrentWindow } = await import('@tauri-apps/api/window');
			await getCurrentWindow().setAlwaysOnTop(!pinned);
			pinned = !pinned;
		} catch {
			/* 브라우저 모드 등 — 방어 */
		}
	}

	// 메뉴 바깥 클릭 시 닫기.
	//
	// BUG-155: 이전엔 window 'click'(버블) 리스너였는데, Tauri 가 주입하는
	// drag-region 스크립트가 타이틀바(data-tauri-drag-region) 위 mousedown
	// 에서 stopImmediatePropagation() 을 호출 + 창 드래그를 시작해버려
	// 타이틀바 빈 영역 클릭으로는 이벤트가 window 까지 오지 않았다(드래그가
	// 시작되면 click 자체가 발생하지 않기도 함) — ☰ 메뉴가 안 닫히던 원인.
	// SearchPalette(DEV-255 4차)와 동일하게 window 'mousedown' 을 **capture**
	// 단계로 등록 — capture 는 window→document→target 순서라 drag 스크립트
	// (document 버블)보다 먼저 실행되고 stopImmediatePropagation 의 영향도
	// 받지 않는다.
	function onWindowMouseDown(e: MouseEvent) {
		if (!menuOpen) return;
		const t = e.target as HTMLElement;
		if (!t.closest('.tb-menu-wrap')) menuOpen = false;
	}
	onMount(() => {
		window.addEventListener('mousedown', onWindowMouseDown, { capture: true });
		return () => window.removeEventListener('mousedown', onWindowMouseDown, { capture: true });
	});

	// BUG-187: pill 폭이 바뀔 때마다(콘텐츠 길이·창 크기) 절반 폭을 실측.
	$effect(() => {
		if (!tbSearchEl) return;
		const el = tbSearchEl;
		const update = () => (pillHalfWidth = el.offsetWidth / 2);
		update();
		const ro = new ResizeObserver(update);
		ro.observe(el);
		return () => ro.disconnect();
	});
</script>

<svelte:window onresize={reportMaxButtonRect} />

<div class="titlebar" class:mac-overlay={isMac} data-tauri-drag-region>
	<!-- DEV-265: 시스템이 창 컨트롤을 왼쪽에 두는 배치면 타이틀바 맨 앞에
	     렌더(그 외엔 spacer 뒤 오른쪽). -->
	{#if controlsOnLeft}{@render winControls()}{/if}
	<!-- macOS: 네이티브 traffic light(빨/노/초) 폭만큼 좌측 여백 — CSS 로
	     처리(.titlebar.mac-overlay padding-left). traffic light 옆에 또
	     아이콘을 붙이면 중복돼 보여서 macOS 는 렌더하지 않음. -->
	<!-- 앱 아이콘 — 장식(클릭 무동작). 드래그 영역의 일부. -->
	{#if !isMac}
		<img class="tb-appicon" src="/title-icon.png" alt="" data-tauri-drag-region />
	{/if}

	<!-- DEV-255: 자식윈도우(단일 문서 보기)는 홈/뒤로·앞으로/☰메뉴 전부 불필요
	     — 다른 곳으로 이동할 일이 없는 창이라 통째로 숨김. -->
	{#if !$isChildWindow}
		<div class="tb-left">
			<button
				class="tb-icon-btn"
				onclick={() => goto('/welcome')}
				title={t('titlebar.welcome', $locale)}
				aria-label={t('titlebar.welcome', $locale)}
			>
				<svg
					width="15"
					height="15"
					viewBox="0 0 16 16"
					fill="none"
					stroke="currentColor"
					stroke-width="1.2"
					stroke-linejoin="round"
					aria-hidden="true"
				>
					<path d="M2 8.2 L8 2.6 L14 8.2" />
					<path d="M3.6 7 V13.4 H12.4 V7" />
				</svg>
			</button>
			<button
				class="tb-icon-btn"
				onclick={() => history.back()}
				title={t('titlebar.back', $locale)}
				aria-label={t('titlebar.back', $locale)}
			>
				<svg
					width="15"
					height="15"
					viewBox="0 0 16 16"
					fill="none"
					stroke="currentColor"
					stroke-width="1.5"
					stroke-linecap="round"
					stroke-linejoin="round"
					aria-hidden="true"
				>
					<path d="M10 3 L5.5 8 L10 13" />
				</svg>
			</button>
			<button
				class="tb-icon-btn"
				onclick={() => history.forward()}
				title={t('titlebar.forward', $locale)}
				aria-label={t('titlebar.forward', $locale)}
			>
				<svg
					width="15"
					height="15"
					viewBox="0 0 16 16"
					fill="none"
					stroke="currentColor"
					stroke-width="1.5"
					stroke-linecap="round"
					stroke-linejoin="round"
					aria-hidden="true"
				>
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
						<svg
							width="15"
							height="15"
							viewBox="0 0 16 16"
							fill="none"
							stroke="currentColor"
							stroke-width="1.3"
							stroke-linecap="round"
							aria-hidden="true"
						>
							<path d="M2.5 4.5h11M2.5 8h11M2.5 11.5h11" />
						</svg>
					</button>
					{#if menuOpen}
						<div class="tb-menu">
							<!-- DEV-260: Nav 에서 넘친 페이지 링크 — 상시 항목과 구분선으로 구획. -->
							{#if $navOverflowItems.length > 0}
								{#each $navOverflowItems as it (it.href)}
									<button class:active={it.active} onclick={() => goto(it.href)}>
										{it.label}
									</button>
								{/each}
								<div class="tb-menu-sep"></div>
							{/if}
							<!-- BUG-201(admin 지시): 어드민은 창 폭과 무관하게 항상 이 메뉴 안에.
					     Nav 의 응답형 목록에서 빠졌으므로 여기 상시 항목으로 둔다
					     (데스크탑·웹 양쪽에서 같은 자리). -->
							<button onclick={() => goto('/admin')}>
								<svg
									width="15"
									height="15"
									viewBox="0 0 16 16"
									fill="none"
									stroke="currentColor"
									stroke-width="1.2"
									stroke-linecap="round"
									stroke-linejoin="round"
									aria-hidden="true"
								>
									<path d="M2.5 4.5h11M2.5 8h11M2.5 11.5h11" />
									<circle cx="5.5" cy="4.5" r="1.4" />
									<circle cx="10.5" cy="8" r="1.4" />
									<circle cx="6.5" cy="11.5" r="1.4" />
								</svg>
								{t('nav.admin', $locale)}
							</button>
							<button onclick={() => goto('/campaigns')}>
								<svg
									width="15"
									height="15"
									viewBox="0 0 16 16"
									fill="none"
									stroke="currentColor"
									stroke-width="1.2"
									stroke-linecap="round"
									stroke-linejoin="round"
									aria-hidden="true"
								>
									<rect x="2.5" y="2.5" width="11" height="11" rx="1.5" />
									<path d="M5 6h6M5 8.5h6M5 11h3.5" />
								</svg>
								{t('titlebar.menuCampaigns', $locale)}
							</button>
							<button onclick={() => goto('/worklog')}>
								<svg
									width="15"
									height="15"
									viewBox="0 0 16 16"
									fill="none"
									stroke="currentColor"
									stroke-width="1.2"
									stroke-linecap="round"
									stroke-linejoin="round"
									aria-hidden="true"
								>
									<circle cx="8" cy="8" r="5.5" />
									<path d="M8 4.8V8l2.2 1.6" />
								</svg>
								{t('titlebar.menuWorklog', $locale)}
							</button>
							<button onclick={() => goto('/tags')}>
								<svg
									width="15"
									height="15"
									viewBox="0 0 16 16"
									fill="none"
									stroke="currentColor"
									stroke-width="1.2"
									stroke-linecap="round"
									stroke-linejoin="round"
									aria-hidden="true"
								>
									<path
										d="M8.3 2.5H3.2A.7.7 0 0 0 2.5 3.2v5.1a1 1 0 0 0 .3.7l4.9 4.9a1 1 0 0 0 1.4 0l4.4-4.4a1 1 0 0 0 0-1.4L9 2.8a1 1 0 0 0-.7-.3Z"
									/>
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

	<!-- 중앙: 길드 이름 pill = 검색 팔레트 (+ DEV-276 최근 문서 버튼).
	     길드 컨텍스트 있을 때만, 자식윈도우에선 숨김(단일 문서 보기 창엔
	     전체 검색/최근 목록이 무의미). -->
	{#if $guildContextActive && guildName && !$isChildWindow}
		<div class="tb-center" style="--pill-half-w: {pillHalfWidth}px">
			<!-- DEV-286: 검색 pill 왼쪽 바깥에 '퀘스트 추가' 버튼. BUG-158 과 동일하게
			     절대 배치라 버튼 유무가 pill 중앙 위치에 영향을 주지 않는다. -->
			<div class="tb-add-wrap">
				<button
					class="tb-icon-btn"
					onclick={() => (newQuestOpen = true)}
					title={t('titlebar.newQuest', $locale)}
					aria-label={t('titlebar.newQuest', $locale)}
				>
					<svg
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
				class="tb-search"
				class:open={searchOpen}
				bind:this={tbSearchEl}
				onclick={() => (searchOpen = true)}
				title={t('titlebar.search', $locale)}
			>
				<span class="tb-search-name">{guildName}</span>
				{#if isRemote}
					<span class="tb-remote" title={t('nav.remoteConnected', $locale)}>
						<svg
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
			<!-- DEV-276: 최근 본 문서 — 뒤로가기는 한 칸씩만 가므로 "아까 그
			     문서"로 바로 점프하는 수단. 기록이 없으면 버튼 자체를 숨김. -->
			{#if $recentDocs.length > 0}
				<div class="tb-recent-wrap">
					<!-- DEV-294: 전용 드롭다운(폭 좁아 제목이 안 보였음) 대신 검색
					     팔레트를 recent 모드로 재사용 — 레이아웃/미리보기/스크롤 공유. -->
					<button
						class="tb-icon-btn"
						class:active={recentOpen}
						onclick={() => {
							recentOpen = true;
							void loadQuestIndex();
						}}
						title={t('titlebar.recent', $locale)}
						aria-label={t('titlebar.recent', $locale)}
					>
						<svg
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

	<div class="tb-spacer" data-tauri-drag-region></div>

	<!-- 오른쪽 배치(기본) — 왼쪽 배치면 위 타이틀바 시작부에서 렌더됨. -->
	{#if !controlsOnLeft}{@render winControls()}{/if}
</div>

{#snippet winControls()}
	<div
		class="tb-controls"
		class:linux={isLinuxControls}
		class:left={controlsOnLeft}
		style={isLinuxControls ? `gap:${LINUX_BTN_GAP}px;` : undefined}
	>
		<!-- DEV-255: 자식윈도우 전용 pin(Always on top) — 문서 창을 다른 작업
		     위에 고정해놓고 참조하는 흐름. -->
		{#if $isChildWindow}
			<button
				class="tb-btn"
				class:tb-pin-on={pinned}
				onclick={togglePin}
				title={pinned ? t('titlebar.unpin', $locale) : t('titlebar.pin', $locale)}
				aria-label={pinned ? t('titlebar.unpin', $locale) : t('titlebar.pin', $locale)}
				aria-pressed={pinned}
			>
				<svg
					width="11"
					height="11"
					viewBox="0 0 16 16"
					fill="none"
					stroke="currentColor"
					stroke-width="1.3"
					stroke-linecap="round"
					stroke-linejoin="round"
					aria-hidden="true"
				>
					{#if pinned}
						<path
							d="M9.5 2 14 6.5l-2.5.7-2.8 2.8-.4 3.5L3 8.2l3.5-.4 2.3-2.8Z"
							fill="currentColor"
							stroke="none"
						/>
						<path d="M5.5 10.5 2 14" />
					{:else}
						<path d="M9.5 2 14 6.5l-2.5.7-2.8 2.8-.4 3.5L3 8.2l3.5-.4 2.3-2.8Z" />
						<path d="M5.5 10.5 2 14" />
					{/if}
				</svg>
			</button>
		{/if}

		{#if isMac}
			<!-- macOS: 아무것도 렌더링하지 않음 — 네이티브 traffic light 가
			     Overlay 로 이미 이 자리(좌측)에 떠 있음. -->
		{:else if isLinuxControls}
			<!-- Linux: 백엔드가 실제 GTK 아이콘 테마를 조회해 순서/아이콘/크기를
			     주면 그대로, 실패 시 기존 Adwaita 근사 CSS 로 폴백. 아이콘은
			     심볼릭 SVG 를 CSS mask 로 그려(background: currentColor) 버튼
			     텍스트색 = 앱 테마색을 따라간다(네이티브 recolor 와 동일). -->
			{@const order = linuxStyle?.order ?? ['min', 'max', 'close']}
			{@const nativeBtn = linuxStyle?.buttonSizePx ?? 34}
			{@const btnSize = Math.min(nativeBtn, LINUX_BTN_CAP)}
			{@const iconSize = Math.round(btnSize * LINUX_ICON_RATIO)}
			{@const linuxBtnStyle = `width:${btnSize}px;height:${btnSize}px;`}
			{#each order as action (action)}
				{#if action === 'min'}
					<button
						class="tb-btn"
						style={linuxBtnStyle}
						onclick={() => winCtl('min')}
						title={t('titlebar.minimize', $locale)}
						aria-label={t('titlebar.minimize', $locale)}
					>
						{#if linuxStyle?.minIcon}
							<span
								class="tb-nativeicon"
								style="width:{iconSize}px;height:{iconSize}px;-webkit-mask-image:url({linuxStyle.minIcon});mask-image:url({linuxStyle.minIcon});"
								aria-hidden="true"
							></span>
						{:else}
							<svg width="10" height="10" viewBox="0 0 10 10" aria-hidden="true">
								<line x1="0" y1="5" x2="10" y2="5" stroke="currentColor" stroke-width="1" />
							</svg>
						{/if}
					</button>
				{:else if action === 'max'}
					<button
						class="tb-btn"
						style={linuxBtnStyle}
						onclick={() => winCtl('max')}
						title={maximized ? t('titlebar.restore', $locale) : t('titlebar.maximize', $locale)}
						aria-label={maximized
							? t('titlebar.restore', $locale)
							: t('titlebar.maximize', $locale)}
					>
						{#if maximized && linuxStyle?.restoreIcon}
							<span
								class="tb-nativeicon"
								style="width:{iconSize}px;height:{iconSize}px;-webkit-mask-image:url({linuxStyle.restoreIcon});mask-image:url({linuxStyle.restoreIcon});"
								aria-hidden="true"
							></span>
						{:else if !maximized && linuxStyle?.maxIcon}
							<span
								class="tb-nativeicon"
								style="width:{iconSize}px;height:{iconSize}px;-webkit-mask-image:url({linuxStyle.maxIcon});mask-image:url({linuxStyle.maxIcon});"
								aria-hidden="true"
							></span>
						{:else if maximized}
							<svg width="10" height="10" viewBox="0 0 10 10" aria-hidden="true">
								<rect
									x="0"
									y="2.5"
									width="7"
									height="7"
									fill="none"
									stroke="currentColor"
									stroke-width="1"
								/>
								<path
									d="M 2.5 2.5 V 0.5 H 9.5 V 7.5 H 7.5"
									fill="none"
									stroke="currentColor"
									stroke-width="1"
								/>
							</svg>
						{:else}
							<svg width="10" height="10" viewBox="0 0 10 10" aria-hidden="true">
								<rect
									x="0.5"
									y="0.5"
									width="9"
									height="9"
									fill="none"
									stroke="currentColor"
									stroke-width="1"
								/>
							</svg>
						{/if}
					</button>
				{:else}
					<button
						class="tb-btn tb-close"
						style={linuxBtnStyle}
						onclick={() => winCtl('close')}
						title={t('titlebar.close', $locale)}
						aria-label={t('titlebar.close', $locale)}
					>
						{#if linuxStyle?.closeIcon}
							<span
								class="tb-nativeicon"
								style="width:{iconSize}px;height:{iconSize}px;-webkit-mask-image:url({linuxStyle.closeIcon});mask-image:url({linuxStyle.closeIcon});"
								aria-hidden="true"
							></span>
						{:else}
							<svg width="10" height="10" viewBox="0 0 10 10" aria-hidden="true">
								<line x1="0" y1="0" x2="10" y2="10" stroke="currentColor" stroke-width="1" />
								<line x1="10" y1="0" x2="0" y2="10" stroke="currentColor" stroke-width="1" />
							</svg>
						{/if}
					</button>
				{/if}
			{/each}
		{:else}
			<!-- Windows: OS 가 실제로 쓰는 Segoe Fluent Icons/Segoe MDL2 Assets
			     글리프 — 손으로 그린 SVG 근사가 아니라 네이티브 캡션 버튼과
			     동일한 아이콘 모양. -->
			<button
				class="tb-btn"
				onclick={() => winCtl('min')}
				title={t('titlebar.minimize', $locale)}
				aria-label={t('titlebar.minimize', $locale)}
			>
				<span class="tb-winicon">{winIcon.min}</span>
			</button>
			<button
				bind:this={maxBtnEl}
				class="tb-btn"
				onclick={() => winCtl('max')}
				onmouseenter={reportMaxButtonRect}
				title={maximized ? t('titlebar.restore', $locale) : t('titlebar.maximize', $locale)}
				aria-label={maximized ? t('titlebar.restore', $locale) : t('titlebar.maximize', $locale)}
			>
				<span class="tb-winicon">{maximized ? winIcon.restore : winIcon.max}</span>
			</button>
			<button
				class="tb-btn tb-close"
				onclick={() => winCtl('close')}
				title={t('titlebar.close', $locale)}
				aria-label={t('titlebar.close', $locale)}
			>
				<span class="tb-winicon">{winIcon.close}</span>
			</button>
		{/if}
	</div>
{/snippet}

{#if newQuestOpen}
	<NewQuestModal
		onclose={() => (newQuestOpen = false)}
		oncreated={(quest) => goto('/quests/' + quest.quest_id)}
	/>
{/if}

{#if searchOpen}
	<SearchPalette onclose={() => (searchOpen = false)} />
{/if}

<!-- DEV-294: 최근 본 문서 = 같은 팔레트의 recent 모드. -->
{#if recentOpen}
	<SearchPalette mode="recent" onclose={() => (recentOpen = false)} />
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
	/* macOS: titleBarStyle:"Overlay" 로 네이티브 traffic light(빨/노/초)가
	   좌측 상단에 그대로 떠 있음 — 그 폭만큼 콘텐츠가 안 겹치게 여백. 정확한
	   폭은 macOS 버전/스케일에 따라 미세하게 다를 수 있어 근사값. */
	.titlebar.mac-overlay {
		padding-left: 78px;
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
	/* DEV-260: Nav overflow 항목 — 활성 페이지 하이라이트 유지 + 구분선. */
	.tb-menu button.active {
		background: var(--nav-hover-bg);
		color: var(--text-strong);
	}
	.tb-menu-sep {
		height: 1px;
		margin: 0.25rem 0.3rem;
		background: var(--border);
	}

	/* ── DEV-276: 최근 본 문서 드롭다운 ── */
	/* BUG-158: pill 오른쪽 **바깥**에 절대 배치 — 흐름에서 빠지므로 버튼이
	   생겨도 pill 중앙 위치가 그대로다. left:50% 는 폭 0 앵커의 중앙이고,
	   translateX 로 pill 반폭(21vw = max-width 42vw 의 절반) 만큼 밀어
	   오른쪽 끝에 붙인다 — pill 이 min-width 로 더 좁을 땐 그만큼 간격이
	   생기지만 겹치지는 않는다. */
	/* DEV-286: 최근 버튼(오른쪽)의 좌우 대칭 — pill 왼쪽 바깥에 절대 배치.
	   `-100%` 로 자기 폭만큼 더 왼쪽으로 빼 오른쪽 끝을 pill 왼쪽에 붙인다. */
	.tb-add-wrap {
		position: absolute;
		left: 50%;
		/* BUG-187: vw 근사 대신 JS 로 실측한 pill 절반 폭(--pill-half-w) 사용 —
		   pill 의 min-width/max-width/내용 길이와 무관하게 항상 가장자리 밖. */
		transform: translateX(calc(-1 * var(--pill-half-w, 134px) - 100% - 4px));
		display: inline-flex;
		align-items: center;
		flex: none;
	}
	.tb-recent-wrap {
		position: absolute;
		left: 50%;
		transform: translateX(var(--pill-half-w, 134px));
		display: inline-flex;
		align-items: center;
		flex: none;
	}
	/* ── 중앙 검색 pill ── */
	/* DEV-276 → BUG-158: 검색 pill 은 **항상 화면 중앙 고정**.
	   처음엔 pill + 최근 버튼을 한 wrapper 로 묶어 통째로 중앙 정렬했는데,
	   그러면 최근 버튼이 생기거나 사라질 때마다 묶음 폭이 바뀌어 pill 이
	   좌우로 밀렸다(사용자 지적). wrapper 자체를 폭 0 의 중앙 앵커로 두고
	   pill 은 그 앵커 기준 중앙, 최근 버튼은 pill 오른쪽 바깥에 absolute
	   로 띄운다 — 버튼 유무가 pill 위치에 전혀 영향을 주지 않는다. */
	.tb-center {
		position: absolute;
		left: 50%;
		transform: translateX(-50%);
		display: flex;
		align-items: center;
		justify-content: center;
		/* 폭 0 앵커 — 자식이 넘쳐도 레이아웃에 영향 없음. */
		width: 0;
	}
	.tb-search {
		display: inline-flex;
		align-items: center;
		justify-content: center;
		height: 22px;
		min-width: 260px;
		max-width: 42vw;
		padding: 0 12px;
		/* 폭 0 앵커 안에서 자기 폭만큼 좌우로 균등하게 삐져나오도록. */
		flex: none;
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
		/* Windows 네이티브 캡션 버튼 관례: 버튼 사이 간격 0, 서로 붙어있음. */
		gap: 0;
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
	/* Windows: OS 캡션 버튼과 동일한 폰트 글리프. */
	.tb-winicon {
		font-family: 'Segoe Fluent Icons', 'Segoe MDL2 Assets', sans-serif;
		font-size: 10px;
		font-weight: 400;
		font-style: normal;
		line-height: 1;
	}
	/* Linux: 백엔드가 조회해 온 실제 시스템 심볼릭 SVG 를 CSS mask 로 그림 —
	   background:currentColor 라 버튼 텍스트색(= 앱 다크/라이트 테마)을 그대로
	   따라간다(네이티브 GTK 의 심볼릭 recolor 와 동일 결과). 크기는 인라인
	   style 로 버튼 크기에 비례해 지정. */
	.tb-nativeicon {
		display: inline-block;
		background-color: currentColor;
		-webkit-mask-repeat: no-repeat;
		mask-repeat: no-repeat;
		-webkit-mask-position: center;
		mask-position: center;
		-webkit-mask-size: contain;
		mask-size: contain;
		pointer-events: none;
	}
	/* DEV-255: pin(Always on top) 켜짐 상태 — 아이콘 채움 + 강조색. */
	.tb-btn.tb-pin-on {
		color: var(--accent);
	}
	.tb-close:hover {
		background: var(--danger);
		color: var(--btn-primary-text);
	}

	/* ── DEV-265: Linux 창 컨트롤 — GNOME/Adwaita 근사: 원형 버튼 + 원형
	   호버 배경, 버튼 간 간격, 닫기도 동일 원형 호버(Adwaita 관례 — 빨강
	   강조 없음).
	   크기/간격/패딩 전부 px — 창 컨트롤은 OS 크롬이라 앱 UI 크기(root
	   font-size = rem) 조절에 영향받지 않아야 top-right 고정이 유지된다.
	   (이전엔 padding 이 rem 이라 UI 스케일 시 버튼이 좌측으로 밀렸음.) */
	.tb-controls.linux {
		align-items: center;
		gap: 10px;
		padding: 0 10px 0 5px;
	}
	/* 왼쪽 배치(시스템 button-layout 이 왼쪽) — 패딩을 좌우 반전해 창 왼쪽
	   가장자리와의 간격을 맞춘다. */
	.tb-controls.linux.left {
		padding: 0 5px 0 10px;
	}
	.tb-controls.linux .tb-btn {
		width: 24px;
		height: 24px;
		border-radius: 50%;
		background: color-mix(in srgb, var(--text) 8%, transparent);
		color: var(--text);
		cursor: default;
	}
	.tb-controls.linux .tb-btn:hover {
		background: color-mix(in srgb, var(--text) 18%, transparent);
		color: var(--text-strong);
	}
	.tb-controls.linux .tb-close:hover {
		background: color-mix(in srgb, var(--text) 18%, transparent);
		color: var(--text-strong);
	}
</style>
