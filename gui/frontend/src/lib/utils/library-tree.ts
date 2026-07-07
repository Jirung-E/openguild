// DEV-239: 도서관 폴더 트리 빌더 — folders(빈 폴더 포함) + books(path 필드)를
// 조합해 계층 구조로 만든다. 순수 함수 — tree 모드/explorer 모드 둘 다 사용.

import type { Book, LibraryFolder } from '$lib/api/library';

export interface FolderNode {
	path: string;
	name: string;
	children: FolderNode[];
	docs: Book[];
}

export interface LibraryTree {
	roots: FolderNode[];
	rootDocs: Book[];
	/** path → node — explorer 모드의 현재 위치 조회용. */
	nodeMap: Map<string, FolderNode>;
}

export function buildLibraryTree(folders: LibraryFolder[], books: Book[]): LibraryTree {
	const nodeMap = new Map<string, FolderNode>();

	function ensure(path: string): FolderNode {
		const existing = nodeMap.get(path);
		if (existing) return existing;
		const name = path.slice(path.lastIndexOf('/') + 1);
		const node: FolderNode = { path, name, children: [], docs: [] };
		nodeMap.set(path, node);
		const slash = path.lastIndexOf('/');
		if (slash >= 0) {
			const parent = ensure(path.slice(0, slash));
			parent.children.push(node);
		}
		return node;
	}

	for (const f of folders) ensure(f.path);

	const rootDocs: Book[] = [];
	for (const b of books) {
		if (!b.path) {
			rootDocs.push(b);
			continue;
		}
		ensure(b.path).docs.push(b);
	}

	function sortNode(n: FolderNode) {
		n.children.sort((a, b) => a.name.localeCompare(b.name));
		n.docs.sort((a, b) => a.title.localeCompare(b.title));
		n.children.forEach(sortNode);
	}
	const roots = [...nodeMap.values()].filter((n) => !n.path.includes('/'));
	roots.sort((a, b) => a.name.localeCompare(b.name));
	roots.forEach(sortNode);
	rootDocs.sort((a, b) => a.title.localeCompare(b.title));

	return { roots, rootDocs, nodeMap };
}

/** 폴더 select용 평탄화 목록 (path 순 — depth 표시는 호출측이 들여쓰기로). */
export function flattenFolderPaths(tree: LibraryTree): string[] {
	return [...tree.nodeMap.keys()].sort((a, b) => a.localeCompare(b));
}

/**
 * DEV-238: 도서관 검색 — 제목/본문 부분일치, 클라이언트 필터링(문서 수가
 * 크지 않다는 전제 — 커지면 서버 LIKE/FTS5 로 옮길 수 있음).
 *
 * 빈 쿼리는 `null`(검색 비활성 — 호출측이 평소 폴더 구조 렌더와 구분).
 * 제목 매칭이 본문만 매칭보다 우선 노출, 각 그룹 안은 제목 가나다순.
 */
export function searchBooks(books: Book[], query: string): Book[] | null {
	const q = query.trim().toLowerCase();
	if (!q) return null;
	const titleHits: Book[] = [];
	const bodyOnlyHits: Book[] = [];
	for (const b of books) {
		const titleHit = b.title.toLowerCase().includes(q);
		const bodyHit = !titleHit && b.body.toLowerCase().includes(q);
		if (titleHit) titleHits.push(b);
		else if (bodyHit) bodyOnlyHits.push(b);
	}
	titleHits.sort((a, b) => a.title.localeCompare(b.title));
	bodyOnlyHits.sort((a, b) => a.title.localeCompare(b.title));
	return [...titleHits, ...bodyOnlyHits];
}
