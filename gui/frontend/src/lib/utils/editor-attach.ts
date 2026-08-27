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
import { detectEnvironment, isLocalTauri, postWithUploadProgress } from '$lib/api/transport';
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
async function saveAttachmentBytes(
	file: File,
	ext: string,
	report?: UploadReport,
	signal?: AbortSignal
): Promise<string> {
	// DEV-337: **HTTP(원격/브라우저)는 스트리밍 라우트로 원문을 그대로 보낸다.**
	// 예전엔 파일을 통째로 읽어 base64(약 1.33배)로 만들어 JSON 에 담았고, 그
	// 방식 때문에 64MB 상한이 붙어 있었다(원래 명분이던 index.db blob 백업은
	// BUG-188 에서 이미 사라진 상태였다). 원문 전송은 메모리가 상수고 전송량도
	// 25% 줄며, 진행률(upload.onprogress)도 그대로 동작한다.
	if (detectEnvironment() === 'http' || getRemoteServerUrl()) {
		report?.({ phase: 'uploading', percent: 0 });
		const qs = new URLSearchParams({ ext });
		if (file.name) qs.set('name', file.name);
		return postWithUploadProgress<string>(
			`/api/attachments/stream?${qs.toString()}`,
			file,
			(sent, total) =>
				report?.({ phase: 'uploading', percent: total > 0 ? (sent / total) * 100 : null }),
			signal
		);
	}
	// Tauri + 로컬(원격 아님)에서 붙여넣기/드래그&드랍으로 들어온 bytes — invoke
	// 경로라 base64 를 그대로 쓴다. 여기엔 IPC payload 한도가 있어 상한 유지.
	if (file.size > MAX_ATTACHMENT_BYTES) throw new Error(tooLargeMessage(file));
	// DEV-321: base64 변환은 전송 **전**이고 진행을 관측할 수 없다 — 큰 파일이면
	// 여기서 수 초 멈춘 것처럼 보이므로 '준비 중' 단계로 따로 알린다.
	report?.({ phase: 'preparing', percent: null });
	const data_base64 = toBase64(await file.arrayBuffer());
	report?.({ phase: 'uploading', percent: 0 });
	// HTTP body 필드는 이 프로젝트 컨벤션상 snake_case(server 의 axum Deserialize
	// 와 1:1) — transport.ts 의 routeToInvoke 가 Tauri invoke 용 camelCase args 로 변환.
	// DEV-324: 원본 파일명도 보낸다 — 저장 파일명에 남아 나중에 알아볼 수 있다.
	// 붙여넣기처럼 이름이 없으면(빈 문자열) 보내지 않는다.
	const name = file.name || undefined;
	return postWithUploadProgress<string>(
		'/api/attachments',
		{ data_base64, ext, name },
		(sent, total) =>
			report?.({ phase: 'uploading', percent: total > 0 ? (sent / total) * 100 : null }),
		signal
	);
}

/**
 * BUG-168: 업로드 1건 — "무엇을 어떻게 올릴지"를 감싼 단위.
 *
 * bytes(붙여넣기·드래그&드랍·브라우저 파일선택)와 경로(로컬 Tauri 파일선택)는
 * 전송 방식이 전혀 다른데 삽입/치환 로직은 같다. 호출부가 분기하지 않도록
 * `run()` 뒤로 숨긴다 — 반환값은 양쪽 모두 `.guild` 상대 경로.
 */
type Upload = {
	name: string;
	ext: string;
	/** DEV-323: `signal` 이 abort 되면 전송/복사를 중단한다(AbortError 로 reject). */
	run: (report?: UploadReport, signal?: AbortSignal) => Promise<string>;
};

/** DEV-321: 업로드 1건의 진행 보고 — 단계 + 퍼센트(모르면 null). */
export type UploadReport = (p: { phase: AttachPhase; percent: number | null }) => void;

function uploadFromFile(file: File): Upload {
	const ext = extOf(file);
	return {
		name: file.name || 'clipboard',
		ext,
		run: (report, signal) => saveAttachmentBytes(file, ext, report, signal)
	};
}

