<!--
  DEV-256: 크로스링크 호버 미리보기 팝업.

  본문에 렌더된 크로스링크(`[[DEV-033]]` 류, MarkdownView 의 a.xlink)에
  마우스를 올리면 검색 팔레트(DEV-255)의 미리보기와 같은 구성(종류 칩 +
  제목 헤더 + MarkdownView 본문 + 새창/페이지이동 버튼)의 팝업을 앵커
  근처에 띄운다. 호버 intent/닫힘 타이머는 호출부(MarkdownView)가 관리 —
  이 컴포넌트는 "떠 있는 동안"의 렌더/위치/자체 hover 유지만 담당.

  순환 참조 주의: 이 컴포넌트는 MarkdownView 를 정적 import 하고(본문
  렌더), MarkdownView 는 이 컴포넌트를 **동적 import** 로만 연다 — 정적
  순환 의존을 만들지 않기 위한 의도적 비대칭. 팝업 안 본문의 크로스링크에
  다시 호버하면 중첩 팝업도 자연히 동작한다(위키피디아 프리뷰 식).
-->
<script lang="ts">
	import MarkdownView from './MarkdownView.svelte';
	import { locale, t } from '$lib/stores/locale';
	import { openInWindow, openInPage } from '$lib/utils/open-item';
	import type { Kind } from '$lib/stores/questIndex';
	import { questsApi } from '$lib/api/quests';
	import { campaignsApi } from '$lib/api/campaigns';
	import { rulesApi } from '$lib/api/rules';
	import { libraryApi } from '$lib/api/library';

	let {
		kind,
		id,
		slug,
		title,
		href,
		anchorRect,
		onenter,
		onleave,
		onnavigate
	}: {
		kind: Kind;
		/** quest/campaign/book 은 대문자 정규 ID, rule 은 slug 와 동일. */
		id: string;
		/** rule 전용 — 원본 대소문자 slug (API 조회에 필요). */
		slug?: string;
		title: string;
		href: string;
		/** 앵커(크로스링크 요소)의 화면 좌표 — 위치 계산용. */
		anchorRect: { left: number; right: number; top: number; bottom: number };
		/** 팝업 위로 마우스가 들어옴/나감 — 호출부의 닫힘 타이머 제어. */
		onenter: () => void;
		onleave: () => void;
		/** 페이지 이동 등으로 팝업을 정리해야 할 때. */
		onnavigate: () => void;
	} = $props();

	let body = $state('');
	let loading = $state(true);

	function kindLabel(k: Kind): string {
		return t(`kind.${k}`, $locale);
	}

	// 표시 이름 — 검색 팔레트의 displayName 과 동일 규칙(규칙은 slug 만).
	const displayName = $derived(kind === 'rule' ? (slug ?? id) : title ? `${id} ${title}` : id);

	async function load(): Promise<string> {
		switch (kind) {
			case 'quest':
				return (await questsApi.getBySlug(id)).description ?? '';
			case 'campaign':
				return (await campaignsApi.get(id)).description ?? '';
			case 'rule':
				return (await rulesApi.get(slug ?? id)).content ?? '';
			case 'book':
				return (await libraryApi.get(id)).body ?? '';
		}
	}

	$effect(() => {
		// kind/id 가 바뀌면(팝업 재사용) 다시 로드.
		void kind;
		void id;
		loading = true;
		body = '';
		load()
			.then((b) => (body = b.trim() ? b : t('palette.emptyBody', $locale)))
			.catch(() => (body = t('palette.previewLoadFail', $locale)))
			.finally(() => (loading = false));
	});

	// 위치: 앵커 아래(기본) — 화면 하부(60% 이하)면 위로. 가로는 clamp.
	const WIDTH = 400;
	const pos = $derived.by(() => {
		const vw = typeof window === 'undefined' ? 1280 : window.innerWidth;
		const vh = typeof window === 'undefined' ? 800 : window.innerHeight;
		const left = Math.max(8, Math.min(anchorRect.left, vw - WIDTH - 8));
		const below = anchorRect.bottom <= vh * 0.6;
		return below
			? { left, top: anchorRect.bottom + 6, bottom: null as number | null }
			: { left, top: null as number | null, bottom: vh - anchorRect.top + 6 };
	});

	function goPage() {
		onnavigate();
		openInPage(href);
	}
	function goWindow() {
		void openInWindow(href, displayName);
	}
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div
	class="lp"
	style:left="{pos.left}px"
	style:top={pos.top !== null ? `${pos.top}px` : undefined}
	style:bottom={pos.bottom !== null ? `${pos.bottom}px` : undefined}
	onmouseenter={onenter}
	onmouseleave={onleave}
	role="tooltip"
