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

// DEV-069 후속(admin #8): 임의 파일 첨부 허용. 미디어(이미지/동영상)는 본문에
// embed, 그 외는 다운로드 링크로 삽입. backend(save_attachment)도 화이트리스트
// 제거됨.
const IMAGE_EXTS = new Set(['png', 'jpg', 'jpeg', 'gif', 'webp', 'bmp', 'svg']);
const VIDEO_EXTS = new Set(['mp4', 'webm']);

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

/** MIME 우선, 없으면 파일명 확장자. 확장자 없으면 'bin'. (임의 파일 허용.) */
function extOf(file: File): string {
	const byMime = EXT_BY_MIME[file.type];
	if (byMime) return byMime;
	const dot = file.name.lastIndexOf('.');
	if (dot < 0) return 'bin';
	const ext = file.name
		.slice(dot + 1)
		.toLowerCase()
		.replace(/[^a-z0-9]/g, '');
	return ext || 'bin';
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

/** rel 경로 → 본문 삽입용 마크다운. 이미지는 `![]()`, 동영상은 video, 그 외 링크. */
function markdownFor(rel: string, ext: string, name: string): string {
	if (IMAGE_EXTS.has(ext)) return `![${name}](${rel})`;
	if (VIDEO_EXTS.has(ext)) {
		// marked 는 raw HTML pass-through — MarkdownView 가 video src 재작성.
		return `<video controls src="${rel}"></video>`;
	}
	// pdf 포함 기타 파일 — 다운로드 링크.
	return `[${name}](${rel})`;
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

/** FileList → 첨부 대상(임의 파일 허용). */
function allowedFiles(files: FileList | null | undefined): { file: File; ext: string }[] {
	if (!files) return [];
	return Array.from(files).map((file) => ({ file, ext: extOf(file) }));
}

// ───────────────────────── textarea (DEV-151) ─────────────────────────
// 댓글 편집기는 CodeMirror 가 아니라 <textarea> 라 위 EditorView 경로가 안 먹는다.
// textarea 용 paste/drop/버튼 첨부 — bind:value 동기화를 위해 'input' 이벤트를
// 직접 dispatch 한다.

/** 커서 위치에 text 삽입 + input 이벤트 (Svelte bind:value 갱신). */
function insertIntoTextarea(ta: HTMLTextAreaElement, text: string) {
	const start = ta.selectionStart ?? ta.value.length;
	const end = ta.selectionEnd ?? ta.value.length;
	ta.setRangeText(text, start, end, 'end');
	ta.dispatchEvent(new Event('input', { bubbles: true }));
}

/** placeholder 를 결과 마크다운으로 치환 (업로드 완료 시). */
function replaceInTextarea(ta: HTMLTextAreaElement, placeholder: string, md: string) {
	const idx = ta.value.indexOf(placeholder);
	if (idx < 0) return; // 사용자가 지웠으면 그냥 둔다.
	ta.value = ta.value.slice(0, idx) + md + ta.value.slice(idx + placeholder.length);
	ta.dispatchEvent(new Event('input', { bubbles: true }));
}

function uploadAndInsertTextarea(
	ta: HTMLTextAreaElement,
	file: File,
	ext: string,
	onError: (msg: string) => void
) {
	const placeholder = `![업로드 중… ${file.name || 'clipboard'} #${++uploadSeq}]()`;
	insertIntoTextarea(ta, placeholder);
	void (async () => {
		try {
			const dataBase64 = toBase64(await file.arrayBuffer());
			const rel = await invoke<string>('save_attachment', { dataBase64, ext });
			const md = markdownFor(rel, ext, file.name || rel.split('/').pop() || rel);
			replaceInTextarea(ta, placeholder, md);
		} catch (e) {
			replaceInTextarea(ta, placeholder, '');
			onError(typeof e === 'string' ? e : ((e as Error).message ?? String(e)));
		}
	})();
}

/**
 * Svelte action — <textarea> 에 clipboard paste / 파일 drag&drop 첨부 부착.
 * `use:textareaAttach={{ onError }}`. 브라우저(server) 모드에선 noop.
 */
export function textareaAttach(
	ta: HTMLTextAreaElement,
	opts: { onError?: (msg: string) => void } = {}
): { destroy(): void } {
	if (detectEnvironment() !== 'tauri') return { destroy() {} };
	const onError = opts.onError ?? ((m) => console.error('첨부 업로드 실패:', m));
	const onPaste = (e: ClipboardEvent) => {
		const picked = allowedFiles(e.clipboardData?.files);
		if (picked.length === 0) return; // 일반 텍스트 paste 는 기본 동작.
		e.preventDefault();
		for (const { file, ext } of picked) uploadAndInsertTextarea(ta, file, ext, onError);
	};
	const onDrop = (e: DragEvent) => {
		const picked = allowedFiles(e.dataTransfer?.files);
		if (picked.length === 0) return;
		e.preventDefault();
		ta.focus();
		for (const { file, ext } of picked) uploadAndInsertTextarea(ta, file, ext, onError);
	};
	ta.addEventListener('paste', onPaste);
	ta.addEventListener('drop', onDrop);
	return {
		destroy() {
			ta.removeEventListener('paste', onPaste);
			ta.removeEventListener('drop', onDrop);
		}
	};
}

/** '첨부' 버튼 — textarea 커서 위치에 파일(다중) 업로드/삽입. */
export function pickAndAttachTextarea(
	ta: HTMLTextAreaElement | undefined | null,
	onError: (msg: string) => void = (m) => console.error('첨부 업로드 실패:', m)
): void {
	if (!ta) return;
	if (detectEnvironment() !== 'tauri') {
		onError('첨부 업로드는 데스크탑 앱에서만 지원됩니다.');
		return;
	}
	const input = document.createElement('input');
	input.type = 'file';
	input.multiple = true;
	input.style.display = 'none';
	input.onchange = () => {
		const picked = allowedFiles(input.files);
		input.remove();
		if (picked.length === 0) return;
		ta.focus();
		for (const { file, ext } of picked) uploadAndInsertTextarea(ta, file, ext, onError);
	};
	document.body.appendChild(input);
	input.click();
}

/**
 * DEV-069 후속(admin #8): '첨부파일 추가' 버튼 핸들러. 숨은 file input 으로
 * 임의 파일(다중) 선택 → 커서 위치에 업로드/삽입. CodeMirror 편집기 전용
 * (quest/campaign 본문, 규칙). 미디어는 embed, 그 외는 링크.
 */
export function pickAndAttach(
	view: EditorView,
	onError: (msg: string) => void = (m) => console.error('첨부 업로드 실패:', m)
): void {
	if (detectEnvironment() !== 'tauri') {
		onError('첨부 업로드는 데스크탑 앱에서만 지원됩니다.');
		return;
	}
	const input = document.createElement('input');
	input.type = 'file';
	input.multiple = true;
	input.style.display = 'none';
	input.onchange = () => {
		const picked = allowedFiles(input.files);
		input.remove();
		if (picked.length === 0) return;
		view.focus();
		let pos = view.state.selection.main.head;
		for (const { file, ext } of picked) {
			pos += uploadAndInsert(view, file, ext, pos, onError);
		}
	};
	document.body.appendChild(input);
	input.click();
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