function uploadFromPath(path: string): Upload {
	const name = path.split(/[\\/]/).pop() || path;
	return {
		name,
		ext: extOfName(name),
		run: async (report, signal) => (await uploadAttachmentPath(path, report, signal)).rel
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
async function uploadAttachmentPath(
	path: string,
	report?: UploadReport,
	signal?: AbortSignal
): Promise<{ rel: string; name: string }> {
	const { invoke } = await import('@tauri-apps/api/core');
	const name = path.split(/[\\/]/).pop() || path;
	// DEV-323: signal 이 있으면 취소를 위해 uploadId 경로를 타야 한다(플래그를
	// 걸 대상이 있어야 하므로).
	if (!report && !signal) {
		const rel = await invoke<string>('save_attachment_from_path', { path });
		return { rel, name };
	}
	// DEV-321: Rust 가 버퍼 단위로 복사하며 진행을 이벤트로 보낸다. 여러 파일을
	// 연달아 올릴 때 섞이지 않도록 uploadId 로 자기 것만 걸러낸다.
	const { listen } = await import('@tauri-apps/api/event');
	const uploadId = `att-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`;
	report?.({ phase: 'uploading', percent: 0 });
	const unlisten = await listen<{ upload_id: string; copied: number; total: number }>(
		'attachment://progress',
		(e) => {
			if (e.payload.upload_id !== uploadId) return;
			const { copied, total } = e.payload;
			report?.({ phase: 'uploading', percent: total > 0 ? (copied / total) * 100 : null });
		}
	);
	// DEV-323: 취소는 Rust 쪽 플래그로 전달한다 — 복사 루프가 4MiB 청크마다
	// 확인하고 멈추면서 **조각 파일을 지운다**(core 의 cancel_removes_partial_file).
	const onAbort = () => void invoke('cancel_attachment_upload', { uploadId });
	signal?.addEventListener('abort', onAbort, { once: true });
	try {
		const rel = await invoke<string>('save_attachment_from_path', { path, uploadId });
		return { rel, name };
	} finally {
		unlisten();
		signal?.removeEventListener('abort', onAbort);
	}
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
	onQueue?: (q: AttachQueueItem[] | null) => void;
	/**
	 * DEV-323: 취소 손잡이를 호출부(UI)에 넘긴다. 파일 선택 직후 한 번 호출되며,
	 * 업로드가 끝나면 `null` 로 다시 호출해 버튼을 걷게 한다.
	 *
	 * DEV-338: 전체 취소(`cancelAll`)와 **항목별 취소**(`cancelOne(id)`)를 함께
	 * 넘긴다. 여러 개를 올리는 중 하나만 빼려면 전부 취소하고 다시 고르는 수밖에
	 * 없었다(admin 보고).
	 */
	onCancelHandle?: (
		h: { cancelAll: () => void; cancelOne: (id: number) => void } | null
	) => void;
}): Promise<void> {
	const { onOne, onError, onQueue, onCancelHandle } = handlers;
	const picked = await pickUploads();
	if (picked.length === 0) return;
	// DEV-322: 고른 파일 **전체**를 먼저 목록으로 보여준다. 예전엔 현재 파일
	// 하나와 순번(n/N)만 보여서 "무엇이 남았는지" 를 알 수 없었다(admin 보고).
	const queue: AttachQueueItem[] = picked.map((up, i) => ({
		id: i,
		name: up.name,
		status: 'pending',
		phase: null,
		percent: null
	}));
	const emit = () => onQueue?.(queue.map((it) => ({ ...it })));
	emit();
	// DEV-323 / DEV-338: 취소는 **항목마다** AbortController 를 둔다. 하나만
	// 끊어도 나머지는 계속 진행돼야 하기 때문. 전체 취소는 이것들을 모두 끊는다.
	const aborts = queue.map(() => new AbortController());
	let cancelledAll = false;
	/** 아직 시작 안 한 항목은 시작 자체를 막는다 — 정리할 것이 없다. */
	const markCancelled = (it: AttachQueueItem) => {
		if (it.status === 'pending' || it.status === 'uploading') {
			it.status = 'cancelled';
			it.percent = null;
		}
	};
	const cancelOne = (id: number) => {
		const it = queue[id];
		if (!it || it.status === 'done' || it.status === 'error') return;
		aborts[id].abort();
		markCancelled(it);
		emit();
	};
	const cancelAll = () => {
		cancelledAll = true;
		for (const it of queue) {
			if (it.status === 'done' || it.status === 'error') continue;
			aborts[it.id].abort();
			markCancelled(it);
		}
		emit();
	};
	onCancelHandle?.({ cancelAll, cancelOne });
	let failed = false;
	// DEV-322: 병렬 업로드 — 실측 결과 순차보다 빨랐다(로컬 HTTP 9~24%,
	// 디스크 복사 28~52%. 상세는 퀘스트 댓글). 다만 **환경에 따라 위험이 다르다**:
	//
	// - 데스크톱(경로 기반): Rust 가 4MiB 버퍼로 스트리밍 복사하므로 파일 크기와
	//   무관하게 메모리가 상수다 → 동시 3개까지 안전.
	// - 브라우저(bytes 기반): 파일을 통째로 읽어 base64(약 1.33배)로 만들어
	//   보낸다. 동시에 돌리면 그만큼 메모리가 배로 늘어난다 — BUG-168 이
	//   1.5GB 첨부였던 걸 생각하면 여기서 동시성을 올리면 안 된다 → 1개씩.
	const concurrency = isLocalTauri() ? 3 : 1;
	// 등록(onOne)은 첨부 목록 sidecar 를 read-modify-write 하므로 동시에 부르면
	// lost update 가 난다. 업로드만 병렬로 하고 등록은 이 체인으로 직렬화한다.
	let registerChain: Promise<void> = Promise.resolve();
	const runOne = async (it: AttachQueueItem) => {
		// 이미 취소된 항목(전체 취소 포함)은 시작하지 않는다.
		if (cancelledAll || it.status === 'cancelled' || aborts[it.id].signal.aborted) {
			markCancelled(it);
			emit();
			return;
		}
		const up = picked[it.id];
		it.status = 'uploading';
		it.phase = 'uploading';
		emit();
		try {
			const rel = await up.run((p) => {
				it.phase = p.phase;
				it.percent = p.percent;
				emit();
			}, aborts[it.id].signal);
			it.status = 'done';
			it.percent = 100;
			emit();
			const name = up.name || rel.split('/').pop() || rel;
			registerChain = registerChain.then(() => onOne({ rel, name }));
			await registerChain;
		} catch (e) {
			// DEV-323: 취소는 실패가 아니다 — 배너를 띄우지 않고 상태만 바꾼다.
			// 브라우저는 AbortError, 데스크톱은 Rust 의 AppError::Cancelled 문자열.
			if (isCancellation(e, aborts[it.id].signal.aborted)) {
				it.status = 'cancelled';
				it.percent = null;
				emit();
				return;
			}
			const msg = e instanceof Error ? e.message : String(e);
			it.status = 'error';
			it.error = msg;
			failed = true;
			emit();
			onError(msg);
		}
	};
	try {
		if (concurrency <= 1) {
			for (const it of queue) await runOne(it);
		} else {
			// 고정 크기 워커 — 앞에서부터 하나씩 집어간다.
			let next = 0;
			const worker = async () => {
				while (next < queue.length) await runOne(queue[next++]);
			};
			await Promise.all(
				Array.from({ length: Math.min(concurrency, queue.length) }, () => worker())
			);
		}
	} finally {
		onCancelHandle?.(null);
		// 전부 성공했으면 목록을 걷는다. 실패가 있으면 **남겨둔다** — 어떤 파일이
		// 실패했는지가 배너 메시지 하나보다 중요하다. 취소도 남긴다(무엇이
		// 올라갔고 무엇이 안 올라갔는지가 사용자에게 필요한 정보다).
		const anyCancelled = queue.some((it) => it.status === 'cancelled');
		if (!failed && !anyCancelled) onQueue?.(null);
	}
}

/** 취소로 인한 중단인지 — 실패 배너를 띄우지 않기 위한 판정. */
function isCancellation(e: unknown, cancelled: boolean): boolean {
	if (e instanceof DOMException && e.name === 'AbortError') return true;
	// Rust 쪽은 문자열로 넘어온다(AppError::Cancelled 의 tf! 메시지).
	const msg = e instanceof Error ? e.message : String(e);
	return cancelled && /취소|cancel/i.test(msg);
}

/**
 * DEV-321: 업로드 단계.
 * - `preparing` — 아직 보내기 전(브라우저 경로의 base64 변환). %를 알 수 없다.
 * - `uploading` — 실제 전송/복사 중. 보통 %가 함께 온다.
 */
export type AttachPhase = 'preparing' | 'uploading';

/**
 * DEV-322: 업로드 대기열 한 줄.
 *
 * DEV-298/321 에서는 "현재 파일 하나" 만 보고했는데, 여러 개를 고르면 남은
 * 것들이 안 보였다. 이제 고른 파일 전체가 이 목록으로 나가고 각 항목이 자기
 * 상태를 들고 있다. `percent === null` 이면 관측 불가(불확정 바).
 */
export type AttachItemStatus = 'pending' | 'uploading' | 'done' | 'error' | 'cancelled';
export type AttachQueueItem = {
	/** picked 배열의 인덱스 — 목록 key. */
	id: number;
	name: string;
	status: AttachItemStatus;
	phase: AttachPhase | null;
	percent: number | null;
	error?: string;
};

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