>
	<div class="lp-head">
		<span class="lp-kind {kind}">{kindLabel(kind)}</span>
		<span class="lp-title" title={displayName}>{displayName}</span>
	</div>
	<div class="lp-body">
		{#if loading}
			<div class="lp-loading">{t('palette.loading', $locale)}</div>
		{:else}
			<MarkdownView source={body} />
		{/if}
	</div>
	<div class="lp-foot">
		<button class="lp-btn" onclick={goWindow}>{t('palette.openWindow', $locale)}</button>
		<button class="lp-btn primary" onclick={goPage}>{t('palette.goPageArrow', $locale)}</button>
	</div>
</div>

<style>
	.lp {
		position: fixed;
		width: min(400px, 90vw);
		z-index: 1300; /* 검색 팔레트(1200)보다 위 — 팔레트 미리보기 안에서도 뜸. */
		background: var(--bg-elevated);
		border: 1px solid var(--border);
		border-radius: 8px;
		box-shadow: 0 8px 28px rgba(0, 0, 0, 0.4);
		overflow: hidden;
		display: flex;
		flex-direction: column;
	}
	.lp-head {
		display: flex;
		align-items: center;
		gap: 0.5rem;
		padding: 0.45rem 0.7rem;
		border-bottom: 1px solid var(--border);
	}
	.lp-kind {
		flex: none;
		font-size: 0.66rem;
		font-weight: 600;
		border-radius: 4px;
		padding: 0.08rem 0.34rem;
		color: var(--accent);
		background: color-mix(in srgb, var(--accent) 14%, transparent);
	}
	/* 종류별 색 — SearchPalette .ptype 과 동일 톤. */
	.lp-kind.campaign {
		color: var(--hl-pre);
		background: color-mix(in srgb, var(--hl-pre) 14%, transparent);
	}
	.lp-kind.rule {
		color: var(--success);
		background: color-mix(in srgb, var(--success) 14%, transparent);
	}
	.lp-kind.book {
		color: var(--warning);
		background: color-mix(in srgb, var(--warning) 14%, transparent);
	}
	.lp-title {
		flex: 1;
		font-size: 0.82rem;
		font-weight: 600;
		color: var(--text-strong);
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.lp-body {
		max-height: 260px;
		overflow-y: auto;
		padding: 0.4rem 0.6rem;
	}
	.lp-loading {
		padding: 0.7rem;
		text-align: center;
		font-size: 0.78rem;
		color: var(--text-faint);
	}
	.lp-foot {
		display: flex;
		justify-content: flex-end;
		gap: 0.4rem;
		padding: 0.4rem 0.6rem;
		border-top: 1px solid var(--border);
	}
	.lp-btn {
		font-size: 0.72rem;
		padding: 0.24rem 0.55rem;
		border-radius: 5px;
		border: 1px solid var(--border);
		background: transparent;
		color: var(--text);
		cursor: pointer;
	}
	.lp-btn:hover {
		background: var(--nav-hover-bg);
	}
	.lp-btn.primary {
		background: var(--btn-primary-bg);
		border-color: transparent;
		color: var(--btn-primary-text);
	}
</style>
