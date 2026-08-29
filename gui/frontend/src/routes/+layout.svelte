<script lang="ts">
	import { page } from '$app/stores';
	import { afterNavigate, beforeNavigate, goto, replaceState } from '$app/navigation';
	import {
		pageScrollTop,
		scrollPageTo,
		pageScrollHeight,
		pageViewportHeight,
		onPageScroll
	} from '$lib/utils/page-scroll';
	// BUG-176 / DEV-355: 히스토리 항목의 길드 표식 비교 + 길드 복원.
	import {
		currentGuildId,
		invalidateCurrentGuild,
		sameGuild,
		guildSwitchingPossible,
		type GuildId
	} from '$lib/stores/guildIdentity';
	import {
		markRemoteSessionActive,
		pingRemoteServer,
		setRemoteServerUrl
	} from '$lib/stores/remoteServer';
	// DEV-153: 미저장 변경 통합 가드.
	import ConfirmDialog from '$lib/components/ConfirmDialog.svelte';
	import { anyUnsaved, clearUnsaved } from '$lib/stores/unsaved';
	// BUG-179: 데스크탑에서 새로고침 단축키만 가로채기 위한 판정.
	import { isReloadShortcut } from '$lib/utils/reload-shortcut';
	import { onMount, tick } from 'svelte';
	// DEV-276: 최근 본 문서 추적.
	import { pushRecentDoc, classifyDocRoute, canonicalDocHref } from '$lib/stores/recentDocs';
	import Nav from '$lib/components/Nav.svelte';
	// 커스텀 타이틀바 — Windows Tauri 전용 (tauri.windows.conf.json 의
	// decorations:false 와 세트). 네이티브 타이틀바 테마 어긋남 원천 해소.
	import TitleBar from '$lib/components/TitleBar.svelte';
	// DEV-259: 앱 알림 통합 호스트 — 토스트/업데이트/스키마 알림을 우하단 단일
	// 스택으로. 업데이트·스키마 watcher 도 이 호스트가 내장(이전의 UpdateBanner/
	// SchemaAheadBanner 껍데기 컴포넌트 제거).
	import ToastHost from '$lib/components/ToastHost.svelte';
	import { showToast } from '$lib/stores/toast';
	// DEV-074 fix13: window 스크롤 overlay — 컨텐츠 폭 차지 X.
	import OverlayScrollbar from '$lib/components/OverlayScrollbar.svelte';
	import { detectEnvironment } from '$lib/api/transport';
	// BUG-140: 커스텀 타이틀바 플랫폼 판별(Windows/Linux) — 단일 진리원.
	import { usesCustomTitlebar, isLinux, hasCoarsePointer } from '$lib/utils/platform';
	import { uiScale, applyUiScaleToDocument } from '$lib/stores/uiScale';
	import { hdrLimit, applyHdrLimitToDocument } from '$lib/stores/hdrSettings';
	import { contentWidth, contentWidthCss } from '$lib/stores/contentWidth';
	import {
		theme,
		applyThemeToDocument,
		watchSystemPreference,
		resolveTheme,
		type ThemeChoice
	} from '$lib/stores/theme';
	// DEV-114: 커스텀 테마 — 시동 시 활성 프리셋의 토큰 override 복원.
	import { initCustomTheme } from '$lib/stores/customThemes';
	import { get } from 'svelte/store';
	// DEV-205: 앱 언어 → <html lang>. 네이티브 컨트롤(날짜 선택기의 년-월-일
	// 표기 등)이 앱 언어를 따르도록.
	import { locale, t } from '$lib/stores/locale';
	// DEV-255: 자식윈도우(검색 팔레트 "새 창으로 열기") 판정 — 메뉴바/타이틀바
	// 일부 숨김에 사용.
	import { isChildWindow, detectWindowKind } from '$lib/stores/windowKind';
	import '$lib/styles/global.css';

	let { children } = $props();
	// DEV-355: 다른 길드의 히스토리 항목으로 이동하는 동안, 새 URL 과 아직
	// 전환되지 않은 Store 가 섞인 화면을 mount 하지 않는다.
	let historyGuildSwitching = $state(false);
	// BUG-257: 스크롤 컨테이너. `page-scroll.ts` 가 querySelector 로도 찾지만,
	// 여기서는 OverlayScrollbar 에 넘겨야 해서 참조를 들고 있는다.
	let mainEl = $state<HTMLElement | undefined>(undefined);

	// DEV-052 후속: /welcome 라우트에선 Nav (Board/List/Admin/+New Quest) 숨김.
	// 길드 컨텍스트가 없는 상태에서 의미 없는 액션 노출 방지.
	// DEV-255: 자식윈도우(단일 문서 보기)도 메뉴바 불필요 — 함께 숨김.
	let showNav = $derived(
		$page.url.pathname !== '/welcome' && !$isChildWindow && !historyGuildSwitching
	);

	// BUG-257: 스크롤 컨테이너가 문서가 아니게 되면서 **SvelteKit 의 스크롤
	// 처리가 더 이상 닿지 않는다.** 라우터는 window 를 스크롤하는데 window 는
	// 이제 항상 0 이다. 그래서 두 가지를 직접 해야 한다.
	//
	//  1) 앞으로 가는 이동(링크·goto)은 맨 위에서 시작.
	//  2) 뒤로/앞으로(popstate)는 그 항목의 위치로 복원.
	//
	// 저장은 위 `saveScrollPosition` 과 같은 sessionStorage 키를 쓴다 — 새로고침
	// 복원과 같은 저장소를 공유하므로 규칙이 한 벌이다.
	beforeNavigate((nav) => {
		const from = nav.from?.url;
		saveScrollPosition(from ? from.pathname + from.search : undefined);
	});
	afterNavigate((nav) => {
		if (nav.type === 'popstate') {
			restoreScrollPosition();
			return;
		}
		// 해시 앵커로 가는 이동은 각 페이지가 자기 방식으로 처리한다 —
		// 여기서 맨 위로 올리면 그 이동을 덮어쓴다.
		if (nav.to?.url.hash) return;
		scrollPageTo(0);
	});

	onMount(() => {
		detectWindowKind();
	});

	// DEV-255: 검색 팔레트 "새 창으로 열기"가 만든 자식윈도우는 항상 `/`
	// (어떤 서빙 방식에서도 항상 존재가 보장되는 경로)로 뜬 뒤, 이 쿼리
	// 파라미터로 실제 목적지를 넘겨받아 client-side goto 로 이동한다 —
	// Tauri 의 asset protocol 이 임의의 딥링크 경로를 직접 서빙해준다는
	// 보장이 없어(SPA fallback 미보장), 항상 이미 존재하는 진입점을 먼저
	// 로드한 뒤 앱 안에서 라우팅하는 편이 dev/HTTP/Tauri 어디서나 동일하게
	// 동작한다.
	onMount(() => {
		const target = $page.url.searchParams.get('winTarget');
		if (target) void goto(target, { replaceState: true });
	});

	// 커스텀 타이틀바 — decorations:false 인 플랫폼(Windows/Linux, 각
	// tauri.{platform}.conf.json)과 세트. BUG-140: Linux 도 커스텀 사용
	// (판별을 usesCustomTitlebar() 로 단일화 — 자식윈도우 decorations 옵션과
	// 어긋나지 않게). 표시 시 sticky 요소들(Nav 등)의 top offset 용
	// CSS 변수(--titlebar-h)를 root 에 심는다.
	const showTitleBar = usesCustomTitlebar();
	// 모바일 수정(admin 보고): 터치 기기는 브라우저가 스크롤 인디케이터를 스스로
	// 그리므로 페이지 전체용 커스텀 스크롤바까지 그리면 두 개가 겹쳐 보인다.
	// 컨테이너(검색 팔레트·자동완성 팝업·목록 등)의 커스텀 스크롤바는 그대로 —
	// 그쪽은 브라우저가 대신 그려주지 않는다.
	let coarsePointer = $state(false);
	$effect(() => {
		if (typeof window === 'undefined' || !window.matchMedia) return;
		const mq = window.matchMedia('(pointer: coarse)');
		const sync = () => (coarsePointer = mq.matches);
		sync();
		mq.addEventListener('change', sync);
		return () => mq.removeEventListener('change', sync);
	});

	// DEV-265: 리눅스는 네이티브 창 버튼을 더 크게 담기 위해 타이틀바를 살짝
	// 높이고(+8px), 그만큼 메뉴바(Nav) 높이를 줄여(–8px) 콘텐츠 영역 총합은
	// 그대로 유지한다. Windows/macOS 는 기존 32px 유지.
	const linuxTitlebar = showTitleBar && isLinux();
	$effect(() => {
		if (typeof document === 'undefined') return;
		const root = document.documentElement.style;
		// BUG-246: rem 이라 UI 크기 조절(DEV-101 — root font-size 배율)을 따라간다.
		//
		// Linux 만 px 하한을 둔다. 그쪽 창 컨트롤은 **의도적으로 px 고정**이고
		// (아래 TitleBar 의 `.tb-controls.linux` 주석 참고 — rem 이면 배율에서
		// 버튼이 좌측으로 밀린다) 지름이 24px 이라, 바가 그보다 낮아지면 버튼이
		// 넘친다. 배율 50% 면 2.5rem = 20px 다.
		// px 상수이던 시절엔 바 높이만 고정이라, 안쪽 요소를 rem 으로 바꾸면
		// 배율 150% 에서 pill 이 바 밖으로 넘쳤다. 이 값을 쓰는 곳(Nav 의 top,
		// QuestList / QuestBoard / SearchPalette 의 calc)은 전부 var() 라 함께 따라간다.
		root.setProperty('--titlebar-h', showTitleBar ? (linuxTitlebar ? 'max(2.5rem, 30px)' : '2rem') : '0px');
		// Nav 기본 높이 3.25rem(=52px @scale1). 리눅스에서만 8px 줄임.
		root.setProperty('--nav-h', linuxTitlebar ? 'calc(3.25rem - 8px)' : '3.25rem');
	});

	// DEV-205: <html lang> 을 앱 언어에 맞춤 — 네이티브 date input 등이 반영.
	$effect(() => {
		if (typeof document === 'undefined') return;
		document.documentElement.lang = $locale;
	});

	// DEV-153: 미저장 변경 통합 가드. 편집 중(unsaved.ts 에 보고된 dirty)이면
	// 라우트 이동(링크/뒤로·앞으로가기)을 취소하고 공용 확인 모달을 띄운다.
	// 새로고침/창 닫기 등 willUnload 는 cancel 불가 → 아래 beforeunload 가 담당.
	let showUnsavedModal = $state(false);
	// 확인 시 실행할 동작 (라우트 이동 / 창 닫기 등) — 모달 일반화.
	let pendingAction: (() => void) | null = null;
	// DEV-355: BUG-176 당시에는 길드 재오픈의 전체 reindex 비용을 피하려고
	// Welcome 의 popstate 를 모두 막았다. 지금은 sync_on_open 이 일반 변경을
	// 증분 동기화하므로, 해당 히스토리 항목에 기록된 길드를 실제로 복원한다.
	//
	// SvelteKit 의 공개 NavigationTarget 에는 target PageState 가 없다.
	// popstate 원본 event 의 SvelteKit state 를 읽어 **라우트가 렌더되기 전에**
	// 전환 화면으로 바꾼다. 그래야 이전 URL + 현재 Store 조합이 한 프레임도
	// 보이지 않고, 해당 페이지 컴포넌트의 API 요청도 잘못된 길드로 나가지 않는다.
	beforeNavigate((nav) => {
		if (anyUnsaved() && !nav.willUnload) {
			const url = nav.to?.url;
			if (!url) return;
			nav.cancel();
			pendingAction = () => goto(url);
			showUnsavedModal = true;
			return;
		}

		if (nav.type !== 'popstate' || !guildSwitchingPossible()) return;
		if (historyGuildSwitching) {
			// 길드 open/ping 중 연속 뒤로가기 — 첫 전환이 끝날 때까지 현재 항목 유지.
			nav.cancel();
			return;
		}
		const targetGuild = nav.event.state?.['sveltekit:states']?.guild as GuildId | undefined;
		const currentStamp = $page.state.guild as GuildId | undefined;
		if (targetGuild && !sameGuild(targetGuild, currentStamp)) {
			historyGuildSwitching = true;
		}
	});
	function discardAndProceed() {
		showUnsavedModal = false;
		clearUnsaved();
		const act = pendingAction;
		pendingAction = null;
		act?.();
	}
	function keepEditing() {
		showUnsavedModal = false;
		pendingAction = null;
	}
	// BUG-075: 창 닫기(onCloseRequested) 가드는 제거된 상태로 둔다 — WebView2 에서
	// 창이 안 닫히는 회귀(심각도 1). 앱 종료는 어떤 경우에도 막지 않는다.
	//
	// BUG-179: 다만 **새로고침**은 그때 같이 빠져서, 편집 중 F5 를 누르면 경고
	// 없이 내용이 사라졌다(BUG-075 커밋이 "안전한 방법 확보 후 재도입" 이라고
	// 남긴 부분). 환경별로 창 닫기와 무관한 수단만 쓴다:
	//  - 브라우저/서버 모드: beforeunload (Tauri 창과 무관 → BUG-075 와 충돌 없음)
	//  - Tauri: 새로고침 단축키만 keydown 에서 가로채 공용 모달로. beforeunload /
	//    onCloseRequested 는 건드리지 않는다.
	onMount(() => {
		if (detectEnvironment() !== 'tauri') {
			const guard = (e: BeforeUnloadEvent) => {
				if (!anyUnsaved()) return;
				e.preventDefault();
				e.returnValue = '';
			};
			window.addEventListener('beforeunload', guard);
			return () => window.removeEventListener('beforeunload', guard);
		}
		const onKeyDown = (e: KeyboardEvent) => {
			if (!isReloadShortcut(e)) return;
			e.preventDefault();
			// DEV-345: 미저장 변경이 없으면 확인 모달 없이 바로 새로고침.
			// 예전엔 이 분기가 없어서 Cmd+R 이 (미저장 상태일 때만 동작하는
			// 모달 경로 말고는) 아무 반응도 없었다 — Tauri 는 브라우저와 달리
			// Cmd+R 기본 동작이 없어 직접 reload() 를 호출해야 함.
			if (!anyUnsaved()) {
				window.location.reload();
				return;
			}
			// discardAndProceed 가 clearUnsaved() 후 이 동작을 실행한다.
			pendingAction = () => window.location.reload();
			showUnsavedModal = true;
		};
		// capture — CodeMirror 등이 먼저 삼키지 않도록.
		window.addEventListener('keydown', onKeyDown, true);
		return () => window.removeEventListener('keydown', onKeyDown, true);
	});

	// DEV-111 fix1: mermaid 가 render() 중 실패하면 body 끝에 leftover 임시
	// 컨테이너 (bomb 아이콘 + "Syntax error in text mermaid version X.Y.Z") 가
	// 남는다. SPA 라 라우트 전환에도 안 사라져 markdown preview 없는 페이지
	// 에서도 보임. MarkdownView 의 parse pre-check 로 신규 leftover 는 막지만,
	// 이전 코드에서 발생한 / 다른 경로로 생긴 leftover 까지 청소하기 위해
	// 매 navigation 후 sweep.
	function sweepMermaidLeftovers() {
		if (typeof document === 'undefined') return;
		// MarkdownView 가 발급한 id 패턴 = `mm-<n>-<rand>`.
		// mermaid v11 가 만드는 임시 노드 후보: 같은 id 의 svg, `d` 프리픽스 div.
		document
			.querySelectorAll<HTMLElement>('body > svg[id^="mm-"], body > div[id^="dmm-"]')
			.forEach((el) => el.remove());
	}
	onMount(sweepMermaidLeftovers);
	afterNavigate(sweepMermaidLeftovers);

	// DEV-276: 최근 본 문서 기록 — "최근" 버튼(타이틀바/Nav)의 소스.
	// 문서 성격 라우트(퀘스트/캠페인 상세·규칙·도서관)만 기록하고, 목록/보드/
	// 설정 같은 탐색 화면은 제외(classifyDocRoute 가 판별).
	//
	// BUG-159: 제목은 여기서 저장하지 않는다 — 예전엔 화면의 `main h1` 을
	// 긁었는데 페이지마다 h1 의미가 달라 엉뚱한 값이 들어갔다(규칙은
	// `# {slug}` 로 라벨 중복, 도서관은 첫 h1 이 페이지 제목 "도서관"이라
	// 모든 문서가 같게 보임). 이제 표시 시점에 cross-link 인덱스에서 조회
	// (recentDocTitle) — 정확하고 이름 변경도 자동 반영.
	function trackRecentDoc() {
		if ($isChildWindow) return; // 자식창(단일 문서 보기)은 목록을 쌓을 필요 없음
		const hit = classifyDocRoute($page.url.pathname + $page.url.search);
		if (!hit) return;
		// BUG-181: `?from=` 같은 추적 쿼리가 섞인 원본 URL 대신 정규 href 로
		// 저장 — SearchPalette 전역 인덱스의 href 와 문자열이 일치해야 recent
		// 모드에서 매칭된다(불일치 시 퀘스트가 조용히 누락됨).
		pushRecentDoc({
			href: canonicalDocHref(hit.kind, hit.label),
			kind: hit.kind,
			label: hit.label
		});
	}
	afterNavigate(trackRecentDoc);
	onMount(trackRecentDoc);

	// DEV-355: 네비게이션마다 현재 길드를 항목 state 에 남긴다. popstate 로
	// 다른 길드의 항목에 도달하면 그 길드를 다시 연 뒤 reload 한다. reload 가
	// 필요한 이유는 같은 URL 이 길드마다 존재할 수 있고, 여러 페이지가
	// onMount 에서만 데이터를 읽기 때문 — Store 만 바꾸고 invalidateAll 해서는
	// 같은 route component 가 옛 길드의 로컬 상태를 계속 들고 있을 수 있다.
	async function reopenGuildFromHistory(target: GuildId): Promise<void> {
		if (target.kind === 'local') {
			const { invoke } = await import('@tauri-apps/api/core');
			// BUG-245: 히스토리 복원으로 여는 것은 "최근 연 길드"가 아니다 —
			// 시각을 갱신하면 뒤로가기만으로 옛 길드가 목록 맨 위로 올라온다.
			await invoke('open_guild_in_current_window', {
				path: target.path,
				touchRecents: false
			});
			// open 성공 뒤에만 remote override 를 끈다. 실패했을 때 현재 원격
			// 세션까지 잃어버리지 않기 위함이다.
			setRemoteServerUrl(null);
		} else {
			const reachable = await pingRemoteServer(target.url);
			if (!reachable) throw new Error(t('history.guildUnreachable', $locale));
			markRemoteSessionActive();
			setRemoteServerUrl(target.url);
		}
		invalidateCurrentGuild();
	}

	async function guardGuildHistory(nav: { type: string }) {
		const cur = await currentGuildId();
		if (!cur) {
			historyGuildSwitching = false;
			return; // 브라우저 모드 / 길드 미오픈 — 전환 개념 없음.
		}
		const stamped = $page.state.guild as GuildId | undefined;
		if (!stamped) {
			// 아직 표식이 없는 항목(길드를 연 직후, 콜드 스타트 등) — 현재 길드로
			// 표시만 남기고 통과. replaceState 라 히스토리가 늘지 않는다.
			replaceState('', { ...$page.state, guild: cur });
			historyGuildSwitching = false;
			return;
		}
		if (sameGuild(stamped, cur)) {
			historyGuildSwitching = false;
			return;
		}
		// 다른 길드의 항목 — 뒤로/앞으로 이동으로 도달했을 때만 개입한다.
		// (링크 클릭으로 이런 상태가 되는 경로는 없다.)
		if (nav.type !== 'popstate') {
			historyGuildSwitching = false;
			return;
		}

		historyGuildSwitching = true;
		try {
			await reopenGuildFromHistory(stamped);
			// 전환 상태를 유지한 채 전체 route tree 를 새 길드 기준으로 재생성.
			window.location.reload();
		} catch (error) {
			console.error('[guild-history] 길드 복원 실패', error);
			historyGuildSwitching = false;
			const detail = error instanceof Error ? error.message : String(error);
			showToast(`${t('history.switchFailed', $locale)}: ${detail}`, 'error');
			void goto('/welcome', { replaceState: true, state: { guild: cur } });
		}
	}
	afterNavigate((nav) => {
		void guardGuildHistory(nav);
	});

	// 비정상 quest 파일(정의되지 않은 status / 파싱 실패) 감지 시 시동 알림.
	// 그런 파일은 reindex/sync 에서 조용히 skip 되므로 사용자에게 안 보임 → toast.
	onMount(async () => {
		if (detectEnvironment() !== 'tauri') return;
		try {
			const { invoke } = await import('@tauri-apps/api/core');
			const problems = await invoke<{ path: string; reason: string }[]>('list_problem_files');
			if (problems.length > 0) {
				const lines = problems
					.slice(0, 5)
					.map((p) => `· ${p.path.split(/[/\\]/).pop()} — ${p.reason}`)
					.join('\n');
				const more = problems.length > 5 ? `\n…외 ${problems.length - 5}개` : '';
				showToast(
					`비정상 파일 ${problems.length}개 감지 (캐시에서 제외됨):\n${lines}${more}`,
					'error',
					0
				);
			}
		} catch {
			/* 길드 모드 아님 / 조회 실패 — 무시 */
		}
	});

	// DEV-126: 페이지 새로고침 후 스크롤 위치 유지.
	// SvelteKit 의 SPA navigation 은 자동으로 위치 관리 (afterNavigate 가 history
	// state 따라 복원) 하지만, F5 / window.location.reload() / DEV-120 의 admin
	// reindex 후 reload 는 page 자체 reload — 항상 top 으로. sessionStorage 에
	// path 별 scrollY 를 저장해서 reload 시 복원.
	const SCROLL_KEY_PREFIX = 'openguild.scroll.';
	/**
	 * @param key 저장할 경로+쿼리. 생략하면 현재 주소.
	 *
	 * BUG-257: `beforeNavigate` 는 **popstate 에서 이미 주소가 바뀐 뒤** 실행된다.
	 * 그때 현재 주소로 저장하면 떠나는 페이지의 위치를 **도착 페이지 키에**
	 * 덮어써서, 뒤로가기 복원이 항상 0 이 된다(실측으로 잡음). 그래서 이동
	 * 시에는 `nav.from` 의 주소를 명시적으로 넘긴다.
	 */
	function saveScrollPosition(key?: string) {
		if (typeof window === 'undefined') return;
		try {
			sessionStorage.setItem(
				SCROLL_KEY_PREFIX + (key ?? window.location.pathname + window.location.search),
				String(pageScrollTop())
			);
		} catch {
			/* quota / disabled — ignore */
		}
	}
	function restoreScrollPosition() {
		if (typeof window === 'undefined') return;
		try {
			const raw = sessionStorage.getItem(
				SCROLL_KEY_PREFIX + window.location.pathname + window.location.search
			);
			if (raw === null) return;
			const y = parseInt(raw, 10);
			if (!Number.isFinite(y) || y <= 0) return;
			// DEV-126 fix: 페이지 본문은 onMount fetch 후 비동기로 자라난다.
			// rAF 두 번만으로는 컨텐츠 높이가 아직 y 에 못 미쳐 scrollTo 가 짧게
			// clamp 되는 경우가 많았다 (= 복원 안 됨). 목표 y 에 도달 가능할
			// 때까지 (scrollHeight 충분) 또는 최대 ~1.2초 동안 짧은 간격으로 재시도.
			let tries = 0;
			const MAX_TRIES = 40; // 40 × ~30ms ≈ 1.2s
			const attempt = () => {
				scrollPageTo(y);
				tries += 1;
				const reached = Math.abs(pageScrollTop() - y) <= 2;
				const tallEnough = pageScrollHeight() - pageViewportHeight() >= y;
				if (reached || tallEnough || tries >= MAX_TRIES) return;
				setTimeout(attempt, 30);
			};
			// BUG-257: 시작을 rAF 에만 걸어 두면 **화면이 안 보이는 동안 복원이
			// 아예 안 된다** — 백그라운드 탭이나 숨겨진 창에서 rAF 가 멈추기
			// 때문이다(자동 검증 중에 그대로 재현됐다). 아래 재시도 루프가 이미
			// setTimeout 이므로 시작도 같은 방식으로 맞춘다. 레이아웃을 기다리는
			// 역할은 루프의 `tallEnough` 조건이 대신한다.
			setTimeout(attempt, 0);
		} catch {
			/* ignore */
		}
	}
	onMount(() => {
		// reload 직전 / 페이지 떠나기 직전에 저장.
		const onBeforeUnload = () => saveScrollPosition();
		// 주기적 저장 (throttled scroll listener) — 강제 종료 / 크래시 대비.
		let lastSave = 0;
		const onScroll = () => {
			const now = Date.now();
			if (now - lastSave < 200) return;
			lastSave = now;
			saveScrollPosition();
		};
		window.addEventListener('beforeunload', onBeforeUnload);
		window.addEventListener('pagehide', onBeforeUnload);
		// BUG-257: 컨테이너 스크롤은 window 로 버블하지 않는다 — window 에 붙여
		// 두면 이 주기 저장이 조용히 죽어 새로고침 복원이 안 된다.
		const offScroll = onPageScroll(onScroll);
		// 첫 mount 시 — 마지막 저장값 있으면 복원.
		restoreScrollPosition();
		return () => {
			window.removeEventListener('beforeunload', onBeforeUnload);
			window.removeEventListener('pagehide', onBeforeUnload);
			offScroll();
		};
	});

	// DEV-101: UI 크기 — root font-size scale 영속 store 의 현재 값을 매 변경마다
	// `<html>` 에 반영. HTTP / Tauri 양쪽 동일 (rem 기반 layout).
	onMount(() => {
		const unsub = uiScale.subscribe(applyUiScaleToDocument);
		// 첫 mount 시 한 번 더 — onMount 보다 store 가 먼저 init 됐다면 noop.
		return () => unsub();
	});

	// DEV-335: 첨부 이미지 HDR 표시 제한 — `<html>` 의 `--hdr-limit` 갱신.
	onMount(() => {
		const unsub = hdrLimit.subscribe(applyHdrLimitToDocument);
		return () => unsub();
	});

	// DEV-101 fix2: 컨텐츠 영역 폭 — `<html>` 의 `--content-max-width` 토큰 갱신.
	// 페이지 max-width: var(--content-max-width, …) 사용처가 자동 반응.
	// BUG-141: uiScale 과 동일하게 rAF 병합 — 슬라이더 드래그가 매 pointermove
	// 마다 CSS 변수를 갱신하면(→ 전체 reflow) Linux(WebKitGTK)에서 버벅였다.
	onMount(() => {
		let rafId: number | null = null;
		let pendingW = 0;
		const unsub = contentWidth.subscribe((w) => {
			if (typeof document === 'undefined') return;
			pendingW = w;
			if (rafId !== null) return;
			rafId = requestAnimationFrame(() => {
				rafId = null;
				// DEV-275: 최대값이면 'none' — 폭 제한 해제(화면 전체).
				document.documentElement.style.setProperty(
					'--content-max-width',
					contentWidthCss(pendingW)
				);
				// BUG-064 후속: 고정 폭 팝업/모달이 '컨텐츠 폭' 설정에 비례하도록
				// --popup-scale 토큰 발급. 기준 1100px = 1.0, 0.9~1.3 으로 clamp
				// (너무 좁거나 과하게 넓어지지 않게). 팝업 width 는
				// calc(<base>rem * var(--popup-scale)) 로 참조.
				const scale = Math.max(0.9, Math.min(1.3, pendingW / 1100));
				document.documentElement.style.setProperty('--popup-scale', scale.toFixed(3));
			});
		});
		return () => {
			if (rafId !== null) cancelAnimationFrame(rafId);
			unsub();
		};
	});

	// DEV-074: 테마 — store 변경 시 `<html data-theme>` 갱신. 'system' 일 때
	// OS preference 변경도 listener 로 즉시 반영.
	// DEV-201: 동시에 Tauri 네이티브 창 테마(Windows 타이틀바 등)도 동기화 —
	// 시스템이 라이트인데 앱을 다크로 둔 경우 타이틀바가 흰색으로 튀던 문제 해결.
	async function applyWindowTheme(t: ThemeChoice) {
		if (detectEnvironment() !== 'tauri') return;
		try {
			const { getCurrentWindow } = await import('@tauri-apps/api/window');
			// 'dark'/'light' = 강제(앱 선택 우선), 'system' = null(OS 따름).
			// Windows 에선 이게 immersive dark mode 로 네이티브 타이틀바를 칠한다.
			await getCurrentWindow().setTheme(t === 'system' ? null : t);
		} catch (e) {
			console.warn('[theme] 네이티브 창 테마 적용 실패', e);
		}
	}
	onMount(() => {
		const applyAll = (t: ThemeChoice) => {
			applyThemeToDocument(t);
			void applyWindowTheme(t);
		};
		const unsubTheme = theme.subscribe(applyAll);
		// DEV-114: 활성 커스텀 프리셋 복원 — base 테마 적용(위 subscribe 초기
		// 발화) 후 override 를 얹는다. DEV-249: Tauri 는 ~/.openguild/themes.json
		// 로드(async) 포함.
		void initCustomTheme();
		const unwatchSys = watchSystemPreference(() => {
			// system 모드일 때만 재적용 (다른 모드는 사용자가 명시 — OS 변경 무시).
			// 창 테마는 system=null 이라 OS 가 알아서 따라가므로 document 만 재적용.
			// BUG-121: JS 가 색을 직접 계산하는 컴포넌트(QuestBoard 의 Cytoscape/SVG)
			// 는 `effectiveTheme` 스토어를 구독 — 그쪽은 theme.ts 자체에서
			// matchMedia listener 로 갱신되므로 여기서 별도 처리 불필요.
			if (get(theme) === 'system') {
				applyThemeToDocument('system');
			}
		});
		return () => {
			unsubTheme();
			unwatchSys();
		};
	});

	// BUG-031 / BUG-033: Tauri 데스크탑 앱에서 웹 기본 우클릭 메뉴
	// (Inspect / Reload / Back / Forward) 노출 차단.
	// - capture phase 로 등록해 다른 핸들러가 먼저 e.preventDefault() 를 호출
	//   할 가능성 없이 가장 먼저 받음.
	// - document 와 window 둘 다 등록 (WebView2 환경별 fallback).
	// - 브라우저 (`http`) 에서는 dev 편의를 위해 그대로 둠.
	onMount(() => {
		if (detectEnvironment() !== 'tauri') return;
		const block = (e: MouseEvent) => {
			e.preventDefault();
			e.stopPropagation();
			return false;
		};
		// capture: true — bubbling 단계가 아닌 capture 단계에서 즉시 차단.
		document.addEventListener('contextmenu', block, { capture: true });
		window.addEventListener('contextmenu', block, { capture: true });

		// BUG-040: 외부 링크 (`http://...` / `https://...`) 클릭 시 webview 안에서
		// navigate 되면 SPA 가 사라짐 → 시스템 브라우저로 강제. 내부 anchor
		// (`/quests/...`) 와 빈 / # / javascript: 는 그대로 통과.
		const interceptExternalLink = async (e: MouseEvent) => {
			// 가장 가까운 a 찾기 — 자식 element 클릭에도 동작.
			const path = e.composedPath() as Element[];
			const anchor = path.find((el) => el instanceof HTMLAnchorElement) as
				| HTMLAnchorElement
				| undefined;
			if (!anchor) return;
			const href = anchor.getAttribute('href') ?? '';
			if (!/^https?:\/\//i.test(href)) return; // internal / hash / javascript: → pass
			e.preventDefault();
			e.stopPropagation();
			try {
				const { openUrl } = await import('@tauri-apps/plugin-opener');
				await openUrl(href);
			} catch (err) {
				console.error('[opener] failed', err);
				// fallback — 마지막 안전망. window.open 은 webview 안 새 창이지만
				// 적어도 SPA 가 사라지진 않음.
				window.open(href, '_blank');
			}
		};
		document.addEventListener('click', interceptExternalLink, { capture: true });

		// DEV-069: tauri.conf 의 dragDropEnabled=false 로 webview 가 HTML5 drag&drop
		// 을 직접 받게 했다(편집기 첨부용). 그 부작용으로 편집기 '밖' 에 파일을
		// 떨구면 WebView 가 그 파일 URL 로 navigate → SPA 소실. 전역 가드로 기본
		// 동작을 막는다. 편집기(CodeMirror)의 자체 drop 핸들러는 target 단계에서
		// 먼저 동작하므로 첨부 업로드는 정상.
		const dropGuard = (e: DragEvent) => {
			// 파일이 포함된 drag 만 가드 (텍스트 선택 drag&drop 은 통과).
			if (e.dataTransfer && Array.from(e.dataTransfer.types).includes('Files')) {
				e.preventDefault();
			}
		};
		window.addEventListener('dragover', dropGuard);
		window.addEventListener('drop', dropGuard);

		return () => {
			document.removeEventListener('contextmenu', block, { capture: true });
			window.removeEventListener('contextmenu', block, { capture: true });
			document.removeEventListener('click', interceptExternalLink, {
				capture: true
			});
			window.removeEventListener('dragover', dropGuard);
			window.removeEventListener('drop', dropGuard);
		};
	});
</script>

{#if showTitleBar}
	<TitleBar />
{/if}
{#if showNav}
	<Nav />
{/if}
<main class:no-nav={!showNav} bind:this={mainEl}>
	{#if historyGuildSwitching}
		<div class="history-guild-switch" role="status" aria-live="polite">
			<span class="history-guild-spinner" aria-hidden="true"></span>
			{t('history.switchingGuild', $locale)}
		</div>
	{:else}
		{@render children()}
	{/if}
</main>
{#if !coarsePointer}
	<OverlayScrollbar target={mainEl ?? null} />
{/if}
<!-- DEV-259: 알림 통합 호스트(토스트/업데이트/스키마) — 우하단 단일 스택.
     업데이트·스키마 watcher 내장. 모든 라우트 공통 단일 mount. -->
<ToastHost />

<!-- DEV-153: 미저장 변경 시 라우트 이동 확인 (모든 페이지 공통). -->
<ConfirmDialog
	open={showUnsavedModal}
	title="편집 중 이동"
	message={'저장하지 않은 변경 사항이 있습니다.\n버리고 이동할까요?'}
	confirmLabel="버리고 이동"
	danger
	onconfirm={discardAndProceed}
	oncancel={keepEditing}
/>

<style>
	main {
		/* BUG-257: 문서 대신 여기가 스크롤 컨테이너다(자세한 배경은
		   `utils/page-scroll.ts`). min-height 가 아니라 **정확한 height** 여야
		   자기 안에서 스크롤이 생긴다 — min-height 면 컨텐츠만큼 늘어나
		   고정된 문서 밖으로 넘쳐 잘린다.
		   `overscroll-behavior: contain` 은 여기서 끝까지 스크롤했을 때 그
		   제스처가 문서로 넘어가지 않게 한다(문서는 어차피 고정이지만, 상위로
		   새는 것을 명시적으로 막아 둔다). */
		height: calc(100vh - var(--nav-h, 3.25rem) - var(--titlebar-h, 0px));
		overflow-y: auto;
		overflow-x: hidden;
		overscroll-behavior: contain;
		background: var(--bg);
	}
	main.no-nav {
		height: calc(100vh - var(--titlebar-h, 0px));
	}
	.history-guild-switch {
		display: flex;
		/* BUG-257: 예전엔 `min-height: inherit` 로 main 의 min-height 를 물려받아
		   화면 한가운데에 섰다. main 이 `height` 로 바뀌면서 물려받을 min-height
		   가 사라져(=auto) 스피너가 위쪽에 붙었다. main 이 이제 확정 높이를
		   가지므로 `100%` 로 같은 결과를 얻는다. */
		min-height: 100%;
		align-items: center;
		justify-content: center;
		gap: 0.65rem;
		color: var(--text-muted);
		font-size: 0.9rem;
	}
	.history-guild-spinner {
		width: 1rem;
		height: 1rem;
		border: 2px solid var(--border);
		border-top-color: var(--accent);
		border-radius: 50%;
		animation: history-guild-spin 0.75s linear infinite;
	}
	@keyframes history-guild-spin {
		to {
			transform: rotate(360deg);
		}
	}
</style>
