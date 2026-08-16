<!--
  DEV-253: 전 문서 검색 팔레트 (vscode Quick Open 식).

  타이틀바 중앙의 길드 이름 pill 을 누르면 열린다. 길드의 모든 문서
  (퀘스트 / 캠페인 / 규칙 / 도서관)를 제목·식별자·태그로 검색한다.
  `#태그` 로 시작하면 태그 전용 검색.

  결과 선택(Enter / 클릭) 시 페이지 이동 없이 내용 미리보기 팝업을 띄우고,
  거기서 "페이지로 이동" 으로 실제 라우트 이동. 미리보기 본문은 아래
  가장자리 핸들로 세로 크기 조절 가능(기존 OverlayScrollbar 사용).

  검색은 클라이언트 사이드 — 각 타입의 기존 list API 를 병합해 필터.
  본문 미리보기는 선택 시점에 상세 API 로 지연 로드.
-->
<script lang="ts">
	import { onMount } from 'svelte';
	import { goto, afterNavigate } from '$app/navigation';
	import { questsApi } from '$lib/api/quests';
	import { campaignsApi } from '$lib/api/campaigns';
	import { rulesApi } from '$lib/api/rules';
	import { libraryApi } from '$lib/api/library';
	import MarkdownView from './MarkdownView.svelte';
	// BUG-157: 팔레트의 결과 목록/미리보기 본문도 overlay 스크롤바 —
	// 콤보박스·퀘스트 목록 등 다른 스크롤 영역과 같은 규칙(컨텐츠 폭 0 차지).
	import OverlayScrollbar from './OverlayScrollbar.svelte';
	// 크로스링크(`[[kind:ID]]`)와 동일한 네임스페이스 별칭 — 검색 범위 좁히기.
	import { KIND_ALIASES } from '$lib/stores/questIndex';
	// DEV-255: 결과 열기 방식(미리보기/자식윈도우/페이지이동) 선택 — 공용 헬퍼.
	import { openInWindow, openInPage } from '$lib/utils/open-item';
	// DEV-205(3차): i18n — KIND_LABEL(ko 고정) 대신 t() 기반 kindLabel().
	import { locale, t } from '$lib/stores/locale';
	// DEV-294: recent 모드에서 최근 본 문서 순서 소스.
	import { recentDocs } from '$lib/stores/recentDocs';
	// DEV-297: 전체 제목은 네이티브 title 대신 앱 스타일 커스텀 팝업으로.
	import { titlePopup } from '$lib/actions/title-popup';
	import { isPointerDrivenHover } from '$lib/utils/hover-guard';

	// DEV-294: `mode='recent'` — 별도 드롭다운을 만들지 않고 이 팔레트를 그대로
	// 재사용해 "최근 본 문서"를 보여준다(폭·행 레이아웃·미리보기·스크롤 전부 공유).
	// 검색어를 입력하는 순간 일반 검색과 동일하게 동작.
	let { onclose, mode = 'search' }: { onclose: () => void; mode?: 'search' | 'recent' } = $props();

	type Kind = 'quest' | 'campaign' | 'rule' | 'book';
	interface Item {
		kind: Kind;
		label: string; // "DEV-253" / "C-001" / rule slug / "BOOK-012"
		title: string;
		tags: string[];
		href: string;
		// 상태 등 짧은 부가 정보 — 언어 토글에 반응하도록 양쪽 언어를 들고
		// 렌더 시점에 고른다(목록은 loadAll 1회 적재라 재계산이 안 됨).
		metaKo: string;
		metaEn: string;
		load: () => Promise<string>; // 미리보기 본문(markdown) 지연 로더
	}

	function kindLabel(k: Kind): string {
		return t(`kind.${k}`, $locale);
	}
	function metaOf(it: Item): string {
		return $locale === 'en' ? it.metaEn : it.metaKo;
	}

	let all = $state<Item[]>([]);
	let loading = $state(true);
	let query = $state('');
	let selIndex = $state(0);
	let inputEl = $state<HTMLInputElement | null>(null);
	// DEV-255 버그 수정: 방향키로 선택이 화면 밖으로 나가도 스크롤 안 되던 문제
	// — 선택 행을 scrollIntoView 하기 위한 목록 컨테이너 참조.
	let rowsEl = $state<HTMLDivElement | null>(null);
	// BUG-157: 미리보기 본문도 overlay 스크롤바 대상.
	let previewBodyEl = $state<HTMLDivElement | null>(null);

	// 미리보기 상태.
	let preview = $state<Item | null>(null);
	let previewBody = $state('');
	let previewLoading = $state(false);
	let previewH = $state(220);

	onMount(() => {
		inputEl?.focus();
		void loadAll();
	});

	// DEV-255 회귀 수정(3차): `<svelte:window onclick>` 을 쓰면 컴포넌트 생성
	// 즉시(=같은 tick) 리스너가 등록된다. 그런데 팔레트를 여는 그 클릭(타이틀바
	// 검색 pill) 자체가 아직 window 까지 버블링 중이던 이벤트라, 새로 등록된
	// 리스너가 그 "여는 클릭"에도 반응해 열리자마자 다시 닫혀버렸다("검색
	// 팔레트 안 열림" 재현). setTimeout(0) 지연으로 임시 봉합했었으나 — 왜
	// 동작하는지 불분명한 타이밍 hack이라 근본 원인에 맞는 방식으로 교체.
	//
	// 근본 원인은 이벤트 종류: 한 번의 클릭은 mousedown → mouseup → click
	// 순서로 발생하고 그중 click 이 가장 마지막. "여는 클릭"과 같은 이벤트
	// 종류(click)로 바깥-클릭 감지를 걸면 그 이벤트가 window 까지 버블링되는
	// 도중에 리스너가 새로 붙어 자기 자신을 잡는다. mousedown 으로 감지하면
	// 그 시점엔 이미 "여는 클릭"의 mousedown 이 완전히 끝난 뒤라 새로 등록된
	// 리스너가 되짚어 잡을 이벤트가 없음 — 다음 실제 mousedown 부터만 반응.
	// 지연/타이머 없이 동작 원리로 해결(Radix/Headless UI 등도 outside-press
	// 감지에 pointerdown/mousedown 을 쓰는 이유와 동일).
	//
	// DEV-255 회귀 수정(4차 — "타이틀바 클릭으로 안 닫힘"): Tauri 가 주입하는
	// drag-region 스크립트(tauri src/window/scripts/drag.js)가 titlebar
	// (data-tauri-drag-region) 위 mousedown 에서 `e.stopImmediatePropagation()`
	// 을 호출한다. 그 핸들러는 document(버블)에 붙어 있어, window(버블,
	// document 다음)에 붙인 우리 리스너에는 이벤트가 아예 도달하지 않았음 —
	// 타이틀바 빈 영역 클릭이 팔레트를 못 닫던 진짜 원인. capture 단계는
	// window → document → target 순서로 어떤 핸들러보다도 먼저 실행되고
	// stopImmediatePropagation 의 영향도 받지 않으므로 capture 로 등록한다.
	// (여는 클릭의 mousedown 은 mount 이전에 이미 끝났으므로 capture 여도
	// 자기 자신을 닫는 문제는 재발하지 않음.)
	onMount(() => {
		window.addEventListener('mousedown', onWindowMouseDown, { capture: true });
		return () => window.removeEventListener('mousedown', onWindowMouseDown, { capture: true });
	});

	// DEV-255 후속(사용자 요청): Esc 로 닫기 — 이전엔 입력박스의 onkeydown 에만
	// 걸려 있어 커서가 입력박스 밖(미리보기 스크롤 후 등)이면 Esc 가 안 먹혔다.
	// window 레벨로 옮겨 포커스 위치와 무관하게 동작. 입력박스에 포커스가 있어도
	// 이벤트는 여기까지 버블되므로 단일 경로로 처리(onKey/backdrop 의 기존 Esc
	// 처리는 중복 방지 위해 제거).
	function onWindowKeyDown(e: KeyboardEvent) {
		if (e.key !== 'Escape') return;
		e.preventDefault();
		if (preview) preview = null;
		else onclose();
	}
	onMount(() => {
		window.addEventListener('keydown', onWindowKeyDown);
		return () => window.removeEventListener('keydown', onWindowKeyDown);
	});

	// DEV-255 버그 수정: 방향키 이동 시 선택 행이 보이도록 스크롤.
	// DEV-255 후속(사용자 보고 "스크롤이 이상함"): selIndex 는 마우스 호버
	// (onmouseenter)로도 바뀌는데, 그때마다 scrollIntoView 를 부르면 커서가
	// 목록 위를 지나갈 때마다 스크롤이 강제로 움직여 덜컥거렸다 — 크로스링크
	// 자동완성(QuestCommentsSection, BUG-114)에서 이미 확립한 대로 키보드
	// (↑/↓) 이동일 때만 스크롤하고 마우스 호버는 무시한다.
	let selFromKeyboard = false;
	// DEV-297 수정: 선택된 행은 팝업 대신 **제자리에서 펼쳐** 전체 제목을
	// 보여준다 — 팝업이 위/아래 행을 통째로 가렸다(admin 보고).
	// DEV-359: 호버로 고른 행도 같다. 제목 자리를 항상 2줄 확보해 두고 선택된
	// 행만 말줄임을 푸는 방식이라(스타일 참고) **높이가 아예 변하지 않는다** —
	// 스크롤 보정도, 펼침 애니메이션도, 그로 인한 흔들림도 없다.
	$effect(() => {
		void selIndex;
		if (preview || !rowsEl) return;
		if (!selFromKeyboard) return;
		selFromKeyboard = false;
		const el = rowsEl.children[selIndex] as HTMLElement | undefined;
		el?.scrollIntoView({ block: 'nearest' });
	});

	// DEV-255 버그 수정: 타이틀바(메뉴 버튼 포함)를 눌러도 팔레트가 안 꺼지던
	// 문제 — 기존 backdrop 은 titlebar 영역을 제외(inset: titlebar-h)해서
	// 그 위 클릭이 안 잡혔다. window 레벨로 팔레트 바깥 클릭을 감지해 닫는다.
	function onWindowMouseDown(e: MouseEvent) {
		const target = e.target as HTMLElement;
		if (!target.closest('.palette')) onclose();
	}

	// DEV-255 버그 수정: 팔레트가 열린 채로 뒤로/앞으로가기(타이틀바 버튼·
	// 마우스 사이드버튼·단축키 등 어떤 경로든)가 되면 팔레트가 남아있던 문제
	// — 어떤 이유로든 라우트가 바뀌면 팔레트를 닫는다.
	// DEV-255 회귀 수정: `afterNavigate` 는 "컴포넌트가 mount 될 때도" 한 번
	// 호출된다(SvelteKit 문서: "runs ... when the current component mounts,
	// and also whenever we navigate") — 그 첫 호출(type === 'enter')까지
	// onclose() 를 태워서 팔레트가 열리자마자 닫혀버렸다(검색 팔레트 안 열림
	// 버그). 실제 라우트 전환(mount 이후)만 걸러서 닫는다.
	afterNavigate((nav) => {
		if (nav.type === 'enter') return;
		onclose();
	});

	async function loadAll() {
		loading = true;
		try {
			const [quests, camps, rules, books] = await Promise.all([
				// DEV-277: 최근 갱신순 — 검색 전 첫 화면에 최근 손댄 문서가 위로.
				questsApi.listRecent(true).catch(() => []),
				campaignsApi.list().catch(() => []),
				rulesApi.list().catch(() => ({ entries: [] })),
				libraryApi.list().catch(() => [])
			]);
			const items: Item[] = [];
			for (const q of quests) {
				items.push({
					kind: 'quest',
					label: q.quest_id,
					title: q.title,
					tags: q.tags ?? [],
					// BUG(발견 2026-07-14): 이전엔 q.id(숫자 row id) 사용 — [id] 라우트는
					// getBySlug(quest_id 문자열) 로 조회해 "이동"/"자식창" 모두 빈 화면.
					href: `/quests/${q.quest_id}`,
					metaKo: q.status_name_ko || q.status_name_en,
					metaEn: q.status_name_en || q.status_name_ko,
					load: async () => (await questsApi.get(q.id)).description ?? ''
				});
			}
			for (const c of camps) {
				items.push({
					kind: 'campaign',
					label: c.campaign_slug,
					title: c.title,
					tags: [],
					href: `/campaigns/${encodeURIComponent(c.campaign_slug)}`,
					metaKo: String(c.status),
					metaEn: String(c.status),
					load: async () => (await campaignsApi.get(c.campaign_slug)).description ?? ''
				});
			}
			for (const r of rules.entries) {
				items.push({
					kind: 'rule',
					// 규칙은 slug 가 곧 식별자 — 별도 제목 없음(중복 표시 방지).
					label: r.slug,
					title: '',
					tags: r.tags ?? [],
					href: `/rules?slug=${encodeURIComponent(r.slug)}`,
					metaKo: '규칙',
					metaEn: 'Rule',
					load: async () => r.content ?? ''
				});
			}
			for (const b of books) {
				items.push({
					kind: 'book',
					label: b.book_id,
					title: b.title,
					tags: b.tags ?? [],
					href: `/library?id=${encodeURIComponent(b.book_id)}`,
					metaKo: '도서관',
					metaEn: 'Library',
					load: async () => (await libraryApi.get(b.book_id)).body ?? ''
				});
			}
			all = items;
		} finally {
			loading = false;
		}
	}

	// 입력을 `namespace:` 접두 + 나머지 검색어로 분리. 크로스링크(`[[kind:ID]]`)와
	// 동일한 별칭 테이블 재사용: quest/q · campaign/c · rule/rules/r · book/library/lib.
	const parsed = $derived.by((): { kind: Kind | null; term: string } => {
		let raw = query.trim();
		const ci = raw.indexOf(':');
		if (ci > 0) {
			const prefix = raw.slice(0, ci).toLowerCase();
			const k = KIND_ALIASES[prefix];
			if (k) return { kind: k, term: raw.slice(ci + 1).trim() };
		}
		return { kind: null, term: raw };
	});

	// 활성 네임스페이스 범위 — 있으면 그 종류만 검색(범위 칩 표시용).
	const scopeKind = $derived(parsed.kind);

	const filtered = $derived.by(() => {
		const { kind, term } = parsed;
		// DEV-294: recent 모드 + 검색어 없음 → 최근 본 문서만, 최근순.
		if (mode === 'recent' && !term && !kind) {
			const order = new Map($recentDocs.map((d, i) => [d.href, i]));
			return all
				.filter((i) => order.has(i.href))
				.sort((a, b) => (order.get(a.href) ?? 0) - (order.get(b.href) ?? 0));
		}
		const pool = kind ? all.filter((i) => i.kind === kind) : all;
		// 결과 개수 상한 없음 — 단순 행이라 문서가 많아도 렌더 부담 미미, 영역 스크롤.
		if (!term) return pool;
		if (term.startsWith('#')) {
			const tag = term.slice(1).toLowerCase();
			if (!tag) return pool.filter((i) => i.tags.length > 0);
			return pool.filter((i) => i.tags.some((tg) => tg.toLowerCase().includes(tag)));
		}
		const q = term.toLowerCase();
		return pool.filter(
			(i) =>
				i.title.toLowerCase().includes(q) ||
				i.label.toLowerCase().includes(q) ||
				i.tags.some((tg) => tg.toLowerCase().includes(q))
		);
	});

	// 필터가 바뀌어 선택 index 가 범위를 벗어나면 리셋.
	$effect(() => {
		if (selIndex >= filtered.length) selIndex = 0;
	});

	async function openPreview(it: Item) {
		preview = it;
		previewLoading = true;
		previewBody = '';
		try {
			previewBody = await it.load();
			if (!previewBody.trim()) previewBody = t('palette.emptyBody', $locale);
		} catch {
			previewBody = t('palette.previewLoadFail', $locale);
		} finally {
			previewLoading = false;
		}
	}

	// DEV-255: 페이지로 이동 — 현재 창 라우팅 + 팔레트 닫기(기존 동작 유지).
	function goItem(it: Item) {
		openInPage(it.href);
		onclose();
	}

	// DEV-255: 항목별 새 창 — 팔레트는 열어둔 채 유지(여러 개 동시에 띄우고
	// 비교하는 사용 흐름 지원, AskUserQuestion 결정).
	function windowItem(it: Item) {
		void openInWindow(it.href, displayName(it));
	}

	// 표시 이름 — 규칙처럼 title 이 비면 label 만(중복/후행 공백 방지).
	function displayName(it: Item): string {
		return it.title ? `${it.label} ${it.title}` : it.label;
	}

	// Esc 는 window 레벨(onWindowKeyDown)에서 단일 처리 — 여기선 목록 탐색만.
	function onKey(e: KeyboardEvent) {
		if (preview) return;
		if (e.key === 'ArrowDown') {
			e.preventDefault();
			selFromKeyboard = true;
			selIndex = Math.min(selIndex + 1, filtered.length - 1);
		} else if (e.key === 'ArrowUp') {
			e.preventDefault();
			selFromKeyboard = true;
			selIndex = Math.max(selIndex - 1, 0);
		} else if (e.key === 'Enter') {
			e.preventDefault();
			const it = filtered[selIndex];
			if (it) void openPreview(it);
		}
	}

	// 미리보기 아래 가장자리 = 세로 크기 조절 핸들.
	function startResize(e: MouseEvent) {
		e.preventDefault();
		const startY = e.clientY;
		const startH = previewH;
		const onMove = (ev: MouseEvent) => {
			previewH = Math.max(90, Math.min(460, startH + (ev.clientY - startY)));
		};
		const onUp = () => {
			window.removeEventListener('mousemove', onMove);
			window.removeEventListener('mouseup', onUp);
			document.body.style.userSelect = '';
		};
		document.body.style.userSelect = 'none';
		window.addEventListener('mousemove', onMove);
		window.addEventListener('mouseup', onUp);
	}
