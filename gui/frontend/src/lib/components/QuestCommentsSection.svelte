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
	import MarkdownView from './MarkdownView.svelte';
	import { commentsApi, type CommentEntry } from '$lib/api/comments';

	let { slug }: { slug: string } = $props();

	let loading = $state(true);
	let loadError = $state<string | null>(null);
	let entries = $state<CommentEntry[]>([]);

	// DEV-107: 섹션 접기. 기본 펼침, 사용자 선택 localStorage 영속 (전역 선호).
	const COLLAPSE_KEY = 'openguild.commentsSectionCollapsed';
	function loadCollapsed(): boolean {
		try {
			return localStorage.getItem(COLLAPSE_KEY) === 'true';
		} catch {
			return false;
		}
	}
	let collapsed = $state(loadCollapsed());
	function toggleCollapsed() {
		collapsed = !collapsed;
		try {
			localStorage.setItem(COLLAPSE_KEY, String(collapsed));
		} catch {
			/* 무시 */
		}
	}

	// 신규 top-level 작성 폼
	let newAuthor = $state('');
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
	let replyAuthor = $state('');
	let replyBody = $state('');
	let replySaving = $state(false);
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

	async function remove(id: number) {
		if (!confirm('이 댓글을 삭제할까요? (답글이 있다면 그대로 남고 안내가 표시됩니다)')) return;
		try {
			await commentsApi.deleteComment(slug, id);
			entries = entries.filter((e) => e.id !== id);
		} catch (e) {
			alert(e instanceof Error ? e.message : 'delete failed');
		}
	}

	function enterReply(parentId: number) {
		replyingTo = parentId;
		replyBody = '';
		replyError = null;
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
	<li class="entry" class:reply={isReply}>
		<div class="entry-head">
			<span class="author">{e.author || '(이름 없음)'}</span>
			<span class="sep">·</span>
			<time class="ts" datetime={e.ts}>{formatTs(e.ts)}</time>
			{#if editingId !== e.id}
				<div class="entry-actions">
					{#if !isReply}
						<button class="link-btn" onclick={() => enterReply(e.id)}>↩ 답글</button>
					{/if}
					<button class="link-btn" onclick={() => enterEdit(e)}>✎ 편집</button>
					<button class="link-btn danger" onclick={() => remove(e.id)}>× 삭제</button>
				</div>
			{/if}
		</div>
		{#if editingId === e.id}
			<textarea
				class="body-input"
				bind:value={editBody}
				rows="4"
				placeholder="본문 (markdown)"
			></textarea>
			{#if editError}<p class="state err">{editError}</p>{/if}
			<div class="actions">
				<button class="btn-save" onclick={() => saveEdit(e.id)} disabled={editSaving}>
					{editSaving ? '저장…' : '저장'}
				</button>
				<button class="btn-cancel" onclick={cancelEdit} disabled={editSaving}>취소</button>
			</div>
		{:else}
			<div class="entry-body">
				<MarkdownView source={e.body} />
			</div>
		{/if}
	</li>
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
					{@render entryView(root, false)}
					{#if (groups.childrenByRoot.get(root.id) ?? []).length > 0 || replyingTo === root.id}
						<li class="thread">
							<ul class="reply-list">
								{#each groups.childrenByRoot.get(root.id) ?? [] as r (r.id)}
									{@render entryView(r, true)}
								{/each}
								{#if replyingTo === root.id}
									<li class="reply-form">
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
											class="body-input"
											bind:value={replyBody}
											rows="3"
											placeholder={`@${root.author || root.id} 에 답글…`}
											disabled={replySaving}
										></textarea>
										{#if replyError}<p class="state err">{replyError}</p>{/if}
										<div class="actions">
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
									</li>
								{/if}
							</ul>
						</li>
					{/if}
				{/each}
				{#if groups.orphans.length > 0}
					<li class="orphan-head">
						<span class="orphan-label">↩ 삭제된 댓글에 대한 답글</span>
					</li>
					{#each groups.orphans as o (o.id)}
						{@render entryView(o, true)}
					{/each}
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
				class="body-input"
				bind:value={newBody}
				rows="3"
				placeholder="댓글 작성 (markdown 사용 가능)"
				disabled={saving}
			></textarea>
			{#if saveError}<p class="state err">{saveError}</p>{/if}
			<div class="actions">
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
	.entry {
		border: 1px solid var(--bg-subtle);
		border-radius: 6px;
		background: var(--bg);
		padding: 0.6rem 0.85rem;
	}
	.entry.reply {
		background: var(--bg);
	}
	.thread {
		list-style: none;
		margin: 0;
		padding: 0;
	}
	.reply-list {
		list-style: none;
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

	.orphan-head {
		list-style: none;
		margin-top: 0.5rem;
	}
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
