/**
 * DEV-069: 본문 편집기 첨부 업로드 — CodeMirror Extension.
 *
 * - 클립보드 이미지 Ctrl+V (사용자 댓글 #2)
 * - 파일 드래그&드랍 (사용자 댓글 #4)
 *
 * 흐름: 파일 bytes → base64 → Tauri `save_attachment` invoke →
 * `.guild/attachments/{name}` 저장 (+ index.db blob 백업, 댓글 #3) →
 * 반환된 상대 경로를 마크다운으로 커서 위치에 삽입.
 * MarkdownView 가 `attachments/...` src 를 asset URL 로 재작성해 표시.
 *
 * Tauri 전용 — 브라우저 (server) 모드 업로드는 DEV-097 범위.
 */

import { EditorView } from '@codemirror/view';
import type { Extension } from '@codemirror/state';
import { invoke } from '@tauri-apps/api/core';
import { detectEnvironment } from '$lib/api/transport';

/** core ALLOWED_EXTS 와 동기 (core/src/ops/attachments.rs). */
const ALLOWED_EXTS = new Set([
	'png', 'jpg', 'jpeg', 'gif', 'webp', 'bmp', 'svg', 'mp4', 'webm', 'pdf'
]);

const EXT_BY_MIME: Record<string, string> = {
	'image/png': 'png',
	'image/jpeg': 'jpg',
	'image/gif': 'gif',
	'image/webp': 'webp',
	'image/bmp': 'bmp',
	'image/svg+xml': 'svg',
	'video/mp4': 'mp4',
	'video/webm': 'webm',
	'application/pdf': 'pdf'
};

/** MIME 우선, 없으면 파일명 확장자. 허용 목록 밖이면 null. */
function extOf(file: File): string | null {
	const byMime = EXT_BY_MIME[file.type];
	if (byMime) return byMime;
	const dot = file.name.lastIndexOf('.');
	if (dot < 0) return null;
	const ext = file.name.slice(dot + 1).toLowerCase();
	return ALLOWED_EXTS.has(ext) ? ext : null;
}

/** ArrayBuffer → base64 — 큰 파일도 안전하게 청크 단위 변환. */
function toBase64(buf: ArrayBuffer): string {
	const bytes = new Uint8Array(buf);
	let bin = '';
	const CHUNK = 0x8000;
	for (let i = 0; i < bytes.length; i += CHUNK) {
		bin += String.fromCharCode(...bytes.subarray(i, i + CHUNK));
	}
	return btoa(bin);
}

/** rel 경로 → 본문 삽입용 마크다운. 이미지는 `![]()`, 그 외는 링크. */
function markdownFor(rel: string, ext: string, name: string): string {
	if (ext === 'mp4' || ext === 'webm') {
		// marked 는 raw HTML pass-through — MarkdownView 가 video src 재작성.
		return `<video controls src="${rel}"></video>`;
	}
	if (ext === 'pdf') return `[${name}](${rel})`;
	return `![${name}](${rel})`;
}

let uploadSeq = 0;

/**
 * placeholder 를 pos 에 즉시 삽입 (동기) 후 업로드 — 완료 시 placeholder 를
 * 결과 마크다운으로 치환 (업로드 중 사용자가 편집해도 위치 안전).
 * 반환: 삽입한 placeholder 길이 (다음 파일의 삽입 위치 계산용).
 */
function uploadAndInsert(
	view: EditorView,
	file: File,
	ext: string,
	pos: number,
	onError: (msg: string) => void
): number {
	// 고유 번호 — 같은 파일명 여러 개 동시 업로드 시 치환 대상 구분.
	const placeholder = `![업로드 중… ${file.name || 'clipboard'} #${++uploadSeq}]()`;
	view.dispatch({ changes: { from: pos, insert: placeholder } });
	void (async () => {
		try {
			const dataBase64 = toBase64(await file.arrayBuffer());
			const rel = await invoke<string>('save_attachment', { dataBase64, ext });
			const md = markdownFor(rel, ext, file.name || rel.split('/').pop() || rel);
			replacePlaceholder(view, placeholder, md);
		} catch (e) {
			replacePlaceholder(view, placeholder, '');
			onError(typeof e === 'string' ? e : ((e as Error).message ?? String(e)));
		}
	})();
	return placeholder.length;
}

function replacePlaceholder(view: EditorView, placeholder: string, md: string) {
	const doc = view.state.doc.toString();
	const idx = doc.indexOf(placeholder);
	if (idx < 0) return; // 사용자가 지웠으면 그냥 둔다.
	view.dispatch({ changes: { from: idx, to: idx + placeholder.length, insert: md } });
}

/** FileList / DataTransferItemList 에서 허용 파일만 추출. */
function allowedFiles(files: FileList | null | undefined): { file: File; ext: string }[] {
	if (!files) return [];
	const out: { file: File; ext: string }[] = [];
	for (const file of files) {
		const ext = extOf(file);
		if (ext) out.push({ file, ext });
	}
	return out;
}

/**
 * 편집기 extension 생성 — quest / campaign 상세 initEditor 의 extensions 에 추가.
 * 브라우저 (server) 모드에선 빈 extension (업로드 미지원, DEV-097).
 */
export function attachmentExtension(
	onError: (msg: string) => void = (m) => console.error('첨부 업로드 실패:', m)
): Extension {
	if (detectEnvironment() !== 'tauri') return [];
	return EditorView.domEventHandlers({
		paste(event, view) {
			const picked = allowedFiles(event.clipboardData?.files);
			if (picked.length === 0) return false; // 일반 텍스트 paste 는 기본 동작.
			event.preventDefault();
			let pos = view.state.selection.main.head;
			for (const { file, ext } of picked) {
				pos += uploadAndInsert(view, file, ext, pos, onError);
			}
			return true;
		},
		drop(event, view) {
			const picked = allowedFiles(event.dataTransfer?.files);
			if (picked.length === 0) return false;
			event.preventDefault();
			let pos =
				view.posAtCoords({ x: event.clientX, y: event.clientY }) ??
				view.state.selection.main.head;
			for (const { file, ext } of picked) {
				pos += uploadAndInsert(view, file, ext, pos, onError);
			}
			return true;
		}
	});
}