</script>

<!-- DEV-255 버그 수정: 팔레트 바깥(타이틀바 포함) 아무 데나 클릭해도 닫힘.
     리스너(mousedown, 위 onMount)는 여는 클릭과 이벤트 종류가 달라 자기
     자신을 잡지 않음 — 상세 이유는 onMount 주석 참고. -->

<!-- 바깥 클릭으로 닫기. backdrop 은 투명 — 콘텐츠를 어둡게 덮지 않음.
     Esc 는 window 레벨(onWindowKeyDown)에서 단일 처리 — 키보드 접근성은
     그쪽이 담당하므로 backdrop 자체 keydown 은 불필요. -->
<!-- svelte-ignore a11y_click_events_have_key_events -->
<div
	class="backdrop"
	role="button"
	tabindex="-1"
	aria-label={t('palette.closeAria', $locale)}
	onclick={onclose}
></div>

<div class="palette" role="dialog" aria-label={t('palette.dialogAria', $locale)}>
	{#if !preview}
		<div class="input-wrap">
			{#if scopeKind}
				<span class="scope-chip {scopeKind}"
					>{kindLabel(scopeKind)}{t('palette.scopeOnly', $locale)}</span
				>
			{/if}
			<input
				bind:this={inputEl}
				bind:value={query}
				onkeydown={onKey}
				placeholder={t(
					mode === 'recent' ? 'palette.placeholderRecent' : 'palette.placeholder',
					$locale
				)}
				spellcheck="false"
			/>
		</div>
		<div class="rows" bind:this={rowsEl}>
			{#if loading}
				<div class="empty">{t('palette.loading', $locale)}</div>
			{:else if filtered.length === 0}
				<div class="empty">
					{t(
						mode === 'recent' && !query.trim() ? 'palette.noRecent' : 'palette.noResults',
						$locale
					)}
				</div>
			{:else}
				{#each filtered as it, i (it.kind + it.label)}
					<!-- DEV-255: 행 = 라벨(기본 클릭 = 미리보기) + 열기 방식 아이콘 3개(항상 노출). -->
					<!-- svelte-ignore a11y_no_static_element_interactions -->
					<div
						class="row"
						class:sel={i === selIndex}
						class:expanded={i === selIndex}
						onmouseenter={(ev) => {
							// DEV-359: 굴리는 중에 행이 커서 밑을 지나가며 들어오는 hover(커서는
							// 가만히 있다)는 무시한다 — 선택이 휠을 따라다니면 걸리는 느낌이 난다.
							if (!isPointerDrivenHover(ev)) return;
							selIndex = i;
						}}
					>
						<button
							class="row-main"
							onclick={() => openPreview(it)}
						>
							<span class="ptype {it.kind}">{kindLabel(it.kind)}</span>
							<span class="ptitle">{displayName(it)}</span>
							{#if it.tags.length}
								<span class="ptags">{it.tags.map((tg) => '#' + tg).join(' ')}</span>
							{/if}
						</button>
						<div class="row-actions">
							<button
								class="row-act"
								onclick={() => openPreview(it)}
								title={t('palette.preview', $locale)}
								aria-label={t('palette.preview', $locale)}
							>
								<svg
									width="13"
									height="13"
									viewBox="0 0 16 16"
									fill="none"
									stroke="currentColor"
									stroke-width="1.3"
									stroke-linecap="round"
									stroke-linejoin="round"
									aria-hidden="true"
								>
									<path d="M1.5 8S4 3.5 8 3.5 14.5 8 14.5 8 12 12.5 8 12.5 1.5 8 1.5 8Z" />
									<circle cx="8" cy="8" r="1.7" />
								</svg>
							</button>
							<button
								class="row-act"
								onclick={() => windowItem(it)}
								title={t('palette.openWindow', $locale)}
								aria-label={t('palette.openWindow', $locale)}
							>
								<svg
									width="13"
									height="13"
									viewBox="0 0 16 16"
									fill="none"
									stroke="currentColor"
									stroke-width="1.3"
									stroke-linecap="round"
									stroke-linejoin="round"
									aria-hidden="true"
								>
									<path d="M6 3H3.3a.8.8 0 0 0-.8.8v8.4a.8.8 0 0 0 .8.8h8.4a.8.8 0 0 0 .8-.8V10" />
									<path d="M9 2.5h4.5V7" />
									<path d="M13.5 2.5 7.2 8.8" />
								</svg>
							</button>
							<button
								class="row-act"
								onclick={() => goItem(it)}
								title={t('palette.goPage', $locale)}
								aria-label={t('palette.goPage', $locale)}
							>
								<svg
									width="13"
									height="13"
									viewBox="0 0 16 16"
									fill="none"
									stroke="currentColor"
									stroke-width="1.4"
									stroke-linecap="round"
									stroke-linejoin="round"
									aria-hidden="true"
								>
									<path d="M2.5 8h11" />
									<path d="M9 4.2 12.8 8 9 11.8" />
								</svg>
							</button>
						</div>
					</div>
				{/each}
			{/if}
		</div>
		<!-- BUG-157: native scrollbar 대신 overlay — 목록 폭을 밀지 않는다. -->
		{#if rowsEl}
			<OverlayScrollbar target={rowsEl} />
		{/if}
	{:else}
		<div class="dp-head">
			<span class="ptype {preview.kind}">{kindLabel(preview.kind)}</span>
			<span class="dp-title" use:titlePopup={displayName(preview)}>{displayName(preview)}</span>
			<button
				class="dp-x"
				onclick={() => (preview = null)}
				title={t('palette.backToListTitle', $locale)}>✕</button
			>
		</div>
		<div class="dp-meta">
			<span>{metaOf(preview)}</span>
			{#if preview.tags.length}
				<span class="tag">{preview.tags.map((tg) => '#' + tg).join(' ')}</span>
			{/if}
		</div>
		<div class="dp-body" bind:this={previewBodyEl} style="height:{previewH}px">
			{#if previewLoading}
				<div class="empty">{t('palette.loading', $locale)}</div>
			{:else}
				<MarkdownView source={previewBody} />
			{/if}
		</div>
		{#if previewBodyEl}
			<OverlayScrollbar target={previewBodyEl} />
		{/if}
		<div class="dp-foot">
			<button class="dp-btn" onclick={() => (preview = null)}
				>{t('palette.backToList', $locale)}</button
			>
			<!-- DEV-255: 미리보기에서도 자식윈도우/페이지이동으로 전환 가능. -->
			<button class="dp-btn" onclick={() => preview && windowItem(preview)}
				>{t('palette.openWindow', $locale)}</button
			>
			<button class="dp-btn primary" onclick={() => preview && goItem(preview)}
				>{t('palette.goPageArrow', $locale)}</button
			>
		</div>
		<!-- svelte-ignore a11y_no_static_element_interactions -->
		<div
			class="dp-resize"
			onmousedown={startResize}
			title={t('palette.resizeHandle', $locale)}
		></div>
	{/if}
</div>

<style>
	.backdrop {
		position: fixed;
		inset: var(--titlebar-h, 0px) 0 0 0;
		z-index: 1190;
		background: transparent;
		border: none;
		cursor: default;
	}
	.palette {
		position: fixed;
		top: calc(var(--titlebar-h, 32px) + 2px);
		left: 50%;
		transform: translateX(-50%);
		width: min(560px, 62vw);
		z-index: 1200;
		background: var(--bg-elevated);
		border: 1px solid var(--border);
		border-radius: 8px;
		box-shadow: 0 10px 34px rgba(0, 0, 0, 0.45);
		overflow: hidden;
	}
	/* DEV-257: 모바일 폭에선 62vw 가 너무 좁음(375px 기준 232px) — 거의 전폭. */
	@media (max-width: 640px) {
		.palette {
			width: calc(100vw - 16px);
		}
	}
	.input-wrap {
		display: flex;
		align-items: center;
		gap: 0.5rem;
		padding: 0 0.8rem;
		background: var(--bg-subtle);
		border-bottom: 1px solid var(--border);
	}
	.scope-chip {
		flex: none;
		font-size: 0.68rem;
		font-weight: 600;
		border-radius: 4px;
		padding: 0.1rem 0.4rem;
		color: var(--accent);
		background: color-mix(in srgb, var(--accent) 14%, transparent);
	}
	.scope-chip.campaign {
		color: var(--hl-pre);
		background: color-mix(in srgb, var(--hl-pre) 14%, transparent);
	}
	.scope-chip.rule {
		color: var(--success);
		background: color-mix(in srgb, var(--success) 14%, transparent);
	}
	.scope-chip.book {
		color: var(--warning);
		background: color-mix(in srgb, var(--warning) 14%, transparent);
	}
	input {
		flex: 1;
		width: 100%;
		padding: 0.55rem 0;
		font-size: 0.9rem;
		border: none;
		outline: none;
		background: transparent;
		color: var(--text-strong);
	}
	input::placeholder {
		color: var(--text-faint);
	}
	.rows {
		max-height: 340px;
		overflow-y: auto;
		/* BUG-157: native scrollbar 숨김 — OverlayScrollbar 가 대신 그린다
		   (QuestCombobox 등과 동일 규칙). */
		scrollbar-width: none;
	}
	.rows::-webkit-scrollbar {
		display: none;
	}
	.empty {
		padding: 0.9rem;
		text-align: center;
		font-size: 0.82rem;
		color: var(--text-faint);
	}
	/* DEV-255: 행 = row-main(라벨, 기본 클릭 = 미리보기) + row-actions(열기 방식
	   아이콘 3개). 이전엔 행 전체가 하나의 <button> 이었으나 중첩 버튼이
	   필요해져 컨테이너를 div 로 변경. */
	.row {
		display: flex;
		align-items: stretch;
		width: 100%;
	}
	.row.sel {
		background: var(--nav-hover-bg);
	}
	.row-main {
		display: flex;
		/* DEV-359: `center` 로 두면 제목이 1줄일 때와 2줄일 때 상자 높이가 4px
		   달라져(정렬 기준선 차이) 선택이 옮겨갈 때마다 종류 칩과 우측 버튼이
		   미세하게 움직였다. 위 기준으로 붙여 어느 쪽이든 높이가 같게 한다. */
		align-items: flex-start;
		gap: 0.6rem;
		flex: 1;
		min-width: 0;
		padding: 0.45rem 0.8rem;
		font-size: 0.85rem;
		background: transparent;
		border: none;
		cursor: pointer;
		text-align: left;
	}
	.row-actions {
		flex: none;
		display: flex;
		align-items: center;
		gap: 0.1rem;
		padding: 0 0.5rem 0 0;
	}
	.row-act {
		display: inline-flex;
		align-items: center;
		justify-content: center;
		width: 22px;
		height: 22px;
		color: var(--text-faint);
		background: transparent;
		border: none;
		border-radius: 4px;
		cursor: pointer;
	}
	.row-act:hover {
		background: var(--nav-hover-bg);
		color: var(--text);
	}
	.ptype {
		flex: none;
		min-width: 3.6rem;
		text-align: center;
		font-size: 0.68rem;
		font-weight: 600;
		border-radius: 4px;
		padding: 0.1rem 0.35rem;
	}
	/* 타입별 색 — QuestBoard / 문서 톤과 맞춤. */
	.ptype.quest {
		color: var(--accent);
		background: color-mix(in srgb, var(--accent) 14%, transparent);
	}
	.ptype.campaign {
		color: var(--hl-pre);
		background: color-mix(in srgb, var(--hl-pre) 14%, transparent);
	}
	.ptype.rule {
		color: var(--success);
		background: color-mix(in srgb, var(--success) 14%, transparent);
	}
	.ptype.book {
		color: var(--warning);
		background: color-mix(in srgb, var(--warning) 14%, transparent);
	}
	.ptitle {
		flex: 1;
		color: var(--text);
		/* DEV-359: 제목 자리를 **항상 2줄** 확보한다. 선택된 행만 clamp 를 1줄에서
		   2줄로 풀어 전체 제목을 보여주므로, 펼쳐도 행 높이가 변하지 않는다 —
		   목록이 미동도 하지 않아 스크롤 보정도, 이웃 항목이 떠는 일도, 휠과
		   싸우는 일도 없다. 대가는 한 화면에 보이는 항목 수(행 36px → 58px). */
		display: -webkit-box;
		-webkit-box-orient: vertical;
		-webkit-line-clamp: 1;
		line-clamp: 1;
		min-height: calc(2 * 1.45em);
		overflow: hidden;
		overflow-wrap: anywhere;
	}
	/* DEV-297 수정: 키보드로 선택된 행만 말줄임을 풀어 제자리에서 펼친다.
	   행 높이는 내용만큼만 늘고, 선택이 옮겨가면 다시 한 줄. 우측 액션 버튼이
	   따라 내려가지 않도록 정렬만 위로 붙인다. */
	/* DEV-359 후속(반응 속도): 결과가 600행쯤 되면 **매 프레임 목록 전체가 다시
	   레이아웃된다** — 펼침 높이 애니메이션이나 스크롤 보정처럼 프레임마다 도는
	   작업에서 그 비용이 그대로 체감된다(실측: 강제 레이아웃 10회 13.3ms).
	   화면 밖 행은 레이아웃을 건너뛰게 하면 같은 측정이 0.2ms 로 떨어진다.
	   접힌 행은 높이가 일정해서 예상 크기를 정확히 줄 수 있고, 펼쳐지는 행은
	   언제나 화면 안이라 영향이 없다. 미지원 엔진에서는 그냥 무시된다. */
	.row {
		/* 화면 밖 행은 레이아웃을 건너뛴다 — 600행 기준 강제 레이아웃 13.3ms → 2.9ms.
		   모든 행 높이가 같으므로 예상 크기가 정확하다. */
		content-visibility: auto;
		/* 실제 행 높이(약 54px)와 예상 크기가 어긋나면 행이 화면에 들어올 때마다
		   추정이 실측으로 교체되며 스크롤이 튄다. `auto` 는 마지막 렌더 크기를
		   기억해 스스로 맞춰간다 — 모든 행 높이가 같아졌으므로 이제 안전하다
		   (예전엔 펼친 행만 커서 `auto` 가 오히려 흔들림의 원인이었다). */
		contain-intrinsic-size: auto 54px;
	}
	/* DEV-297/359: 선택된 행만 말줄임을 풀어 2줄 전체를 보여준다. 자리는 이미
	   잡혀 있으므로 높이는 그대로 — 아무것도 움직이지 않는다. */
	.row.expanded .ptitle {
		-webkit-line-clamp: 2;
		line-clamp: 2;
	}
	.ptags {
		flex: none;
		font-size: 0.7rem;
		color: var(--text-faint);
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
		max-width: 40%;
	}
	/* ── 미리보기 ── */
	.dp-head {
		display: flex;
		align-items: center;
		gap: 0.6rem;
		padding: 0.55rem 0.8rem;
		border-bottom: 1px solid var(--border);
	}
	.dp-title {
		flex: 1;
		font-size: 0.9rem;
		font-weight: 600;
		color: var(--text-strong);
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.dp-x {
		flex: none;
		color: var(--text-muted);
		font-size: 0.8rem;
		background: none;
		border: none;
		cursor: pointer;
		padding: 0.2rem 0.35rem;
		border-radius: 4px;
	}
	.dp-x:hover {
		background: var(--nav-hover-bg);
		color: var(--text);
	}
	.dp-meta {
		display: flex;
		gap: 0.9rem;
		padding: 0.4rem 0.8rem;
		font-size: 0.72rem;
		color: var(--text-muted);
		border-bottom: 1px solid var(--border);
	}
	.dp-meta .tag {
		color: var(--accent);
	}
	.dp-body {
		padding: 0.5rem 0.8rem;
		overflow-y: auto;
		/* BUG-157: 예전 주석은 "global.css 커스텀 스크롤바로 충분"이라 했지만,
		   그건 native scrollbar 를 얇게 칠한 것일 뿐 컨텐츠 폭을 차지한다.
		   다른 스크롤 영역과 같이 OverlayScrollbar 로 통일. */
		scrollbar-width: none;
	}
	.dp-body::-webkit-scrollbar {
		display: none;
	}
	.dp-foot {
		display: flex;
		justify-content: flex-end;
		gap: 0.5rem;
		padding: 0.5rem 0.8rem;
		border-top: 1px solid var(--border);
	}
	.dp-btn {
		font-size: 0.78rem;
		padding: 0.3rem 0.7rem;
		border-radius: 6px;
		border: 1px solid var(--border);
		background: transparent;
		color: var(--text);
		cursor: pointer;
	}
	.dp-btn:hover {
		background: var(--nav-hover-bg);
	}
	.dp-btn.primary {
		background: var(--btn-primary-bg);
		border-color: transparent;
		color: var(--btn-primary-text);
	}
	/* 아래 가장자리 = 세로 크기 조절 핸들. */
	.dp-resize {
		height: 7px;
		cursor: ns-resize;
		background: var(--bg-subtle);
		border-top: 1px solid var(--border);
		display: flex;
		align-items: center;
		justify-content: center;
	}
	.dp-resize::before {
		content: '';
		width: 34px;
		height: 3px;
		border-radius: 2px;
		background: var(--text-faint);
	}
	.dp-resize:hover::before {
		background: var(--text-muted);
	}
</style>
