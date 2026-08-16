<!--
  DEV-172: cross-link 자동완성 팝업 UI — DEV-171(댓글 textarea)에서 만든 걸
  본문 편집기(CodeMirror, MarkdownEditor.svelte)와 공유하기 위해 추출.

  이 컴포넌트는 **렌더링만** 한다 — 후보 매칭(wikiMatch), caret 위치 계산,
  키보드 네비게이션, 적용(insert)은 전부 호출측 책임. 그래야 textarea 기반
  (comments) 과 CodeMirror 기반(MarkdownEditor) 이 서로 다른 캐럿/삽입 API 를
  쓰면서도 같은 팝업을 그릴 수 있다.
-->
<script lang="ts">
	import Icon from './Icon.svelte';
	import OverlayScrollbar from './OverlayScrollbar.svelte';
	import { isPointerDrivenHover } from '$lib/utils/hover-guard';
	import { locale, t } from '$lib/stores/locale';
	import type { WikiItem } from '$lib/utils/textarea-wikilink';

	let {
		items,
		left,
		top = null,
		bottom = null,
		maxH = 224,
		selectedIndex,
		onSelect,
		onHoverSelect,
		popupEl = $bindable<HTMLUListElement | undefined>(undefined)
	}: {
		items: WikiItem[];
		/** viewport 기준 px. */
		left: number;
		/** top 배치일 때 px, 아니면 null(=bottom 배치 중). */
		top?: number | null;
		/** bottom 배치일 때 px, 아니면 null(=top 배치 중). */
		bottom?: number | null;
		maxH?: number;
		selectedIndex: number;
		onSelect: (item: WikiItem, index: number) => void;
		onHoverSelect: (index: number) => void;
		popupEl?: HTMLUListElement;
	} = $props();
</script>

<ul
	class="wiki-pop"
	bind:this={popupEl}
	style="left:{left}px; {bottom != null
		? `bottom:${bottom}px`
		: `top:${top ?? 0}px`}; max-height:{maxH}px"
>
	{#each items as it, i (it.id)}
		<li>
			<button
				type="button"
				class="wiki-opt"
				class:sel={i === selectedIndex}
				class:expanded={i === selectedIndex}
				onmousedown={(ev) => {
					ev.preventDefault();
					onSelect(it, i);
				}}
				onmouseenter={(ev) => {
					// DEV-359: 굴리는 중에 항목이 커서 밑을 지나가며 들어오는 hover
					// (커서는 가만히 있다)는 무시한다.
					if (!isPointerDrivenHover(ev)) return;
					onHoverSelect(i);
				}}
			>
				<!-- BUG-169: 🏷️/🔗 는 컬러 이모지로 렌더돼 OS 마다 크기·기준선이
				     달랐다 — currentColor SVG 로 교체. -->
				<span class="wiki-id" class:missing={!it.exists}>
					<Icon name={it.nsPrefix ? 'tag' : 'link'} size={12} />
					{it.insert ?? it.id}</span
				>
				<span class="wiki-meta">
					{it.nsPrefix
						? it.title
						: it.exists
							? `${it.kind === 'rule' ? t('comment.ruleLinkPrefix', $locale) : it.kind === 'book' ? t('comment.bookLinkPrefix', $locale) : ''}${it.title}`
							: t('comment.newLink', $locale)}
				</span>
			</button>
		</li>
	{/each}
</ul>
<!-- BUG-157: 팝업 native 스크롤바 대신 overlay thumb. -->
<OverlayScrollbar target={popupEl ?? null} />

<style>
	.wiki-pop {
		position: fixed;
		/* DEV-344 후속: 원래 z-index:50 이었는데, 이 컴포넌트가 쓰이는 곳 중
		   NewQuestModal(퀘스트 생성 팝업)은 자체 오버레이가 z-index:200 이라
		   그 뒤에 가려 안 보였다(DOM 에는 있고 동작도 하는데 시각적으로만
		   숨음 — Playwright 의 isVisible() 는 elementFromPoint 겹침을 안 봐서
		   그때는 놓쳤다). 이 팝업이 쓰이는 모든 곳(댓글/본문/생성 모달)의
		   오버레이보다 확실히 위에 오도록 상향.*/
		z-index: 250;
		margin: 0;
		padding: 0.2rem;
		list-style: none;
		/* 모바일 수정: 예전엔 min 14rem / max 22rem 고정이라 375px 화면에서 팝업이
		   화면을 거의 덮고 오른쪽으로 넘쳐 나갔다. 뷰포트를 넘지 않도록 상한을
		   함께 건다(양옆 8px 여백). min-width 도 같은 이유로 뷰포트에 양보. */
		min-width: min(14rem, calc(100vw - 16px));
		max-width: min(22rem, calc(100vw - 16px));
		/* 짧은 화면(가로 모드 등)에서 팝업이 화면 높이를 넘지 않게. */
		max-height: min(14rem, 45vh);
		overflow-y: auto;
		/* BUG-157: native scrollbar 숨김 — OverlayScrollbar 가 대신 그린다. */
		scrollbar-width: none;
		background: var(--bg-elevated);
		border: 1px solid var(--border);
		border-radius: 8px;
		box-shadow: 0 6px 20px rgba(0, 0, 0, 0.35);
	}
	.wiki-pop::-webkit-scrollbar {
		display: none;
	}
	/* DEV-359 후속(반응 속도): 후보가 수백 개면 프레임마다 목록 전체가 다시
	   레이아웃된다 — 화면 밖 항목은 건너뛰게 한다(팔레트와 같은 이유). */
	.wiki-pop li {
		content-visibility: auto;
		contain-intrinsic-size: 46px;
	}
	.wiki-opt {
		display: flex;
		align-items: baseline;
		gap: 0.5rem;
		width: 100%;
		padding: 0.3rem 0.5rem;
		border: none;
		border-radius: 5px;
		background: transparent;
		color: var(--text);
		cursor: pointer;
		text-align: left;
	}
	.wiki-opt.sel,
	.wiki-opt:hover {
		background: color-mix(in srgb, var(--accent) 18%, transparent);
	}
	.wiki-id {
		flex: none;
		font-family: 'SFMono-Regular', Consolas, monospace;
		font-size: 0.8rem;
		color: var(--accent);
	}
	.wiki-id.missing {
		color: var(--danger);
	}
	/* DEV-297/359: 선택된 항목은 말줄임을 풀어 전체를 보여준다. 자리는 **항상
	   2줄** 확보해 두고 선택된 것만 clamp 를 푸는 방식이라 높이가 변하지 않는다 —
	   목록이 미동도 하지 않으므로 스크롤 보정도, 이웃 항목이 떠는 일도 없다.
	   (팝업이 위/아래 항목을 가리던 예전 방식으로 돌아가지 않으면서도 그렇다.) */
	.wiki-meta {
		flex: 1;
		min-width: 0;
		font-size: 0.78rem;
		color: var(--text-muted);
		overflow: hidden;
		display: -webkit-box;
		-webkit-box-orient: vertical;
		-webkit-line-clamp: 1;
		line-clamp: 1;
		min-height: calc(2 * 1.4em);
		overflow-wrap: anywhere;
	}
	.wiki-opt.expanded .wiki-meta {
		-webkit-line-clamp: 2;
		line-clamp: 2;
	}
	.wiki-opt.expanded .wiki-id {
		white-space: normal;
		overflow-wrap: anywhere;
	}
</style>
