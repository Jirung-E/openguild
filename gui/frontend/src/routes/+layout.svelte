<script lang="ts">
	import { page } from '$app/stores';
	import { afterNavigate } from '$app/navigation';
	import { onMount } from 'svelte';
	import Nav from '$lib/components/Nav.svelte';
	import UpdateBanner from '$lib/components/UpdateBanner.svelte';
	import SchemaAheadBanner from '$lib/components/SchemaAheadBanner.svelte';
	// DEV-074 fix13: window 스크롤 overlay — 컨텐츠 폭 차지 X.
	import OverlayScrollbar from '$lib/components/OverlayScrollbar.svelte';
	import { detectEnvironment } from '$lib/api/transport';
	import { uiScale, applyUiScaleToDocument } from '$lib/stores/uiScale';
	import { contentWidth } from '$lib/stores/contentWidth';
	import {
		theme,
		applyThemeToDocument,
		watchSystemPreference,
		resolveTheme
	} from '$lib/stores/theme';
	import { get } from 'svelte/store';
	import '$lib/styles/global.css';

	let { children } = $props();

	// DEV-052 후속: /welcome 라우트에선 Nav (Board/List/Admin/+New Quest) 숨김.
	// 길드 컨텍스트가 없는 상태에서 의미 없는 액션 노출 방지.
	let showNav = $derived($page.url.pathname !== '/welcome');

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
			.querySelectorAll<HTMLElement>(
				'body > svg[id^="mm-"], body > div[id^="dmm-"]'
			)
			.forEach((el) => el.remove());
	}
	onMount(sweepMermaidLeftovers);
	afterNavigate(sweepMermaidLeftovers);

	// DEV-126: 페이지 새로고침 후 스크롤 위치 유지.
	// SvelteKit 의 SPA navigation 은 자동으로 위치 관리 (afterNavigate 가 history
	// state 따라 복원) 하지만, F5 / window.location.reload() / DEV-120 의 admin
	// reindex 후 reload 는 page 자체 reload — 항상 top 으로. sessionStorage 에
	// path 별 scrollY 를 저장해서 reload 시 복원.
	const SCROLL_KEY_PREFIX = 'openguild.scroll.';
	function saveScrollPosition() {
		if (typeof window === 'undefined') return;
		try {
			sessionStorage.setItem(
				SCROLL_KEY_PREFIX + window.location.pathname + window.location.search,
				String(window.scrollY)
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
			if (Number.isFinite(y) && y > 0) {
				// 다음 tick — 컨텐츠 mount 끝나야 정확. requestAnimationFrame 두 번
				// (DOM + layout) 후가 가장 안정.
				requestAnimationFrame(() => {
					requestAnimationFrame(() => window.scrollTo({ top: y, left: 0 }));
				});
			}
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
		window.addEventListener('scroll', onScroll, { passive: true });
		// 첫 mount 시 — 마지막 저장값 있으면 복원.
		restoreScrollPosition();
		return () => {
			window.removeEventListener('beforeunload', onBeforeUnload);
			window.removeEventListener('pagehide', onBeforeUnload);
			window.removeEventListener('scroll', onScroll);
		};
	});

	// DEV-101: UI 크기 — root font-size scale 영속 store 의 현재 값을 매 변경마다
	// `<html>` 에 반영. HTTP / Tauri 양쪽 동일 (rem 기반 layout).
	onMount(() => {
		const unsub = uiScale.subscribe(applyUiScaleToDocument);
		// 첫 mount 시 한 번 더 — onMount 보다 store 가 먼저 init 됐다면 noop.
		return () => unsub();
	});

	// DEV-101 fix2: 컨텐츠 영역 폭 — `<html>` 의 `--content-max-width` 토큰 갱신.
	// 페이지 max-width: var(--content-max-width, …) 사용처가 자동 반응.
	onMount(() => {
		const unsub = contentWidth.subscribe((w) => {
			if (typeof document !== 'undefined') {
				document.documentElement.style.setProperty('--content-max-width', `${w}px`);
			}
		});
		return () => unsub();
	});

	// DEV-074: 테마 — store 변경 시 `<html data-theme>` 갱신. 'system' 일 때
	// OS preference 변경도 listener 로 즉시 반영.
	onMount(() => {
		const unsubTheme = theme.subscribe(applyThemeToDocument);
		const unwatchSys = watchSystemPreference(() => {
			// system 모드일 때만 재적용 (다른 모드는 사용자가 명시 — OS 변경 무시).
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
			const anchor = path.find(
				(el) => el instanceof HTMLAnchorElement
			) as HTMLAnchorElement | undefined;
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

		return () => {
			document.removeEventListener('contextmenu', block, { capture: true });
			window.removeEventListener('contextmenu', block, { capture: true });
			document.removeEventListener('click', interceptExternalLink, {
				capture: true
			});
		};
	});
</script>

<!-- BUG-041: DB schema 가 binary 보다 새로운 경우 알림 (Tauri 만). 항상 최상단. -->
<SchemaAheadBanner />
<!-- DEV-063: 업데이트 배너 — Nav 아래, 새 버전 있을 때만 노출. 모든 라우트
     (welcome 포함) 공통. -->
<UpdateBanner />
{#if showNav}
	<Nav />
{/if}
<main class:no-nav={!showNav}>
	{@render children()}
</main>
<OverlayScrollbar />

<style>
	main {
		min-height: calc(100vh - 3.25rem);
		background: var(--bg);
	}
	main.no-nav {
		min-height: 100vh;
	}
</style>
