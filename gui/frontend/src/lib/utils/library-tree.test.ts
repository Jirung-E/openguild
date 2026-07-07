import { describe, it, expect } from 'vitest';
import { buildLibraryTree, flattenFolderPaths } from './library-tree';
import type { Book, LibraryFolder } from '$lib/api/library';

function book(id: string, title: string, path: string): Book {
	return {
		book_id: id,
		number: Number(id.split('-')[1]),
		title,
		body: '',
		path,
		created_at: '',
		updated_at: '',
		deleted_at: null
	};
}
function folder(path: string): LibraryFolder {
	return { path, created_at: '', updated_at: '' };
}

describe('buildLibraryTree', () => {
	it('root 문서와 폴더를 분리하고 빈 폴더도 트리에 포함', () => {
		const tree = buildLibraryTree(
			[folder('아키텍처'), folder('운영')],
			[book('BOOK-001', '온보딩', ''), book('BOOK-002', '라우터 설계', '아키텍처')]
		);
		expect(tree.rootDocs.map((b) => b.book_id)).toEqual(['BOOK-001']);
		expect(tree.roots.map((n) => n.path).sort()).toEqual(['아키텍처', '운영']);
		const arch = tree.roots.find((n) => n.path === '아키텍처')!;
		expect(arch.docs.map((b) => b.book_id)).toEqual(['BOOK-002']);
		const ops = tree.roots.find((n) => n.path === '운영')!;
		expect(ops.docs).toEqual([]);
	});

	it('명시적 폴더 등록 없이 문서 path 만으로도 폴더 노드가 암묵적으로 생김', () => {
		const tree = buildLibraryTree([], [book('BOOK-001', '가이드', '아키텍처/서브')]);
		const arch = tree.roots.find((n) => n.path === '아키텍처')!;
		expect(arch).toBeDefined();
		expect(arch.children.map((n) => n.path)).toEqual(['아키텍처/서브']);
		expect(arch.children[0].docs.map((b) => b.book_id)).toEqual(['BOOK-001']);
	});

	it('flattenFolderPaths 는 정렬된 전체 경로 목록', () => {
		const tree = buildLibraryTree([folder('운영'), folder('아키텍처')], []);
		expect(flattenFolderPaths(tree)).toEqual(['아키텍처', '운영']);
	});
});
