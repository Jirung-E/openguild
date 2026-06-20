<!--
  DEV-094: Quest Detail 의 댓글 섹션 (entry 단위 + 답글).

  - 목록: top-level entry → 본문 + 답글 목록 (들여쓰기).
  - 편집: 본인 entry "✎ 편집" → inline textarea (작성 시각/작성자 보존).
  - 삭제: "× 삭제" 확인 후 entry 제거. parent 삭제 시 자식은 orphan reference
    로 남되, render 단계에서 "(삭제된 댓글에 대한 답글)" 로 안내.
  - 답글: 각 entry 의 "↩ 답글" → inline form. parent_id 자동 채움.
  - 새 top-level 댓글: 하단 폼.

  parent_id 관계는 1-level threading — 답글의 답글도 동일 root 의 자식으로
  flatten (GitHub PR 댓글 모델). frontend 가 자유롭게 indent / 시각화 가능.
-->
<script lang="ts">
	import { tick, onDestroy } from 'svelte';
	import MarkdownView from './MarkdownView.svelte';
	// DEV-153: 작성/편집/답글 중이면 이탈 가드에 보고.
	import { setUnsaved } from '$lib/stores/unsaved';
	import {
		commentsApi as questCommentsApi,
		campaignCommentsApi,
		type CommentEntry
	} from '$lib/api/comments';
	// DEV-118: native confirm() 대신 인앱 모달.
	import ConfirmDialog from './ConfirmDialog.svelte';
	// DEV-130: Tab = tab 문자 삽입 (focus 이동 X).
	import { tabInsert } from '$lib/actions/tab-insert';
	// DEV-151: 댓글 textarea 첨부 — paste/drag&drop/버튼.
	import { textareaAttach, pickAndAttachTextarea } from '$lib/utils/editor-attach';
	// DEV-140/171: 댓글 textarea cross-link 자동완성 — caret 위치 팝업 + 실재 ID 제안.
	import { wikiMatch, applyWikiLink, caretXY, type WikiItem } from '$lib/utils/textarea-wikilink';
	import { questIndex, loadQuestIndex } from '$lib/stores/questIndex';
	import { get } from 'svelte/store';

	loadQuestIndex();
	// DEV-171: caret 위치 팝업 — 활성 textarea + 후보 + 화면 좌표 + 선택 index.
	let wiki = $state<{
		el: HTMLTextAreaElement;
		from: number;
		to: number;
		items: WikiItem[];
		left: number;
		top: number;
	} | null>(null);
	let wikiSel = $state(0);

	function onWikiInput(e: Event) {
		const el = e.currentTarget as HTMLTextAreaElement;
		const caret = el.selectionStart ?? 0;
		const m = wikiMatch(el.value, caret, get(questIndex));
		if (!m) {
			wiki = null;
			return;
		}
		const c = caretXY(el, m.to);
		const rect = el.getBoundingClientRect();
		wiki = {
			el,
			from: m.from,
			to: m.to,
			items: m.items,
			left: rect.left + c.left - el.scrollLeft,
			top: rect.top + c.top - el.scrollTop + c.height
		};
		wikiSel = 0;
	}
	function applyWiki(item: WikiItem) {
		if (!wiki) return;
		applyWikiLink(wiki.el, wiki.from, wiki.to, item.id);
		wiki = null;
	}
	// VS 식 키보드 네비 (팝업 떠 있을 때만 가로챔).
	function onWikiKeydown(e: KeyboardEvent) {
		if (!wiki) return;
		const n = wiki.items.length;
		if (e.key === 'ArrowDown') {
			e.preventDefault();
			wikiSel = (wikiSel + 1) % n;
		} else if (e.key === 'ArrowUp') {
			e.preventDefault();
			wikiSel = (wikiSel - 1 + n) % n;
		} else if (e.key === 'Enter') {
			e.preventDefault();
			applyWiki(wiki.items[wikiSel]);
		} else if (e.key === 'Escape') {
			e.preventDefault();
			wiki = null;
		}
	}

	// 첨부 버튼이 삽입할 textarea 참조 (편집/답글은 한 번에 하나만 열림).
	let newTextareaEl = $state<HTMLTextAreaElement | undefined>(undefined);
	let editTextareaEl = $state<HTMLTextAreaElement | undefined>(undefined);
	let replyTextareaEl = $state<HTMLTextAreaElement | undefined>(undefined);

	// DEV-100: scope — quest (기본) / campaign. API base 만 다름.
	let { slug, scope = 'quest' }: { slug: string; scope?: 'quest' | 'campaign' } = $props();
	const commentsApi = $derived(scope === 'campaign' ? campaignCommentsApi : questCommentsApi);

	let loading = $state(true);
	let loadError = $state<string | null>(null);
	let entries = $state<CommentEntry[]>([]);

	// DEV-107 fix1: 섹션 접기 — 사용자 피드백 반영해 localStorage 영속 제거.
	// 매 진입 시 펼침 기본. 일회성 토글.
	let collapsed = $state(false);
	function toggleCollapsed() {
		collapsed = !collapsed;
	}

	// DEV-107 fix1: root entry (top-level 댓글) 별 답글 접기.
	// 클릭 시 그 root 의 답글 전체 숨김 (들여쓰기 손실 없음 — 그냥 표시 안 함).
	let collapsedRoots = $state(new Set<number>());
	function toggleRootCollapsed(rootId: number) {
		const next = new Set(collapsedRoots);
		if (next.has(rootId)) next.delete(rootId);
		else next.add(rootId);
		collapsedRoots = next;
	}

	// DEV-108: 이모지 반응 — 고정 4종. 커스텀 추가는 후속 quest.
	// DEV-139: 전체 노출 대신 slack 스타일 — 활성 pill + '+' popup picker.
	const REACTION_SET = ['👍', '✅', '❓', '❌'];
	let pickerOpenFor = $state<number | null>(null);
	// DEV-108: reaction 항목 = "emoji" 또는 "emoji:author1|author2".
	// 누가 반응했는지 호버로 보여주기 위해 파싱.
	function parseReaction(r: string): { emoji: string; authors: string[] } {
		const idx = r.indexOf(':');
		if (idx < 0) return { emoji: r, authors: [] };
		return {
			emoji: r.slice(0, idx),
			authors: r
				.slice(idx + 1)
				.split('|')
				.map((a) => a.trim())
				.filter((a) => a.length > 0)
		};
	}
	function reactionsOf(e: CommentEntry): { emoji: string; authors: string[] }[] {
		return (e.reactions ?? []).map(parseReaction);
	}
	// 현재 사용자(=댓글 작성자 이름). 비어있으면 core 가 '(익명)' 처리.
	function currentAuthor(): string {
		return newAuthor.trim() || loadSavedAuthor();
	}
	function reactedByMe(authors: string[]): boolean {
		const me = currentAuthor().trim() || '(익명)';
		return authors.includes(me);
	}

	async function toggleReaction(id: number, emoji: string) {
		pickerOpenFor = null;
		try {
			const updated = await commentsApi.toggleReaction(slug, id, emoji, currentAuthor());
			entries = entries.map((e) => (e.id === id ? updated : e));
		} catch (e) {
			alert(e instanceof Error ? e.message : 'reaction failed');
		}
	}

	// DEV-142: 토론(discussion) 플래그 토글. discussion 댓글이 미해결이면
	// 이 quest 를 완료 상태로 전환할 수 없다 (core 게이트).
	async function toggleDiscussion(id: number) {
		try {
			const updated = await commentsApi.toggleDiscussion(slug, id);
			entries = entries.map((e) => (e.id === id ? updated : e));
		} catch (e) {
			alert(e instanceof Error ? e.message : 'discussion toggle failed');
		}
	}
	// DEV-142: discussion 댓글 resolve 토글.
	async function toggleResolved(id: number) {
		try {
			const updated = await commentsApi.toggleResolved(slug, id);
			entries = entries.map((e) => (e.id === id ? updated : e));
		} catch (e) {
			alert(e instanceof Error ? e.message : 'resolve toggle failed');
		}
	}

	// DEV-129: 댓글 '내용' 접기 — entry 단위 본문 collapse. 답글 접기 (위)
	// 와 별개 — 본문만 가리고 head (작성자/번호/액션) 는 유지.
	let collapsedBodies = $state(new Set<number>());
	function toggleBodyCollapsed(id: number) {
		const next = new Set(collapsedBodies);
		if (next.has(id)) next.delete(id);
		else next.add(id);
		collapsedBodies = next;
	}
	// 접었을 때 보여줄 1줄 미리보기 — markdown 마커 대충 제거.
	function bodyPreview(body: string): string {
		const firstLine =
			body
				.split('\n')
				.map((l) => l.trim())
				.find((l) => l.length > 0) ?? '';
		const plain = firstLine.replace(/^#+\s*/, '').replace(/[*_`>]/g, '');
		return plain.length > 80 ? plain.slice(0, 80) + '…' : plain;
	}

	// DEV-136: 마지막 작성자 기억 — 비우면 "(이름 없음)" 으로 떠서 매번
	// 입력해야 하는 마찰 제거. localStorage prefill, 저장 성공 시 갱신.
	const AUTHOR_KEY = 'openguild.commentAuthor';
	function loadSavedAuthor(): string {
		try {
			return localStorage.getItem(AUTHOR_KEY) ?? '';
		} catch {
			return '';
		}
	}
	function saveAuthor(name: string) {
		try {
			const n = name.trim();
			if (n) localStorage.setItem(AUTHOR_KEY, n);
		} catch {
			/* 무시 */
		}
	}

	// 신규 top-level 작성 폼
	let newAuthor = $state(loadSavedAuthor());
	let newBody = $state('');
	let saving = $state(false);
	let saveError = $state<string | null>(null);

	// 개별 편집 — 한 번에 하나만.
	let editingId = $state<number | null>(null);
	let editBody = $state('');
	let editSaving = $state(false);
	let editError = $state<string | null>(null);

	// 답글 작성 — 한 번에 한 parent.
	let replyingTo = $state<number | null>(null);
	let replyAuthor = $state(loadSavedAuthor());
	let replyBody = $state('');
	let replySaving = $state(false);

	// DEV-153: 새 댓글에 입력했거나 편집/답글이 열려 있으면 미저장 — 이탈 가드 보고.
	let commentsDirty = $derived(
		newBody.trim() !== '' || editingId !== null || replyingTo !== null
	);
	$effect(() => setUnsaved(`comments:${scope}`, commentsDirty));
	onDestroy(() => setUnsaved(`comments:${scope}`, false));
	let replyError = $state<string | null>(null);

	async function load() {
		loading = true;
		loadError = null;
		try {
			const res = await commentsApi.listComments(slug);
			entries = res.entries ?? [];
		} catch (e) {
			loadError = e instanceof Error ? e.message : 'load failed';
		} finally {
			loading = false;
		}
	}

	$effect(() => {
		void slug;
		load();
	});

	// 화면 구조: roots (parent_id == null) + 각 root 의 replies (parent_id == root.id).
	// 답글의 답글은 root 의 자식으로 flatten (1-level) — server 가 parent_id 가
	// 다른 reply 를 가리키게 둘 수 있지만 frontend 는 가장 가까운 root 까지 거슬러
	// 올라가 그 root 의 자식으로 묶음. 단순 단일-패스로 구현:
	//   1. id → entry 맵.
	//   2. 각 entry 마다 root_id 찾기 (parent_id chain 따라가서 None 인 entry).
	//   3. root 가 사라진 reply 는 "orphan" 표시.
	let groups = $derived.by(() => {
		const byId = new Map<number, CommentEntry>();
		for (const e of entries) byId.set(e.id, e);
		const rootOf = (id: number): number | null => {
			let cur = byId.get(id);
			const visited = new Set<number>();
			while (cur && cur.parent_id != null) {
				if (visited.has(cur.id)) return null; // cycle 방어
				visited.add(cur.id);
				const next = byId.get(cur.parent_id);
				if (!next) return null; // orphan
				cur = next;
			}
			return cur ? cur.id : null;
		};
		const roots: CommentEntry[] = [];
		const childrenByRoot = new Map<number, CommentEntry[]>();
		const orphans: CommentEntry[] = [];
		for (const e of entries) {
			if (e.parent_id == null) {
				roots.push(e);
				childrenByRoot.set(e.id, []);
			}
		}
		for (const e of entries) {
			if (e.parent_id == null) continue;
			const r = rootOf(e.id);
			if (r == null) {
				orphans.push(e);
			} else {
				const arr = childrenByRoot.get(r) ?? [];
				arr.push(e);
				childrenByRoot.set(r, arr);
			}
		}
		return { roots, childrenByRoot, orphans };
	});

	function formatTs(ts: string): string {
		if (!ts) return '(시각 미상)';
		try {
			const d = new Date(ts);
			if (Number.isNaN(d.getTime())) return ts;
			return d.toLocaleString();
		} catch {
			return ts;
		}
	}

	async function add() {
		if (!newBody.trim()) {
			saveError = '본문을 입력하세요.';
			return;
		}
		saving = true;
		saveError = null;
		try {
			const entry = await commentsApi.addComment(slug, newBody, newAuthor, null);
			saveAuthor(newAuthor); // DEV-136: 성공 시 기억.
			entries = [...entries, entry];
			newBody = '';
		} catch (e) {
			saveError = e instanceof Error ? e.message : 'save failed';
		} finally {
			saving = false;
		}
	}

	function enterEdit(e: CommentEntry) {
		editingId = e.id;
		editBody = e.body;
		editError = null;
	}
	function cancelEdit() {
		editingId = null;
		editBody = '';
		editError = null;
	}

	async function saveEdit(id: number) {
		if (!editBody.trim()) {
			editError = '본문을 입력하세요.';
			return;
		}
		editSaving = true;
		editError = null;
		try {
			const updated = await commentsApi.updateComment(slug, id, editBody);
			entries = entries.map((e) => (e.id === id ? updated : e));
			cancelEdit();
		} catch (e) {
			editError = e instanceof Error ? e.message : 'save failed';
		} finally {
			editSaving = false;
		}
	}

	// DEV-118: 인앱 confirm 모달용 state.
	let confirmDeleteId = $state<number | null>(null);
	function askRemove(id: number) {
		confirmDeleteId = id;
	}
	async function remove() {
		const id = confirmDeleteId;
		if (id === null) return;
		confirmDeleteId = null;
		try {
			await commentsApi.deleteComment(slug, id);
			entries = entries.filter((e) => e.id !== id);
		} catch (e) {
			alert(e instanceof Error ? e.message : 'delete failed');
		}
	}

	// DEV-120: 답글 폼 자동 focus + scroll.
	// 원 댓글이 길면 폼이 화면 밖에 나타나서 "↩ 답글" 클릭 후 아무 일도 안 일어난
	// 것처럼 보임. 폼이 mount 된 후 textarea focus + 화면 중앙으로 scroll.
	async function enterReply(parentId: number) {
		replyingTo = parentId;
		replyBody = '';
		replyError = null;
		await tick();
		// 새로 mount 된 .reply-form 의 textarea — 한 번에 한 폼만 떠 있음.
		const form = document.querySelector<HTMLElement>('.reply-form');
		const ta = form?.querySelector<HTMLTextAreaElement>('textarea.body-input');
		if (!ta) return;
		try {
			ta.scrollIntoView({ behavior: 'smooth', block: 'center' });
		} catch {
			// 일부 환경에서 옵션 미지원 — fallback.
			ta.scrollIntoView();
		}
		ta.focus({ preventScroll: true });
	}
	function cancelReply() {
		replyingTo = null;
		replyBody = '';
		replyError = null;
	}

	async function submitReply(parentId: number) {
		if (!replyBody.trim()) {
			replyError = '본문을 입력하세요.';
			return;
		}
		replySaving = true;
		replyError = null;
		try {
			const entry = await commentsApi.addComment(slug, replyBody, replyAuthor, parentId);
			saveAuthor(replyAuthor); // DEV-136: 성공 시 기억.
			entries = [...entries, entry];
			cancelReply();
		} catch (e) {
			replyError = e instanceof Error ? e.message : 'save failed';
		} finally {
			replySaving = false;
		}
	}
</script>

{#snippet entryView(e: CommentEntry, isReply: boolean)}
	<!-- DEV-139: li → div — root + 답글을 하나의 카드 (entry-card) 로 감싸기 위해. -->
	<div class="entry" class:reply={isReply} id={`comment-${e.id}`}>
		<div class="entry-head">
			<!-- DEV-128 → DEV-139: 댓글 번호 — 클릭 시 본문 접기/펼치기 ('내용' 버튼 대체). -->
			<button
				class="entry-no"
				onclick={() => toggleBodyCollapsed(e.id)}
				aria-expanded={!collapsedBodies.has(e.id)}
				title={collapsedBodies.has(e.id) ? `#${e.id} 내용 펼치기` : `#${e.id} 내용 접기`}
			>#{e.id}</button>
			{#if e.parent_id != null}
				<a class="reply-to" href={`#comment-${e.parent_id}`} title={`#${e.parent_id} 댓글로 이동`}>↩ #{e.parent_id}</a>
			{/if}
			<span class="author">{e.author || '(이름 없음)'}</span>
			<span class="sep">·</span>
			<time class="ts" datetime={e.ts}>{formatTs(e.ts)}</time>
			<!-- DEV-142: 토론 댓글 상태 배지 — 미해결이면 완료 차단 (quest 한정).
			     클릭으로 resolve 토글. -->
			{#if scope === 'quest' && e.discussion}
				<button
					class="disc-badge"
					class:resolved={e.resolved}
					onclick={() => toggleResolved(e.id)}
					title={e.resolved ? '해결됨 — 클릭하면 다시 미해결로' : '미해결 토론 — 클릭하면 해결 처리 (완료 차단 해제)'}
				>{e.resolved ? '✓ 해결됨' : '● 미해결 토론'}</button>
			{/if}
			{#if editingId !== e.id}
				<div class="entry-actions">
					{#if scope === 'quest'}
						<button
							class="link-btn"
							class:on={e.discussion}
							onclick={() => toggleDiscussion(e.id)}
							title={e.discussion ? '토론 표시 해제' : '토론으로 표시 — resolve 전까지 완료 차단'}
						>💬 토론</button>
					{/if}
					<button class="link-btn" onclick={() => enterEdit(e)}>✎ 편집</button>
					<button class="link-btn danger" onclick={() => askRemove(e.id)}>× 삭제</button>
				</div>
			{/if}
		</div>
		{#if editingId === e.id}
			<textarea
				use:tabInsert
				use:textareaAttach={{ onError: (m) => (editError = `첨부 실패: ${m}`) }}
				bind:this={editTextareaEl}
				class="body-input"
				bind:value={editBody}
				oninput={onWikiInput}
				onkeyup={onWikiInput}
				onclick={onWikiInput}
				onkeydown={onWikiKeydown}
				rows="4"
				placeholder="본문 (markdown)"
			></textarea>
			{#if editError}<p class="state err">{editError}</p>{/if}
			<div class="actions">
				<button
					type="button"
					class="btn-attach"
					onclick={() =>
						pickAndAttachTextarea(editTextareaEl, (m) => (editError = `첨부 실패: ${m}`))}
					title="이미지·동영상·파일 첨부 (드래그&드랍 / Ctrl+V 도 가능)"
				>📎 첨부</button>
				<button class="btn-save" onclick={() => saveEdit(e.id)} disabled={editSaving}>
					{editSaving ? '저장…' : '저장'}
				</button>
				<button class="btn-cancel" onclick={cancelEdit} disabled={editSaving}>취소</button>
			</div>
		{:else if collapsedBodies.has(e.id)}
			<!-- DEV-129: 접힌 본문 — 1줄 미리보기, 클릭으로 펼침. -->
			<button class="body-collapsed" onclick={() => toggleBodyCollapsed(e.id)} title="내용 펼치기">
				{bodyPreview(e.body)}
			</button>
		{:else}
			<div class="entry-body">
				<MarkdownView source={e.body} />
			</div>
		{/if}
		{#if editingId !== e.id}
			{@const reacts = reactionsOf(e)}
			<!-- DEV-139: 푸터 행 — 좌측 답글 컨트롤 / 우측 이모지 (slack 스타일). -->
			<div class="entry-foot">
				<div class="foot-left">
					{#if !isReply}
						{@const childCount = (groups.childrenByRoot.get(e.id) ?? []).length}
						{@const isThreadCollapsed = collapsedRoots.has(e.id)}
						{#if childCount > 0}
							<!-- 삼각형만 클릭 — '답글 n' 텍스트는 표시 전용. -->
							<button
								class="tri-btn"
								onclick={() => toggleRootCollapsed(e.id)}
								aria-expanded={!isThreadCollapsed}
								title={isThreadCollapsed ? '답글 펼치기' : '답글 접기'}
							>{isThreadCollapsed ? '▶' : '▼'}</button>
							<span class="reply-count">답글 {childCount}</span>
						{/if}
						<button class="reply-write-btn" onclick={() => enterReply(e.id)}>답글 쓰기</button>
					{/if}
				</div>
				<div class="foot-right">
					{#each reacts as r (r.emoji)}
						<!-- DEV-108: 호버하면 누가 반응했는지 (authors) 표시. -->
						<button
							class="reaction-pill"
							class:mine={reactedByMe(r.authors)}
							onclick={() => toggleReaction(e.id, r.emoji)}
							title={r.authors.length ? `${r.authors.join(', ')} · 클릭하면 토글` : '클릭하면 토글'}
						>
							{r.emoji}{#if r.authors.length > 1}<span class="rc">{r.authors.length}</span>{/if}
						</button>
					{/each}
					<div class="picker-wrap">
						<button
							class="reaction-add"
							onclick={() => (pickerOpenFor = pickerOpenFor === e.id ? null : e.id)}
							aria-expanded={pickerOpenFor === e.id}
							title="반응 추가"
						>☺+</button>
						{#if pickerOpenFor === e.id}
							<div class="picker-ov" role="presentation" onclick={() => (pickerOpenFor = null)}></div>
							<div class="reaction-picker" role="menu">
								{#each REACTION_SET as emoji (emoji)}
									<button
										class="picker-item"
										class:on={reactedByMe(reacts.find((x) => x.emoji === emoji)?.authors ?? [])}
										onclick={() => toggleReaction(e.id, emoji)}
									>{emoji}</button>
								{/each}
							</div>
						{/if}
					</div>
				</div>
			</div>
		{/if}
	</div>
{/snippet}

<section class="comments-sec">
	<div class="section-head">
		<!-- DEV-107: 섹션 토글 — title 전체 클릭 가능. -->
		<button
			type="button"
			class="section-toggle"
			onclick={toggleCollapsed}
			aria-expanded={!collapsed}
			title={collapsed ? '댓글 펼치기' : '댓글 접기'}
		>
			<span class="toggle-icon" class:collapsed>▼</span>
			<h2 class="section-title">댓글 (Comments)</h2>
		</button>
		<span class="count">{entries.length}</span>
	</div>

	{#if !collapsed}
	{#if loading}
		<p class="state">Loading…</p>
	{:else if loadError}
		<p class="state err">{loadError}</p>
	{:else}
		{#if entries.length === 0}
			<p class="no-desc">아직 댓글 없음.</p>
		{:else}
			<ul class="entry-list">
				{#each groups.roots as root (root.id)}
					{@const childCount = (groups.childrenByRoot.get(root.id) ?? []).length}
					{@const isCollapsed = collapsedRoots.has(root.id)}
					<!-- DEV-139: root + 답글을 하나의 카드로 — 댓글 간 시각 구분. -->
					<li class="entry-card">
						{@render entryView(root, false)}
						{#if (childCount > 0 && !isCollapsed) || replyingTo === root.id}
							<div class="thread">
								<div class="reply-list">
									{#if !isCollapsed}
										{#each groups.childrenByRoot.get(root.id) ?? [] as r (r.id)}
											{@render entryView(r, true)}
										{/each}
									{/if}
									{#if replyingTo === root.id}
										<div class="reply-form">
											<div class="reply-author">
												<input
													class="author-input"
													type="text"
													placeholder="작성자 (옵션)"
													bind:value={replyAuthor}
													disabled={replySaving}
												/>
											</div>
											<textarea
												use:tabInsert
												use:textareaAttach={{
													onError: (m) => (replyError = `첨부 실패: ${m}`)
												}}
												bind:this={replyTextareaEl}
												class="body-input"
												bind:value={replyBody}
												oninput={onWikiInput}
												onkeyup={onWikiInput}
												onclick={onWikiInput}
												onkeydown={onWikiKeydown}
												rows="3"
												placeholder={`@${root.author || root.id} 에 답글…`}
												disabled={replySaving}
											></textarea>
											{#if replyError}<p class="state err">{replyError}</p>{/if}
											<div class="actions">
												<button
													type="button"
													class="btn-attach"
													onclick={() =>
														pickAndAttachTextarea(
															replyTextareaEl,
															(m) => (replyError = `첨부 실패: ${m}`)
														)}
													title="이미지·동영상·파일 첨부 (드래그&드랍 / Ctrl+V 도 가능)"
												>📎 첨부</button>
												<button
													class="btn-save"
													onclick={() => submitReply(root.id)}
													disabled={replySaving || !replyBody.trim()}
												>
													{replySaving ? '저장…' : '답글 추가'}
												</button>
												<button class="btn-cancel" onclick={cancelReply} disabled={replySaving}>
													취소
												</button>
											</div>
										</div>
									{/if}
								</div>
							</div>
						{/if}
					</li>
				{/each}
				{#if groups.orphans.length > 0}
					<li class="entry-card orphan-card">
						<span class="orphan-label">↩ 삭제된 댓글에 대한 답글</span>
						{#each groups.orphans as o (o.id)}
							{@render entryView(o, true)}
						{/each}
					</li>
				{/if}
			</ul>
		{/if}

		<!-- 새 top-level 댓글 -->
		<div class="new-form">
			<div class="new-row">
				<input
					class="author-input"
					type="text"
					placeholder="작성자 (옵션)"
					bind:value={newAuthor}
					disabled={saving}
				/>
			</div>
			<textarea
				use:tabInsert
				use:textareaAttach={{ onError: (m) => (saveError = `첨부 실패: ${m}`) }}
				bind:this={newTextareaEl}
				class="body-input"
				bind:value={newBody}
				oninput={onWikiInput}
				onkeyup={onWikiInput}
				onclick={onWikiInput}
				onkeydown={onWikiKeydown}
				rows="3"
				placeholder="댓글 작성 (markdown 사용 가능)"
				disabled={saving}
			></textarea>
			{#if saveError}<p class="state err">{saveError}</p>{/if}
			<div class="actions">
				<button
					type="button"
					class="btn-attach"
					onclick={() =>
						pickAndAttachTextarea(newTextareaEl, (m) => (saveError = `첨부 실패: ${m}`))}
					title="이미지·동영상·파일 첨부 (드래그&드랍 / Ctrl+V 도 가능)"
				>📎 첨부</button>
				<button
					class="btn-save"
					onclick={add}
					disabled={saving || !newBody.trim()}
				>
					{saving ? '추가…' : '+ 댓글 추가'}
				</button>
			</div>
		</div>
	{/if}
	{/if}
</section>

<!-- DEV-118: 댓글 삭제 확인 모달. -->
<ConfirmDialog
	open={confirmDeleteId !== null}
	title="댓글 삭제"
	message="이 댓글을 삭제할까요? (답글이 있다면 그대로 남고 안내가 표시됩니다)"
	confirmLabel="삭제"
	danger
	onconfirm={remove}
	oncancel={() => (confirmDeleteId = null)}
/>

<!-- DEV-171: cross-link 자동완성 팝업 — caret 위치에 떠서 실재 ID 후보 표시. -->
{#if wiki}
	<ul class="wiki-pop" style="left:{wiki.left}px; top:{wiki.top}px;">
		{#each wiki.items as it, i (it.id)}
			<li>
				<button
					type="button"
					class="wiki-opt"
					class:sel={i === wikiSel}
					onmousedown={(ev) => {
						ev.preventDefault();
						applyWiki(it);
					}}
					onmouseenter={() => (wikiSel = i)}
				>
					<span class="wiki-id" class:missing={!it.exists}>🔗 {it.id}</span>
					<span class="wiki-meta">{it.exists ? it.title : '새 링크 (미존재)'}</span>
				</button>
			</li>
		{/each}
	</ul>
{/if}

<style>
	.comments-sec { margin-bottom: 1.5rem; }
	.section-head {
		display: flex; align-items: center; gap: 0.5rem;
		margin-bottom: 0.5rem;
	}
	/* DEV-107: 섹션 토글 — title 자체를 button 으로 만들어 클릭 가능. */
	.section-toggle {
		display: flex; align-items: center; gap: 0.4rem;
		background: none; border: none; padding: 0; cursor: pointer;
		color: inherit; font: inherit;
	}
	.section-toggle:hover .section-title { color: var(--text); }
	.toggle-icon {
		font-size: 0.65rem;
		color: var(--text-muted);
		transition: transform 0.12s;
		display: inline-block;
	}
	.toggle-icon.collapsed {
		transform: rotate(-90deg);
	}
	.section-title {
		font-size: 0.8rem; font-weight: 600;
		text-transform: uppercase; letter-spacing: 0.05em; margin: 0;
		color: var(--accent);
		transition: color 0.12s;
	}
	.count {
		font-size: 0.8rem;
		color: var(--text-muted);
	}

	.state { color: var(--text-muted); font-size: 0.825rem; margin: 0.25rem 0; }
	.state.err { color: var(--danger); }
	.no-desc { color: var(--text-faint); font-size: 0.825rem; margin: 0.25rem 0; }

	.entry-list {
		list-style: none;
		margin: 0 0 1rem;
		padding: 0;
		display: flex;
		flex-direction: column;
		gap: 0.75rem;
	}
	/* DEV-139: root + 답글을 감싸는 카드 — 댓글 간 시각 구분.
	   본문 (entry-body 의 MarkdownView) 은 --bg 라 카드 배경과 한 단계 차이. */
	.entry-card {
		list-style: none;
		background: color-mix(in srgb, var(--bg-elevated) 65%, var(--bg));
		border: 1px solid var(--bg-subtle);
		border-radius: 8px;
		padding: 0.6rem 0.75rem;
	}
	.entry {
		border-radius: 6px;
	}
	.thread {
		margin: 0;
		padding: 0;
	}
	.reply-list {
		margin: 0.25rem 0 0 1.5rem;
		padding-left: 0.75rem;
		border-left: 2px solid var(--bg-subtle);
		display: flex;
		flex-direction: column;
		gap: 0.5rem;
	}
	.reply-form {
		border: 1px dashed var(--border);
		border-radius: 6px;
		padding: 0.5rem 0.7rem;
		display: flex;
		flex-direction: column;
		gap: 0.35rem;
	}
	.reply-author { display: flex; gap: 0.4rem; }

	.orphan-card { display: flex; flex-direction: column; gap: 0.5rem; }
	.orphan-label {
		font-size: 0.72rem;
		color: var(--text-muted);
		font-style: italic;
	}

	.entry-head {
		display: flex;
		align-items: center;
		gap: 0.4rem;
		font-size: 0.78rem;
		color: var(--text-muted);
		margin-bottom: 0.4rem;
	}
	.author { font-weight: 600; color: var(--text); }
	.sep { color: var(--text-faint); }
	.ts { color: var(--text-faint); }
	/* DEV-128 → DEV-139: 댓글 번호 — 클릭 시 본문 접기/펼치기 버튼. */
	.entry-no {
		font-family: 'SFMono-Regular', Consolas, monospace;
		font-size: 0.72rem;
		color: var(--text-faint);
		background: transparent;
		cursor: pointer;
		padding: 0.05rem 0.35rem;
		border-radius: 4px;
		border: 1px solid var(--border-muted);
	}
	.entry-no:hover { color: var(--accent); border-color: var(--accent); }
	.reply-to {
		font-family: 'SFMono-Regular', Consolas, monospace;
		font-size: 0.7rem;
		color: var(--text-muted);
		text-decoration: none;
	}
	.reply-to:hover { color: var(--accent); }
	/* DEV-129: 접힌 본문 미리보기 — 1줄 ellipsis, 클릭으로 펼침. */
	.body-collapsed {
		display: block;
		width: 100%;
		text-align: left;
		background: none;
		border: none;
		border-left: 2px solid var(--border);
		padding: 0.15rem 0 0.15rem 0.6rem;
		color: var(--text-faint);
		font-size: 0.8rem;
		font-style: italic;
		cursor: pointer;
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}
	.body-collapsed:hover { color: var(--text-muted); border-left-color: var(--accent); }
	/* DEV-139: 푸터 행 — 좌측 답글 컨트롤 / 우측 이모지. */
	.entry-foot {
		display: flex;
		align-items: center;
		justify-content: space-between;
		margin-top: 0.4rem;
		gap: 0.5rem;
	}
	.foot-left {
		display: flex;
		align-items: center;
		gap: 0.4rem;
	}
	/* 삼각형만 클릭 — 채운 삼각형 (▼/▶), 일반 글자색, 종전보다 큼. */
	.tri-btn {
		background: transparent;
		border: none;
		cursor: pointer;
		font-size: 0.85rem;
		line-height: 1;
		color: var(--text);
		padding: 0.1rem 0.2rem;
	}
	.tri-btn:hover { color: var(--accent); }
	.reply-count {
		font-size: 0.75rem;
		color: var(--text-muted);
		user-select: none;
	}
	/* '답글 쓰기' — 댓글번호 (#N) 와 같은 테두리 버튼 느낌. */
	.reply-write-btn {
		font-size: 0.72rem;
		color: var(--text-muted);
		background: transparent;
		cursor: pointer;
		padding: 0.1rem 0.5rem;
		border-radius: 4px;
		border: 1px solid var(--border-muted);
	}
	.reply-write-btn:hover { color: var(--accent); border-color: var(--accent); }
	/* 우측 — 활성 반응 pill + '+' popup (slack 스타일). */
	.foot-right {
		display: flex;
		align-items: center;
		gap: 0.25rem;
	}
	.reaction-pill {
		padding: 0.1rem 0.45rem;
		background: color-mix(in srgb, var(--accent) 12%, transparent);
		border: 1px solid color-mix(in srgb, var(--accent) 45%, transparent);
		border-radius: 10px;
		font-size: 0.78rem;
		cursor: pointer;
	}
	.reaction-pill:hover { border-color: var(--danger); }
	/* DEV-108: 내가 단 반응은 진한 테두리로 구분. */
	.reaction-pill.mine {
		background: color-mix(in srgb, var(--accent) 24%, transparent);
		border-color: var(--accent);
	}
	/* 반응 수 (2명 이상). */
	.reaction-pill .rc {
		margin-left: 0.2rem;
		font-size: 0.7rem;
		font-weight: 700;
		color: var(--text-muted);
	}
	.picker-wrap { position: relative; }
	.reaction-add {
		padding: 0.1rem 0.4rem;
		background: transparent;
		border: 1px solid var(--border-muted);
		border-radius: 10px;
		font-size: 0.72rem;
		color: var(--text-faint);
		cursor: pointer;
	}
	.reaction-add:hover { color: var(--text); border-color: var(--text-faint); }
	.picker-ov {
		position: fixed;
		inset: 0;
		z-index: 90;
		background: transparent;
	}
	.reaction-picker {
		position: absolute;
		bottom: calc(100% + 4px);
		right: 0;
		z-index: 91;
		display: flex;
		gap: 0.2rem;
		padding: 0.3rem 0.4rem;
		background: var(--bg-elevated);
		border: 1px solid var(--border);
		border-radius: 8px;
		box-shadow: 0 6px 18px var(--shadow);
	}
	.picker-item {
		padding: 0.15rem 0.35rem;
		background: transparent;
		border: 1px solid transparent;
		border-radius: 6px;
		font-size: 0.95rem;
		cursor: pointer;
	}
	.picker-item:hover { background: var(--bg-subtle); }
	.picker-item.on {
		background: color-mix(in srgb, var(--accent) 15%, transparent);
		border-color: color-mix(in srgb, var(--accent) 45%, transparent);
	}
	.entry-actions {
		margin-left: auto;
		display: flex;
		gap: 0.5rem;
	}
	.link-btn {
		background: none; border: none;
		color: var(--accent); cursor: pointer;
		padding: 0; font: inherit; font-size: 0.78rem;
		text-decoration: underline;
	}
	.link-btn:hover { color: var(--accent); }
	.link-btn.danger { color: var(--danger); }
	.link-btn.danger:hover { color: var(--danger); }
	/* DEV-142: '토론' 토글 활성 표시. */
	.link-btn.on { color: var(--warning); font-weight: 700; }

	/* DEV-142: 토론 상태 배지 — 미해결(빨강) / 해결(초록). */
	.disc-badge {
		margin-left: 0.4rem;
		padding: 0.05rem 0.4rem;
		border-radius: 999px;
		border: 1px solid color-mix(in srgb, var(--danger) 40%, transparent);
		background: color-mix(in srgb, var(--danger) 14%, transparent);
		color: var(--danger);
		font-size: 0.7rem;
		font-weight: 700;
		cursor: pointer;
		white-space: nowrap;
	}
	.disc-badge:hover { background: color-mix(in srgb, var(--danger) 22%, transparent); }
	.disc-badge.resolved {
		border-color: color-mix(in srgb, var(--success) 45%, transparent);
		background: color-mix(in srgb, var(--success) 14%, transparent);
		color: var(--success);
	}
	.disc-badge.resolved:hover { background: color-mix(in srgb, var(--success) 22%, transparent); }

	.entry-body :global(p) { margin: 0.25rem 0; }

	.new-form {
		border-top: 1px dashed var(--bg-subtle);
		padding-top: 0.75rem;
		display: flex;
		flex-direction: column;
		gap: 0.4rem;
	}
	.new-row { display: flex; gap: 0.4rem; }
	.author-input {
		flex: 0 0 14rem;
		padding: 0.3rem 0.5rem;
		background: var(--bg);
		border: 1px solid var(--border);
		color: var(--text);
		border-radius: 4px;
		font-size: 0.825rem;
	}
	.body-input {
		width: 100%;
		padding: 0.45rem 0.6rem;
		background: var(--bg);
		border: 1px solid var(--border);
		color: var(--text);
		border-radius: 6px;
		font-size: 0.825rem;
		font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
		resize: vertical;
		min-height: 4rem;
	}
	.actions { display: flex; gap: 0.4rem; margin-top: 0.35rem; }
	/* DEV-151: 댓글 첨부 버튼. */
	.btn-attach {
		padding: 0.3rem 0.7rem;
		background: var(--bg-subtle); border: 1px solid var(--border);
		color: var(--text); border-radius: 6px; cursor: pointer; font-size: 0.8rem;
		margin-right: auto;
	}
	.btn-attach:hover { background: var(--bg-elevated); }
	/* DEV-171: cross-link 자동완성 팝업 (caret 위치, VS 식). */
	.wiki-pop {
		position: fixed;
		z-index: 50;
		margin: 0;
		padding: 0.2rem;
		list-style: none;
		min-width: 14rem;
		max-width: 22rem;
		max-height: 14rem;
		overflow-y: auto;
		background: var(--bg-elevated);
		border: 1px solid var(--border);
		border-radius: 8px;
		box-shadow: 0 6px 20px rgba(0, 0, 0, 0.35);
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
	.wiki-meta {
		flex: 1;
		min-width: 0;
		font-size: 0.78rem;
		color: var(--text-muted);
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.btn-save {
		padding: 0.3rem 0.85rem;
		background: var(--btn-primary-bg); border: 1px solid var(--btn-primary-border);
		color: var(--btn-primary-text); border-radius: 6px; cursor: pointer; font-size: 0.825rem;
	}
	.btn-save:disabled { opacity: 0.5; cursor: not-allowed; }
	.btn-save:hover:not(:disabled) { background: var(--btn-primary-bg-hover); border-color: var(--btn-primary-border-hover); }
	.btn-cancel {
		padding: 0.3rem 0.85rem;
		background: transparent; border: 1px solid var(--border);
		color: var(--text); border-radius: 6px; cursor: pointer; font-size: 0.825rem;
	}
	.btn-cancel:hover:not(:disabled) { background: var(--bg-subtle); }
</style>
