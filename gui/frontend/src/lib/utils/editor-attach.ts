/**
 * DEV-069: 본문 편집기 첨부 업로드 — CodeMirror Extension.
 *
 * - 클립보드 이미지 Ctrl+V (사용자 댓글 #2)
 * - 파일 드래그&드랍 (사용자 댓글 #4)
 *
 * 흐름: 파일 bytes → base64 → `POST /api/attachments` (DEV-152: Tauri 모드는
 * transport.ts 가 invoke 로, 브라우저 모드는 HTTP 로 — 호출부는 동일 코드) →
 * `.guild/attachments/{name}` 저장 (+ index.db blob 백업, 댓글 #3) →
 * 반환된 상대 경로를 마크다운으로 커서 위치에 삽입.
 * MarkdownView 가 `attachments/...` src 를 asset URL 로 재작성해 표시.
 *
 * DEV-152: Tauri 데스크탑 + 브라우저(server) 모드 둘 다 지원.
 */

import { EditorView } from '@codemirror/view';
import type { Extension } from '@codemirror/state';
import { api } from '$lib/api/client';
import { detectEnvironment } from '$lib/api/transport';
import { getRemoteServerUrl } from '$lib/stores/remoteServer';
import { get } from 'svelte/store';
import { locale, t } from '$lib/stores/locale';

/**
 * BUG-168: bytes 경로(붙여넣기·드래그&드랍·브라우저 파일선택)의 원본 크기 상한.
 *
 * server `routes/attachments.rs` 의 `MAX_ATTACHMENT_BYTES` 와 같은 값이어야
 * 한다 — 여기서 미리 걸러 axum 원문 413("Failed to buffer the request body")이
 * 노출되는 걸 막는다. 값을 바꿀 땐 양쪽을 같이 바꿀 것.
 *
 * Tauri 데스크탑의 파일선택은 경로 기반(`uploadAttachmentPath`)이라 이 상한과
 * 무관하다 — IPC 로 bytes 를 보내지 않기 때문.
 */
export const MAX_ATTACHMENT_BYTES = 64 * 1024 * 1024;

/** 로컬 Tauri(원격 연결 아님) — 경로 기반 업로드가 가능한 환경. */
function isLocalTauri(): boolean {
	return detectEnvironment() === 'tauri' && !getRemoteServerUrl();
}

function tooLargeMessage(file: File): string {
	const mb = (n: number) => Math.round((n / (1024 * 1024)) * 10) / 10;
	const loc = get(locale);
	return `${t('attach.tooLarge', loc)} (${mb(file.size)} MB / max ${mb(MAX_ATTACHMENT_BYTES)} MB)`;
}

// DEV-069 후속(admin #8): 임의 파일 첨부 허용. 미디어(이미지/동영상)는 본문에
// embed, 그 외는 다운로드 링크로 삽입. backend(save_attachment)도 화이트리스트
// 제거됨.
const IMAGE_EXTS = new Set(['png', 'jpg', 'jpeg', 'gif', 'webp', 'bmp', 'svg']);
const VIDEO_EXTS = new Set(['mp4', 'webm']);

// BUG-083: 첨부 섹션이 없는 편집기(메모·규칙)는 본문 인라인으로만 넣을 수 있는
// 미디어(이미지/동영상)만 허용 — 비미디어는 클릭해도 안 열리는 죽은 링크가 되므로
// 차단 + 안내.
const MEDIA_ONLY_MSG =
	'이미지·동영상만 첨부할 수 있습니다. 다른 파일은 quest/campaign 의 첨부 섹션을 이용하세요.';

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

/** 파일명 확장자. 없으면 'bin'. (임의 파일 허용.) */
function extOfName(name: string): string {
	const dot = name.lastIndexOf('.');
	if (dot < 0) return 'bin';
	const ext = name
		.slice(dot + 1)
		.toLowerCase()
		.replace(/[^a-z0-9]/g, '');
	return ext || 'bin';
}

