<!--
  BUG-021 fix1: Markdown 본문 렌더링 공유 컴포넌트.

  Quest Detail 과 Campaign Detail 의 본문 프리뷰가 같은 스타일을 쓰도록
  통일. 캠페인 스타일 base (헤더 사이즈 = 브라우저 기본 = 명확히 구분,
  배경 var(--bg)) + Quest 의 확장 (pre / blockquote / table / hr / a) 머지.
-->
<script lang="ts">
	import { Marked } from 'marked';
	// DEV-183: 코드펜스 syntax highlighting — highlight.js (로컬 번들, 외부 통신 없음).
	import { markedHighlight } from 'marked-highlight';
	import hljs from 'highlight.js';
	import { tick } from 'svelte';
	// DEV-111: mermaid 다이어그램 렌더링 — lazy import (~700KB), 블록 있을 때만.
	// BUG-121: `theme`(ThemeChoice) 대신 `effectiveTheme` 구독 — system 모드에서
	// OS 가 테마를 바꿔도 `theme` 값 자체는 'system' 그대로라 재통지가 안 됨.
	import { effectiveTheme } from '$lib/stores/theme';
	// DEV-140: 본문 cross-link — [[DEV-033]] / [[C-001]] 위키문법을 링크로.
	// DEV-219: [[kind:ID]] 명시 네임스페이스 지원 — resolveCrossLinkToken 이 접두
	// 유무와 무관하게 kind/ref 를 함께 풀어준다.
	import {
		questIndex,
		loadQuestIndex,
		refHref,
		resolveCrossLinkToken,
		// BUG-173: missing 발견 시 쿨다운 1회 재적재(자가 치유).
		refreshIndexForMissing,
		KIND_LABEL,
		type Kind
	} from '$lib/stores/questIndex';

	let { source }: { source: string } = $props();

	// DEV-256: 크로스링크 호버 미리보기 — 실재하는 링크(a.xlink, missing 제외)에
	// 마우스를 잠시(280ms) 올리면 검색 팔레트식 미리보기 팝업을 앵커 근처에
	// 띄운다. 링크/팝업 둘 다에서 벗어나면 지연(300ms) 후 닫힘 — 링크→팝업으로
	// 마우스를 옮기는 동안 닫히지 않게. 팝업 컴포넌트는 동적 import — 정적
	// 순환 의존(LinkPreviewPopup ↔ MarkdownView) 회피 + 첫 호버 전 비용 0.
	type HoverTarget = {
		kind: Kind;
		id: string;
		slug?: string;
		title: string;
		href: string;
		anchorRect: { left: number; right: number; top: number; bottom: number };
	};
	let PopupComp = $state<typeof import('./LinkPreviewPopup.svelte').default | null>(null);
	let hoverTarget = $state<HoverTarget | null>(null);
	let openTimer: ReturnType<typeof setTimeout> | null = null;
	let closeTimer: ReturnType<typeof setTimeout> | null = null;

	function cancelClose() {
		if (closeTimer) {
			clearTimeout(closeTimer);
			closeTimer = null;
		}
	}
	function scheduleClose() {
		cancelClose();
		closeTimer = setTimeout(() => (hoverTarget = null), 300);
	}
	function onContainerOver(e: MouseEvent) {
		const a = (e.target as HTMLElement).closest?.('a.xlink') as HTMLAnchorElement | null;
		if (!a || !container?.contains(a) || !a.dataset.xkind) return;
		cancelClose();
		if (openTimer) clearTimeout(openTimer);
		openTimer = setTimeout(async () => {
			openTimer = null;
			if (!PopupComp) PopupComp = (await import('./LinkPreviewPopup.svelte')).default;
			const r = a.getBoundingClientRect();
			hoverTarget = {
				kind: a.dataset.xkind as Kind,
				id: a.dataset.xid ?? '',
				slug: a.dataset.xslug,
				title: a.dataset.xtitle ?? '',
				href: a.getAttribute('href') ?? '#',
				anchorRect: { left: r.left, right: r.right, top: r.top, bottom: r.bottom }
			};
		}, 280);
	}
	function onContainerOut(e: MouseEvent) {
		const a = (e.target as HTMLElement).closest?.('a.xlink');
		if (!a) return;
		if (openTimer) {
			clearTimeout(openTimer);
			openTimer = null;
		}
		if (hoverTarget) scheduleClose();
	}
	// BUG: 팝업이 뜬 채로 팝업의 버튼이 아니라 본문 a.xlink 자체를 클릭해
	// 이동하면(SvelteKit 기본 anchor 인터셉트), onnavigate 경로를 안 타서
	// hoverTarget 이 안 지워지고 팝업이 새 페이지까지 남아있었다. 클릭도
	// 위임으로 잡아 즉시 정리.
	function onContainerClick(e: MouseEvent) {
		const a = (e.target as HTMLElement).closest?.('a.xlink') as HTMLAnchorElement | null;
		if (!a || !container?.contains(a) || !a.dataset.xkind) return;
		if (openTimer) {
			clearTimeout(openTimer);
			openTimer = null;
		}
		cancelClose();
		hoverTarget = null;
	}
	$effect(() => () => {
		// 언마운트 시 타이머 정리.
		if (openTimer) clearTimeout(openTimer);
		if (closeTimer) clearTimeout(closeTimer);
	});

	// DEV-183: highlight.js 연동 marked 인스턴스. language-mermaid 는 mermaid 가
	// 따로 렌더 — plaintext 로 하이라이트돼도 innerText 가 보존돼 영향 없음.
	const md = new Marked(
		markedHighlight({
			emptyLangClass: 'hljs',
			langPrefix: 'hljs language-',
			highlight(code, lang) {
				const language = lang && hljs.getLanguage(lang) ? lang : 'plaintext';
				return hljs.highlight(code, { language }).value;
			}
		})
	);
	let html = $derived(md.parse(source ?? '', { async: false }) as string);
	let container: HTMLDivElement | undefined = $state(undefined);

	// 매 effect 실행 시 mermaid 블록 탐지 + 렌더. id 충돌 방지용 counter.
	let renderCounter = 0;

	// DEV-111 fix1: mermaid.render() 는 임시 컨테이너를 body 에 만들어 SVG 를
	// 그리는데, syntax error 시 그 임시 div 에 bomb 아이콘 + "Syntax error in
	// text mermaid version X.Y.Z" 를 그려놓고 throw 한다. 우리 catch 는 throw
	// 만 잡고 댓글 안의 <pre> 만 교체하므로 body 끝의 leftover 가 남아
	// 페이지 최하단에 폭탄이 계속 보임. SPA 라우트 전환에도 사라지지 않아
	// markdown preview 없는 페이지에서도 보였음.
	//
	// 수정:
	//   (A) mermaid.parse(code, { suppressErrors: true }) 로 render 전 검증.
	//       false 면 render 자체를 부르지 않음 — DOM 오염 X.
	//   (B) 안전망: render 실패 시 mermaid 가 만들었을 수 있는 임시 노드를
	//       id 기준으로 직접 제거.
	async function renderMermaidBlocks() {
		if (!container) return;
		const blocks = container.querySelectorAll<HTMLElement>('pre > code.language-mermaid');
		if (blocks.length === 0) return;
		const { default: mermaid } = await import('mermaid');
		const eff = $effectiveTheme;
		mermaid.initialize({
			startOnLoad: false,
			theme: eff === 'dark' ? 'dark' : 'default',
			securityLevel: 'loose',
			fontFamily: 'inherit'
		});
		for (const block of Array.from(blocks)) {
			const pre = block.parentElement;
			if (!pre) continue;
			const code = block.innerText;
			const id = `mm-${++renderCounter}-${Math.random().toString(36).slice(2, 8)}`;
			// (A) parse pre-check — syntax error 시 render 안 부름.
			let parseOk = true;
			let parseErr: unknown = null;
			try {
				const res = await mermaid.parse(code, { suppressErrors: true });
				parseOk = res !== false;
			} catch (e) {
				// suppressErrors:true 면 throw 안 하지만 방어.
				parseOk = false;
				parseErr = e;
			}
			if (!parseOk) {
				showInlineError(pre, parseErr ?? new Error('syntax error'));
				continue;
			}
			try {
				const { svg } = await mermaid.render(id, code);
				const wrap = document.createElement('div');
				wrap.className = 'mermaid-rendered';
				wrap.innerHTML = svg;
				pre.replaceWith(wrap);
			} catch (e) {
				showInlineError(pre, e);
				// (B) 혹시 모를 leftover 정리. mermaid 가 만드는 임시 컨테이너 후보:
				//   - `<svg id="${id}">` (render 호출 시 전달한 id)
				//   - `<div id="d${id}">` (mermaid v11 의 hidden 임시 div)
				document.getElementById(id)?.remove();
				document.getElementById(`d${id}`)?.remove();
			}
		}
	}

	function showInlineError(pre: HTMLElement, e: unknown) {
		const err = document.createElement('div');
		err.className = 'mermaid-error';
		err.textContent = `mermaid 렌더 실패: ${(e as Error)?.message ?? String(e)}`;
		pre.replaceWith(err);
	}

	// DEV-140: cross-link 판정용 인덱스 — 최초 1회 적재 (실패 무해).
	loadQuestIndex();

	$effect(() => {
		// html / theme / 인덱스 변경 시 재렌더.
		void html;
		void $effectiveTheme;
		void $questIndex;
		tick().then(() => {
			renderMermaidBlocks();
			rewriteLocalMedia();
			rewriteCrossLinks();
		});
	});

	// DEV-140: `[[DEV-033]]` / `[[C-001]]` / `[[규칙slug]]` → 내부 링크.
	// 실재=파랑 / 미존재=빨강.
	//
	// marked 는 `[[...]]` 를 링크로 해석하지 않아 본문에 리터럴 텍스트로 남음.
	// 렌더 후 텍스트 노드만 훑어 토큰을 anchor 로 치환 (code/pre/기존 a 안은 제외).
	// DEV-220(사용자 결정): **명시 `[[..]]` 만 인식** — bare `DEV-033`(대괄호 없음)
	// 자동 링크는 제거(의도치 않은 링크화가 불편). slug 는 한글 등 비ASCII 허용
	// (공백/대괄호 제외 — DEV-173).
	// DEV-219: 접두 `kind:` (quest/q, campaign/c, rules/rule/r, library/lib/book)
	// 를 허용 — 나머지 문자 클래스는 기존과 동일(공백/대괄호 제외).
	// BUG-156: 규칙 slug 는 파일명이라 **공백을 포함할 수 있다**(예: `[[코딩 규칙]]`).
	// 예전 문자 클래스가 `\s` 를 통째로 배제해 띄어쓰기 있는 규칙은 cross-link
	// 자체가 불가능했다. 줄바꿈만 배제(한 줄 안에서만 매칭 — 문단을 가로질러
	// `[[` … `]]` 가 엮이는 오탐 방지)하고 공백/탭은 허용.
	const CROSS_LINK_RE = /\[\[([^[\]\n\r]{1,64})\]\]/g;
	// 별도 non-global tester — /g 의 lastIndex 부작용 없이 acceptNode 에서 검사.
	const CROSS_LINK_TEST = /\[\[[^[\]\n\r]{1,64}\]\]/;
	// quest/campaign 추적번호 형식 (XXX-NNN). 이 형식이 아니면 규칙 slug 로 본다.
	const ID_TOKEN_RE = /^[A-Za-z]{1,}-\d+$/;
	// DEV-219: 접두 없을 때만 쓰는 형태 추정 fallback (미존재 ID 용).
	function guessKindByShape(id: string): 'quest' | 'campaign' | 'rule' | 'book' {
		if (!ID_TOKEN_RE.test(id)) return 'rule';
		if (/^BOOK-\d+$/i.test(id)) return 'book';
		return /^C-\d+$/i.test(id) ? 'campaign' : 'quest';
	}
	// DEV-256에서 발견한 기존 레이스: 첫 렌더 시점에 인덱스가 아직 적재 전이면
	// 링크가 missing(빨강)으로 만들어지는데, 인덱스 적재 후 effect 가 재실행돼도
	// 원본 텍스트 노드는 이미 anchor 로 치환된 뒤라 TreeWalker 가 다시 잡지
	// 못했다 — missing 상태가 영구 고착. anchor 에 원본 토큰(data-xtoken)을
	// 남겨두고, 재실행 시 기존 anchor 들을 다시 resolve 해 갱신한다(reindex 후
	// 존재→미존재 전환도 같은 경로로 반영).
	function applyRefToAnchor(a: HTMLAnchorElement, rawToken: string) {
		const resolved = resolveCrossLinkToken(rawToken);
		const ref = resolved.ref;
		const rawId = resolved.id;
		const id = rawId.toUpperCase();
		const kind = resolved.kind ?? guessKindByShape(id);
		a.href = refHref(id, kind, ref?.slug ?? rawId);
		// DEV-173: 규칙 slug 는 소문자가 정체성 — 원문 그대로 표시.
		a.textContent = kind === 'rule' ? (ref?.slug ?? rawId) : id;
		a.className = ref ? 'xlink' : 'xlink missing';
		a.title = ref
			? `${KIND_LABEL[kind]}: ${ref.title}`
			: `존재하지 않는 ${KIND_LABEL[kind]} — ${kind === 'rule' ? rawId : id}`;
		a.dataset.xtoken = rawToken;
		// DEV-256: 실재 링크만 호버 미리보기 대상 — 팝업이 읽을 메타를
		// data 속성으로 실어둔다(missing 은 보여줄 본문이 없음).
		if (ref) {
			a.dataset.xkind = kind;
			a.dataset.xid = kind === 'rule' ? (ref.slug ?? rawId) : id;
			if (ref.slug) a.dataset.xslug = ref.slug;
			a.dataset.xtitle = ref.title;
		} else {
			delete a.dataset.xkind;
			delete a.dataset.xid;
			delete a.dataset.xslug;
			delete a.dataset.xtitle;
		}
	}
	function rewriteCrossLinks() {
		if (!container) return;
		// 이미 만들어진 anchor 갱신 — 인덱스 늦은 적재/reindex 반영.
		for (const a of Array.from(container.querySelectorAll<HTMLAnchorElement>('a.xlink'))) {
			if (a.dataset.xtoken) applyRefToAnchor(a, a.dataset.xtoken);
		}
		const walker = document.createTreeWalker(container, NodeFilter.SHOW_TEXT, {
			acceptNode(node) {
				const parent = node.parentElement;
				if (!parent) return NodeFilter.FILTER_REJECT;
				// code / pre / 이미 만든 링크 안은 건너뜀.
				if (parent.closest('code, pre, a')) return NodeFilter.FILTER_REJECT;
				return CROSS_LINK_TEST.test(node.nodeValue ?? '')
					? NodeFilter.FILTER_ACCEPT
					: NodeFilter.FILTER_REJECT;
			}
		});
		const targets: Text[] = [];
		let n: Node | null;
		while ((n = walker.nextNode())) targets.push(n as Text);

		for (const textNode of targets) {
			const text = textNode.nodeValue ?? '';
			CROSS_LINK_RE.lastIndex = 0;
			const frag = document.createDocumentFragment();
			let last = 0;
			let m: RegExpExecArray | null;
			while ((m = CROSS_LINK_RE.exec(text))) {
				const whole = m[0];
				// [[token]] 안 — kind: 접두 있을 수 있음(DEV-219).
				// BUG-156: 공백 허용 이후 `[[ 규칙명 ]]` 처럼 여백을 준 표기도
				// 같은 대상으로 풀리도록 trim (내부 공백은 slug 의 일부라 보존).
				const rawToken = m[1].trim();
				if (!rawToken) {
					// `[[   ]]` 같은 빈 토큰은 링크로 만들지 않고 원문 유지.
					continue;
				}
				if (m.index > last) {
					frag.appendChild(document.createTextNode(text.slice(last, m.index)));
				}
				// 접두가 있었으면 그 kind 를 그대로 신뢰(존재 여부와 무관), 없었고
				// 미존재면 형태로 추정 — 상세는 applyRefToAnchor.
				const a = document.createElement('a');
				applyRefToAnchor(a, rawToken);
				frag.appendChild(a);
				last = m.index + whole.length;
			}
			if (last < text.length) {
				frag.appendChild(document.createTextNode(text.slice(last)));
			}
			textNode.parentNode?.replaceChild(frag, textNode);
		}

		// BUG-173: missing 이 하나라도 남았으면 인덱스를 한 번 다시 받아본다.
		// 인덱스는 세션당 1회 적재라, GUI 를 켜둔 채 CLI/에이전트가 만든 문서를
		// 가리키는 링크는 실재하는데도 계속 빨강이었다. 쿨다운이 있어 매 렌더마다
		// 네트워크를 때리지 않고, 성공 시 $questIndex 변경 → 위 재-resolve 경로로
		// 자동 반영된다(진짜 없는 링크는 그대로 빨강).
		if (container.querySelector('a.xlink.missing')) {
			refreshIndexForMissing();
		}
	}

	// DEV-069: markdown 본문의 로컬 이미지 / 동영상 참조 해석.
	//
	// WebView 는 raw 파일 경로 / file:// 로드를 차단 (frontend origin 이
	// http://tauri.localhost) — `![](attachments/foo.png)` 같은 `.guild/` 상대
	// 참조를 asset URL (Tauri convertFileSrc) / 서버 endpoint (브라우저) 로
	// 재작성. 웹 URL (http/https/data/asset) 은 그대로 통과.
	import { guildFileUrl } from '$lib/utils/banner';
	function isExternalSrc(src: string): boolean {
		return /^(https?:|data:|asset:|blob:|\/api\/)/i.test(src) || src.startsWith('//');
	}
	async function rewriteLocalMedia() {
		if (!container) return;
		const targets = container.querySelectorAll<HTMLElement>('img, video, source');
		for (const el of Array.from(targets)) {
			const src = el.getAttribute('src');
			if (!src || isExternalSrc(src) || el.dataset.ogRewritten) continue;
			// 절대 OS 경로 (`C:\...` / `/home/...`) 는 보안상 미지원 — `.guild/`
			// 상대 (attachments/ / assets/) 만. 그 외는 그대로 (깨진 이미지 표시).
			const rel = src.replace(/^\.\//, '');
			if (!rel.startsWith('attachments/') && !rel.startsWith('assets/')) continue;
			try {
				const url = await guildFileUrl(rel);
				el.setAttribute('src', url);
				el.dataset.ogRewritten = '1';
			} catch {
				/* 해석 실패 — 원본 유지 */
			}
		}
	}
</script>

<!-- DEV-256: mouseover/out/click 델리게이션 — 동적 생성되는 a.xlink 에 개별
     리스너를 달 수 없어 컨테이너에서 위임. 키보드 접근성은 링크 자체가 anchor
     라 기본 포커스/이동/Enter 로 이미 제공됨(팝업은 마우스 보조 UI). -->
<!-- svelte-ignore a11y_mouse_events_have_key_events, a11y_no_static_element_interactions, a11y_click_events_have_key_events -->
<div
	class="md"
	bind:this={container}
	onmouseover={onContainerOver}
	onmouseout={onContainerOut}
	onclick={onContainerClick}
>
	{@html html}
</div>

{#if PopupComp && hoverTarget}
	<PopupComp
		{...hoverTarget}
		onenter={cancelClose}
		onleave={scheduleClose}
		onnavigate={() => (hoverTarget = null)}
	/>
{/if}

<style>
	.md {
		background: var(--bg);
		border: 1px solid var(--bg-subtle);
		border-radius: 6px;
		padding: 0.85rem 1rem;
		color: var(--text);
		font-size: 0.9rem;
		line-height: 1.55;
		/* BUG-039: 자식 (긴 링크 / inline code 등) 이 컨테이너 폭을 못 넘게. */
		max-width: 100%;
		overflow-wrap: anywhere;
	}

	/* 헤더 — 캠페인 spirit (브라우저 기본 사이즈로 명확 구분, 컬러만 통일). */
	.md :global(h1),
	.md :global(h2),
	.md :global(h3),
	.md :global(h4),
	.md :global(h5),
	.md :global(h6) {
		color: var(--text-strong);
		margin: 1em 0 0.4em;
	}

	.md :global(p) {
		margin: 0.5em 0;
	}
	.md :global(ul),
	.md :global(ol) {
		padding-left: 1.5rem;
		margin: 0.4em 0;
	}
	.md :global(input[type='checkbox']) {
		margin-right: 0.4rem;
	}

	.md :global(code) {
		background: var(--bg-elevated);
		padding: 0.1rem 0.3rem;
		border-radius: 3px;
	}
	.md :global(pre) {
		background: var(--bg-elevated);
		border: 1px solid var(--bg-subtle);
		border-radius: 6px;
		padding: 0.75rem 1rem;
		overflow-x: auto;
	}
	.md :global(pre code) {
		background: none;
		padding: 0;
		color: var(--text);
	}
	/* DEV-183: highlight.js 토큰 매핑 테마 — 고정 hex 대신 테마 토큰을 써서
	   다크/라이트 자동 적응 (컴포넌트 CSS no-hex 정책 준수). */
	.md :global(.hljs-comment),
	.md :global(.hljs-quote) {
		color: var(--text-faint);
		font-style: italic;
	}
	.md :global(.hljs-keyword),
	.md :global(.hljs-selector-tag),
	.md :global(.hljs-literal),
	.md :global(.hljs-section),
	.md :global(.hljs-built_in),
	.md :global(.hljs-type),
	.md :global(.hljs-name),
	.md :global(.hljs-title) {
		color: var(--accent);
	}
	.md :global(.hljs-string),
	.md :global(.hljs-regexp),
	.md :global(.hljs-attr),
	.md :global(.hljs-attribute),
	.md :global(.hljs-addition),
	.md :global(.hljs-symbol) {
		color: var(--success);
	}
	.md :global(.hljs-number),
	.md :global(.hljs-bullet),
	.md :global(.hljs-meta) {
		color: var(--warning);
	}
	.md :global(.hljs-deletion) {
		color: var(--danger);
	}
	.md :global(.hljs-emphasis) {
		font-style: italic;
	}
	.md :global(.hljs-strong) {
		font-weight: 700;
	}
	.md :global(blockquote) {
		border-left: 3px solid var(--border);
		margin: 0.5em 0;
		padding: 0.25em 0.75em;
		color: var(--text-muted);
	}
	/* BUG-039: 긴 URL 이 본문 폭을 넘어 가로 스크롤 발생하던 문제 — anywhere
	   으로 break (`word-break: break-word` 는 deprecated 대안 — 둘 다 적용해서
	   브라우저 지원 폭 확보). */
	.md :global(a) {
		color: var(--accent);
		overflow-wrap: anywhere;
		word-break: break-word;
	}
	/* DEV-140: cross-link 칩 — 인라인 ID 링크. 미존재 ID 는 빨강. */
	.md :global(a.xlink) {
		font-weight: 600;
		text-decoration: none;
		background: color-mix(in srgb, var(--accent) 12%, transparent);
		border: 1px solid color-mix(in srgb, var(--accent) 30%, transparent);
		border-radius: 4px;
		padding: 0 0.3rem;
		white-space: nowrap;
	}
	.md :global(a.xlink:hover) {
		background: color-mix(in srgb, var(--accent) 22%, transparent);
	}
	.md :global(a.xlink.missing) {
		color: var(--danger);
		background: color-mix(in srgb, var(--danger) 12%, transparent);
		border-color: color-mix(in srgb, var(--danger) 35%, transparent);
	}
	.md :global(a.xlink.missing:hover) {
		background: color-mix(in srgb, var(--danger) 20%, transparent);
	}
	.md :global(hr) {
		border: none;
		border-top: 1px solid var(--bg-subtle);
		margin: 1em 0;
	}
	.md :global(table) {
		border-collapse: collapse;
		width: 100%;
		font-size: 0.875rem;
	}
	.md :global(th),
	.md :global(td) {
		border: 1px solid var(--bg-subtle);
		padding: 0.35rem 0.6rem;
	}
	.md :global(th) {
		background: var(--bg-elevated);
	}
	/* DEV-335/BUG-224: 첨부 이미지·동영상 HDR 표시 제한 — 설정(hdrSettings)이
	   `<html>` 에 쓰는 `--hdr-limit` 를 따라간다. 미지원 브라우저는 프로퍼티
	   자체가 무시됨. 원래 img 만 걸려있었음 — HDR 동영상엔 안 먹힘. */
	.md :global(img) {
		max-width: 100%;
	}
	.md :global(img),
	.md :global(video) {
		dynamic-range-limit: var(--hdr-limit, no-limit);
	}
	/* DEV-111: mermaid 다이어그램 렌더 영역. */
	.md :global(.mermaid-rendered) {
		display: flex;
		justify-content: center;
		padding: 0.5rem 0;
		margin: 0.5em 0;
		/* BUG-237: `.md` 는 BUG-039 때문에 `overflow-wrap: anywhere` 를 건다
		   (긴 링크/inline code 가 컨테이너를 넘지 않게). 그런데 mermaid 는 노드·엣지
		   라벨을 `<foreignObject>` 안 HTML 로 그리므로 그 규칙을 **그대로 상속**한다.

		   그 결과 라벨이 **단어 중간에서 강제로 줄바꿈**된다 — 실측 사례:
		   `CSP_UpdateMaintenanceStates` → `CSP_UpdateMaintenanceSt` / `ates`.
		   mermaid 는 끊기지 않은 너비로 상자 크기를 이미 정해놨으므로, 줄이 늘어난
		   텍스트가 상자를 넘쳐 잘려 보인다.

		   다이어그램 안에서는 상속을 끊는다. 라벨은 저자가 `<br/>` 로 준 곳에서만
		   줄바꿈되어야 mermaid 의 측정과 실제 렌더가 일치한다. */
		overflow-wrap: normal;
		word-break: normal;
	}
	.md :global(.mermaid-rendered svg) {
		max-width: 100%;
		height: auto;
	}
	.md :global(.mermaid-error) {
		padding: 0.5rem 0.75rem;
		background: color-mix(in srgb, var(--danger) 12%, transparent);
		border: 1px solid color-mix(in srgb, var(--danger) 40%, transparent);
		border-radius: 6px;
		color: var(--danger);
		font-size: 0.8rem;
		font-family: 'SFMono-Regular', Consolas, monospace;
		white-space: pre-wrap;
	}
</style>
