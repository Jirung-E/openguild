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

export function buildLibraryTree(
	folders: LibraryFolder[],
	books: Book[],
	// DEV-251: 호출측이 정렬 기준(번호/이름/수정순)을 고르면 books 를 미리
	// 정렬해 넘기고 이 옵션으로 내부 title 재정렬을 끈다. 기본은 기존 동작
	// (title localeCompare) — 다른 호출처/테스트 호환.
	opts: { preserveDocOrder?: boolean } = {}
): LibraryTree {
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
		if (!opts.preserveDocOrder) {
			n.docs.sort((a, b) => a.title.localeCompare(b.title));
		}
		n.children.forEach(sortNode);
	}
	const roots = [...nodeMap.values()].filter((n) => !n.path.includes('/'));
	roots.sort((a, b) => a.name.localeCompare(b.name));
	roots.forEach(sortNode);
	if (!opts.preserveDocOrder) {
		rootDocs.sort((a, b) => a.title.localeCompare(b.title));
	}

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
/**
 * REQ-011: `attachIds` 는 **첨부 파일 이름**이 질의에 맞은 book_id 집합.
 * 첨부 이름은 목록 응답(`list()`)에 없어 클라이언트가 알 수 없으므로 서버가
 * 판정해 넘긴다. 본문/제목 매치와 **OR** 로 합친다.
 */
export function searchBooks(
	books: Book[],
	query: string,
	attachIds?: Set<string>
): Book[] | null {
	const q = query.trim().toLowerCase();
	if (!q) return null;
	const titleHits: Book[] = [];
	const bodyOnlyHits: Book[] = [];
	for (const b of books) {
		const titleHit = b.title.toLowerCase().includes(q);
		const bodyHit = !titleHit && b.body.toLowerCase().includes(q);
		const attachHit = !titleHit && !bodyHit && (attachIds?.has(b.book_id) ?? false);
		if (titleHit) titleHits.push(b);
		else if (bodyHit || attachHit) bodyOnlyHits.push(b);
	}
	titleHits.sort((a, b) => a.title.localeCompare(b.title));
	bodyOnlyHits.sort((a, b) => a.title.localeCompare(b.title));
	return [...titleHits, ...bodyOnlyHits];
}

export interface LibrarySearchResult {
	folders: FolderNode[];
	books: Book[];
}

/**
 * BUG-128(admin 보고): DEV-238 검색은 (1) 항상 전역이었고 (2) 폴더 이름은
 * 대상이 아니었음. `scope` 로 검색 범위를 그 폴더 자신 + 하위로 제한하고
 * (""=전체), 폴더 name 매칭도 결과에 포함.
 *
 * BUG-128 후속(admin 보고): 문서는 scope 폴더 "자신"도 검색 대상이 맞지만
 * (그 폴더 바로 밑 문서), 폴더 이름 매칭에서 scope 폴더 자신까지 걸리면
 * 이미 그 안에 들어와서 검색하는 중인데 자기 자신이 결과로 뜨는 게
 * 무의미하다 — 폴더 매칭은 scope 의 "하위"만(자신 제외).
 */
export function searchLibrary(
	tree: LibraryTree,
	books: Book[],
	query: string,
	scope = '',
	/** REQ-011: 첨부 이름이 맞은 book_id 집합(서버 판정). */
	attachIds?: Set<string>
): LibrarySearchResult | null {
	const q = query.trim().toLowerCase();
	if (!q) return null;
	const inScope = (path: string) => !scope || path === scope || path.startsWith(`${scope}/`);
	const inScopeDescendant = (path: string) => path !== scope && inScope(path);

	const folders = [...tree.nodeMap.values()]
		.filter((n) => inScopeDescendant(n.path) && n.name.toLowerCase().includes(q))
		.sort((a, b) => a.path.localeCompare(b.path));

	const books_ = searchBooks(
		books.filter((b) => inScope(b.path)),
		query,
		attachIds
	);

	return { folders, books: books_ ?? [] };
}
