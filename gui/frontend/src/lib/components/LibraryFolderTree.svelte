<!--
  DEV-239: 도서관 tree 모드 사이드바의 재귀 폴더 노드 — 폴더(들여쓰기 + 삭제
  버튼) 아래 하위 폴더(재귀) + 문서 목록. Svelte 컴포넌트는 자기 자신을
  import 해 재귀 렌더 가능.

  BUG-123(admin 보고): 접기 기능이 아예 없었음 — 토글 화살표 + collapsedFolders
  (부모가 Set 으로 관리) 로 하위 폴더/문서 표시 여부 제어.
-->
<script lang="ts">
	import type { FolderNode } from '$lib/utils/library-tree';
	import type { Book } from '$lib/api/library';
	import LibraryFolderTree from './LibraryFolderTree.svelte';

	let {
		node,
		depth,
		selectedId,
		collapsedFolders,
		onSelectDoc,
		onDeleteFolder,
		onToggleCollapse
	}: {
		node: FolderNode;
		depth: number;
		selectedId: string | null;
		collapsedFolders: Set<string>;
		onSelectDoc: (id: string) => void;
		onDeleteFolder: (path: string) => void;
		onToggleCollapse: (path: string) => void;
	} = $props();

	const collapsed = $derived(collapsedFolders.has(node.path));
	const hasChildren = $derived(node.children.length > 0 || node.docs.length > 0);
</script>

<div class="folder-row" style:padding-left={`${depth * 14}px`}>
	<button
		class="folder-toggle"
		class:invisible={!hasChildren}
		disabled={!hasChildren}
		onclick={() => onToggleCollapse(node.path)}
		aria-expanded={!collapsed}
		aria-label={collapsed ? '폴더 펼치기' : '폴더 접기'}
	>
		{collapsed ? '▶' : '▼'}
	</button>
	<span class="folder-icon" aria-hidden="true">📁</span>
	<span class="folder-name">{node.name}</span>
	<button
		class="folder-del"
		title="폴더 삭제 (비어 있을 때만)"
		onclick={() => onDeleteFolder(node.path)}
	>
		✕
	</button>
</div>

{#if !collapsed}
	{#each node.children as child (child.path)}
		<LibraryFolderTree
			node={child}
			depth={depth + 1}
			{selectedId}
			{collapsedFolders}
			{onSelectDoc}
			{onDeleteFolder}
			{onToggleCollapse}
		/>
	{/each}

	{#each node.docs as b (b.book_id)}
		<button
			class="book-item"
			class:active={b.book_id === selectedId}
			style:padding-left={`${(depth + 1) * 14 + 8}px`}
			onclick={() => onSelectDoc(b.book_id)}
		>
			<span class="book-id">{b.book_id}</span>
			<span class="book-title">{b.title}</span>
		</button>
	{/each}
{/if}

<style>
	.folder-row {
		display: flex;
		align-items: center;
		gap: 0.3rem;
		padding-top: 0.3rem;
		padding-bottom: 0.15rem;
		color: var(--text-muted);
		font-size: 0.78rem;
	}
	.folder-toggle {
		background: transparent;
		border: none;
		color: var(--text-muted);
		cursor: pointer;
		font-size: 0.6rem;
		line-height: 1;
		padding: 0.15rem;
		width: 1rem;
		text-align: center;
		flex-shrink: 0;
	}
	.folder-toggle:hover:not(:disabled) {
		color: var(--accent);
	}
	.folder-toggle.invisible {
		visibility: hidden;
		cursor: default;
	}
	.folder-icon {
		font-size: 0.8rem;
	}
	.folder-name {
		flex: 1;
		min-width: 0;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.folder-del {
		background: transparent;
		border: none;
		color: var(--text-muted);
		cursor: pointer;
		font-size: 0.7rem;
		padding: 0 0.2rem;
		opacity: 0.5;
	}
	.folder-del:hover {
		opacity: 1;
		color: var(--danger);
	}
	.book-item {
		width: 100%;
		text-align: left;
		padding-top: 0.35rem;
		padding-bottom: 0.35rem;
		padding-right: 0.5rem;
		background: transparent;
		border: none;
		border-radius: 4px;
		color: var(--text);
		font-size: 0.85rem;
		cursor: pointer;
		display: flex;
		flex-direction: column;
		gap: 0.1rem;
	}
	.book-item:hover {
		background: var(--bg-elevated);
	}
	.book-item.active {
		background: color-mix(in srgb, var(--accent) 12%, transparent);
	}
	.book-id {
		font-family: 'SFMono-Regular', Consolas, monospace;
		font-size: 0.72rem;
		color: var(--accent);
	}
	.book-title {
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
</style>