/** MIME 우선, 없으면 파일명 확장자. 확장자 없으면 'bin'. (임의 파일 허용.) */
function extOf(file: File): string {
	return EXT_BY_MIME[file.type] ?? extOfName(file.name);
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

/**
 * DEV-152: 첨부 bytes 저장 — Tauri/브라우저 공통 진입점. `api.post` 가
 * transport.ts 를 거쳐 Tauri 면 invoke('save_attachment', ...), 브라우저면
 * `POST /api/attachments` 로 자동 분기(호출부는 환경 무지).
 */
async function saveAttachmentBytes(file: File, ext: string): Promise<string> {
	// BUG-168: 한도를 넘으면 base64 변환(파일 크기의 5~6배 메모리)조차 하지 않고
	// 바로 안내한다 — 서버까지 보내면 axum 원문 413 이 그대로 노출된다.
	if (file.size > MAX_ATTACHMENT_BYTES) throw new Error(tooLargeMessage(file));
	const data_base64 = toBase64(await file.arrayBuffer());
	// HTTP body 필드는 이 프로젝트 컨벤션상 snake_case(server 의 axum Deserialize
	// 와 1:1) — transport.ts 의 routeToInvoke 가 Tauri invoke 용 camelCase args 로 변환.
	return api.post<string>('/api/attachments', { data_base64, ext });
}

/**
 * BUG-168: 업로드 1건 — "무엇을 어떻게 올릴지"를 감싼 단위.
 *
 * bytes(붙여넣기·드래그&드랍·브라우저 파일선택)와 경로(로컬 Tauri 파일선택)는
 * 전송 방식이 전혀 다른데 삽입/치환 로직은 같다. 호출부가 분기하지 않도록
 * `run()` 뒤로 숨긴다 — 반환값은 양쪽 모두 `.guild` 상대 경로.
 */
type Upload = { name: string; ext: string; run: () => Promise<string> };

function uploadFromFile(file: File): Upload {
	const ext = extOf(file);
	return {
		name: file.name || 'clipboard',
		ext,
		run: () => saveAttachmentBytes(file, ext)
	};
}

function uploadFromPath(path: string): Upload {
	const name = path.split(/[\\/]/).pop() || path;
	return {
		name,
		ext: extOfName(name),
		run: async () => (await uploadAttachmentPath(path)).rel
	};
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
	up: Upload,
	pos: number,
	onError: (msg: string) => void
): number {
	// 고유 번호 — 같은 파일명 여러 개 동시 업로드 시 치환 대상 구분.
	const placeholder = `![${t('attach.uploading', get(locale))} ${up.name} #${++uploadSeq}]()`;
	view.dispatch({ changes: { from: pos, insert: placeholder } });
	void (async () => {
		try {
			const rel = await up.run();
			const md = markdownFor(rel, up.ext, up.name || rel.split('/').pop() || rel);
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
function allowedFiles(files: FileList | null | undefined): Upload[] {
	if (!files) return [];
	return Array.from(files).map(uploadFromFile);
}

/**
 * BUG-168: 환경에 맞는 파일 선택 — 로컬 Tauri 는 네이티브 다이얼로그로 경로를
 * 받아 경로 기반(크기 무관), 그 외는 숨은 file input 으로 bytes 기반.
 *
 * `no-native-dialogs` 규칙은 확인/경고 다이얼로그에 한정이고 파일 선택은
 * 예외로 명시돼 있다.
 */
async function pickUploads(): Promise<Upload[]> {
	if (isLocalTauri()) {
		const { open } = await import('@tauri-apps/plugin-dialog');
		const picked = await open({ multiple: true, title: t('attach.pickFile', get(locale)) });
		const paths = Array.isArray(picked) ? picked : picked ? [picked] : [];
		return paths.map(uploadFromPath);
	}
	return (await pickFilesViaInput()).map(uploadFromFile);
}

function isMedia(ext: string): boolean {
	return IMAGE_EXTS.has(ext) || VIDEO_EXTS.has(ext);
}

/**
 * BUG-168: 로컬 파일 **경로**로 첨부 저장 — 로컬 Tauri 전용.
 *
 * bytes 를 IPC 로 보내지 않으므로 파일 크기와 무관하게 payload 가 상수다
 * (base64 경로는 파일 크기의 5~6배 메모리를 JS/Rust 양쪽에 동시에 잡는다).
 * bytes 경로(`saveAttachmentBytes`)와 같은 `.guild` 상대 경로를 반환한다.
 */
async function uploadAttachmentPath(path: string): Promise<{ rel: string; name: string }> {
	const { invoke } = await import('@tauri-apps/api/core');
	const rel = await invoke<string>('save_attachment_from_path', { path });
	const name = path.split(/[\\/]/).pop() || rel;
	return { rel, name };
}

/**
 * BUG-168: '첨부 추가' 공통 파일 선택 — 환경에 맞는 경로를 자동 선택한다.
 *
 * - 로컬 Tauri: 네이티브 파일 다이얼로그로 **경로**를 받아 경로 기반 업로드
 *   (크기 제한 없음, 빠름). `no-native-dialogs` 규칙은 확인/경고 다이얼로그에
 *   한정이고 파일 선택은 예외로 명시돼 있다.
 * - 브라우저 / Tauri+원격: 숨은 `<input type=file>` → bytes 업로드.
 *
 * 각 파일이 저장될 때마다 `onOne` 이 호출된다.
 *
 * DEV-298: 대용량 파일은 저장이 끝날 때까지 아무 반응이 없어 "진행 중인지
 * 멈춘 건지" 알 수 없었다. 파일 하나를 시작할 때마다 `onProgress` 로 현재
 * 파일명과 순번을 알리고, 전부 끝나면 `null` 을 준다(표시 해제 신호).
 */
export async function pickAndUploadAttachments(handlers: {
	onOne: (r: { rel: string; name: string }) => Promise<void> | void;
	onError: (msg: string) => void;
	onProgress?: (p: AttachProgress | null) => void;
}): Promise<void> {
	const { onOne, onError, onProgress } = handlers;
	const picked = await pickUploads();
	try {
		for (let i = 0; i < picked.length; i++) {
			const up = picked[i];
			onProgress?.({ name: up.name, index: i + 1, total: picked.length });
			try {
				const rel = await up.run();
				await onOne({ rel, name: up.name || rel.split('/').pop() || rel });
			} catch (e) {
				onError(e instanceof Error ? e.message : String(e));
			}
		}
	} finally {
		onProgress?.(null);
	}
}

/** DEV-298: 업로드 진행 상태 — 현재 파일명 + 몇 번째/전체 몇 개. */
export type AttachProgress = { name: string; index: number; total: number };

/** 숨은 file input 으로 File 목록 받기 — 취소 시 빈 배열. */
function pickFilesViaInput(): Promise<File[]> {
	return new Promise((resolve) => {
		const input = document.createElement('input');
		input.type = 'file';
		input.multiple = true;
		input.style.display = 'none';
		// 취소를 눌러도 promise 가 남지 않도록 cancel 도 함께 처리.
		input.oncancel = () => {
			input.remove();
			resolve([]);
		};
		input.onchange = () => {
			const files = Array.from(input.files ?? []);
			input.remove();
			resolve(files);
		};
		document.body.appendChild(input);
		input.click();
	});
}

/**
 * DEV-156: 본문 인라인 대신 '첨부 섹션'으로 보낼 때 — 업로드만 하고 (rel, name) 을
 * onAttach 콜백에 전달. 콜백이 Tauri add_*_attachment 커맨드 호출 + 목록 갱신.
 */
function uploadToSection(
	up: Upload,
	onAttach: (rel: string, name: string) => void,
	onError: (msg: string) => void
): void {
	void (async () => {
		try {
			const rel = await up.run();
			onAttach(rel, up.name || rel.split('/').pop() || rel);
		} catch (e) {
			onError(typeof e === 'string' ? e : ((e as Error).message ?? String(e)));
		}
	})();
}

// ───────────────────────── textarea (DEV-151) ─────────────────────────
// 댓글 편집기는 CodeMirror 가 아니라 <textarea> 라 위 EditorView 경로가 안 먹는다.
// textarea 용 paste/drop/버튼 첨부 — bind:value 동기화를 위해 'input' 이벤트를
// 직접 dispatch 한다.

/**
 * 커서 위치에 text 삽입. BUG-083: `execCommand('insertText')` 로 삽입해 네이티브
 * undo 스택을 보존(Ctrl+Z 가능) — setRangeText 는 undo 불가라 미지원 환경 fallback
 * 으로만. execCommand 는 'input' 이벤트를 자동 발생시켜 Svelte bind:value 도 갱신.
 */
function insertIntoTextarea(ta: HTMLTextAreaElement, text: string) {
	const start = ta.selectionStart ?? ta.value.length;
	const end = ta.selectionEnd ?? ta.value.length;
	ta.focus();
	ta.setSelectionRange(start, end);
	let ok = false;
	try {
		ok = document.execCommand('insertText', false, text);
	} catch {
		ok = false;
	}
	if (!ok) {
		ta.setRangeText(text, start, end, 'end');
		ta.dispatchEvent(new Event('input', { bubbles: true }));
	}
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
	up: Upload,
	onError: (msg: string) => void
) {
	const placeholder = `![${t('attach.uploading', get(locale))} ${up.name} #${++uploadSeq}]()`;
	insertIntoTextarea(ta, placeholder);
	void (async () => {
		try {
			const rel = await up.run();
			const md = markdownFor(rel, up.ext, up.name || rel.split('/').pop() || rel);
			replaceInTextarea(ta, placeholder, md);
		} catch (e) {
			replaceInTextarea(ta, placeholder, '');
			onError(typeof e === 'string' ? e : ((e as Error).message ?? String(e)));
		}
	})();
}

/**
 * Svelte action — <textarea> 에 clipboard paste / 파일 drag&drop 첨부 부착.
 * `use:textareaAttach={{ onError }}`. DEV-152: Tauri + 브라우저 모두 지원.
 */
export function textareaAttach(
	ta: HTMLTextAreaElement,
	opts: {
		onError?: (msg: string) => void;
		onAttach?: (rel: string, name: string) => void;
		mediaOnly?: boolean;
	} = {}
): { destroy(): void } {
	const onError = opts.onError ?? ((m) => console.error('첨부 업로드 실패:', m));
	// BUG-083(잠정): 댓글은 mediaOnly — 이미지/동영상만 인라인 임베드, 비미디어 차단.
	// (per-comment 첨부 기능은 On Hold.) onAttach 분기는 추후 재작업용으로 유지.
	const place = (up: Upload) => {
		if (opts.mediaOnly && !isMedia(up.ext)) {
			onError(MEDIA_ONLY_MSG);
			return;
		}
		if (opts.onAttach && !isMedia(up.ext)) uploadToSection(up, opts.onAttach, onError);
		else uploadAndInsertTextarea(ta, up, onError);
	};
	const onPaste = (e: ClipboardEvent) => {
		const picked = allowedFiles(e.clipboardData?.files);
		if (picked.length === 0) return; // 일반 텍스트 paste 는 기본 동작.
		e.preventDefault();
		for (const up of picked) place(up);
	};
	const onDrop = (e: DragEvent) => {
		const picked = allowedFiles(e.dataTransfer?.files);
		if (picked.length === 0) return;
		e.preventDefault();
		ta.focus();
		for (const up of picked) place(up);
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

/**
 * '첨부' 버튼 — textarea 커서 위치에 파일(다중) 업로드/삽입.
 * BUG-168: 파일 선택은 `pickUploads()` 로 — 로컬 Tauri 는 경로 기반(크기 무관).
 */
export function pickAndAttachTextarea(
	ta: HTMLTextAreaElement | undefined | null,
	onError: (msg: string) => void = (m) => console.error('첨부 업로드 실패:', m),
	onAttach?: (rel: string, name: string) => void
): void {
	if (!ta) return;
	void (async () => {
		const picked = await pickUploads();
		if (picked.length === 0) return;
		// BUG-083: onAttach 가 있으면 본문 첨부 버튼과 동일하게 (미디어 포함) 모두
		// 첨부 섹션으로. 없으면 커서 위치 인라인 삽입 (fallback).
		if (onAttach) {
			for (const up of picked) uploadToSection(up, onAttach, onError);
			return;
		}
		ta.focus();
		for (const up of picked) uploadAndInsertTextarea(ta, up, onError);
	})();
}

/**
 * DEV-069 후속(admin #8): '첨부파일 추가' 버튼 핸들러. 파일(다중) 선택 →
 * 커서 위치에 업로드/삽입. CodeMirror 편집기 전용 (quest/campaign 본문, 규칙).
 * 미디어는 embed, 그 외는 링크.
 * BUG-168: 파일 선택은 `pickUploads()` 로 — 로컬 Tauri 는 경로 기반(크기 무관).
 */
export function pickAndAttach(
	view: EditorView,
	onError: (msg: string) => void = (m) => console.error('첨부 업로드 실패:', m),
	onAttach?: (rel: string, name: string) => void,
	opts: { mediaOnly?: boolean } = {}
): void {
	void (async () => {
		const picked = await pickUploads();
		if (picked.length === 0) return;
		// BUG-083: 첨부 섹션 없는 편집기(메모·규칙)는 미디어만 인라인 허용, 비미디어 차단.
		if (opts.mediaOnly) {
			view.focus();
			let pos = view.state.selection.main.head;
			for (const up of picked) {
				if (!isMedia(up.ext)) {
					onError(MEDIA_ONLY_MSG);
					continue;
				}
				pos += uploadAndInsert(view, up, pos, onError);
			}
			return;
		}
		// DEV-156: onAttach 가 있으면 버튼 첨부는 (미디어 포함) 모두 '첨부 섹션'으로.
		if (onAttach) {
			for (const up of picked) uploadToSection(up, onAttach, onError);
			return;
		}
		view.focus();
		let pos = view.state.selection.main.head;
		for (const up of picked) {
			pos += uploadAndInsert(view, up, pos, onError);
		}
	})();
}

/**
 * 편집기 extension 생성 — quest / campaign 상세 initEditor 의 extensions 에 추가.
 * DEV-152: Tauri + 브라우저(server) 모드 둘 다 지원.
 */
export function attachmentExtension(
	onError: (msg: string) => void = (m) => console.error('첨부 업로드 실패:', m),
	onAttach?: (rel: string, name: string) => void,
	opts: { mediaOnly?: boolean } = {}
): Extension {
	// DEV-156 / BUG-083: paste/drop 규칙 —
	//  · mediaOnly(메모·규칙): 미디어만 인라인, 비미디어 차단 + 안내.
	//  · onAttach(quest/campaign 본문): 미디어 인라인, 비미디어는 첨부 섹션.
	//  · 둘 다 없으면: 모두 인라인 (fallback).
	const place = (view: EditorView, up: Upload, pos: number): number => {
		if (opts.mediaOnly && !isMedia(up.ext)) {
			onError(MEDIA_ONLY_MSG);
			return 0;
		}
		if (onAttach && !isMedia(up.ext)) {
			uploadToSection(up, onAttach, onError);
			return 0;
		}
		return uploadAndInsert(view, up, pos, onError);
	};
	return EditorView.domEventHandlers({
		paste(event, view) {
			const picked = allowedFiles(event.clipboardData?.files);
			if (picked.length === 0) return false; // 일반 텍스트 paste 는 기본 동작.
			event.preventDefault();
			let pos = view.state.selection.main.head;
			for (const up of picked) pos += place(view, up, pos);
			return true;
		},
		drop(event, view) {
			const picked = allowedFiles(event.dataTransfer?.files);
			if (picked.length === 0) return false;
			event.preventDefault();
			let pos =
				view.posAtCoords({ x: event.clientX, y: event.clientY }) ?? view.state.selection.main.head;
			for (const up of picked) pos += place(view, up, pos);
			return true;
		}
	});
}
