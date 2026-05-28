// DEV-063: Tauri auto-update wrapper.
//
// - Tauri (desktop) 환경에서만 동작. 브라우저 (http dev) 에서는 모두 no-op.
// - check() → 새 버전 있으면 Update 핸들 보관 → downloadAndInstall() →
//   process::relaunch() 로 재시작.
// - 플러그인 모듈은 동적 import — 브라우저 번들에 강제 로드되지 않게.

import { writable } from 'svelte/store';
import { detectEnvironment } from './transport';

export type UpdateState =
	| { status: 'idle' }
	| { status: 'checking' }
	| { status: 'available'; version: string; notes: string; date?: string }
	| { status: 'downloading'; version: string; pct: number | null }
	| { status: 'ready'; version: string } // 설치 완료, relaunch 대기
	| { status: 'uptodate' }
	| { status: 'error'; message: string };

export const updateState = writable<UpdateState>({ status: 'idle' });

// check() 가 반환한 Update 핸들. available → download 사이에 보관.
// 타입은 plugin 의 Update (동적 import 라 unknown 으로 보관 후 좁힘).
let pendingUpdate: {
	version: string;
	body?: string;
	date?: string;
	downloadAndInstall: (
		onEvent?: (e: {
			event: 'Started' | 'Progress' | 'Finished';
			data?: { contentLength?: number; chunkLength?: number };
		}) => void
	) => Promise<void>;
} | null = null;

/**
 * 새 버전 체크. 결과는 updateState 스토어로 전파.
 *
 * @param opts.silent true 면 'uptodate' / 'error' 를 idle 로 되돌림 (백그라운드
 *        시작 체크용 — 최신이면 UI 노이즈 없이 조용히). false (기본) 면 상태 유지.
 */
export async function checkForUpdate(opts: { silent?: boolean } = {}): Promise<void> {
	const silent = opts.silent ?? false;
	if (detectEnvironment() !== 'tauri') {
		// 브라우저 — 업데이트 개념 없음.
		if (!silent) updateState.set({ status: 'uptodate' });
		return;
	}
	updateState.set({ status: 'checking' });
	try {
		const { check } = await import('@tauri-apps/plugin-updater');
		const update = await check();
		if (update) {
			pendingUpdate = update as unknown as typeof pendingUpdate;
			updateState.set({
				status: 'available',
				version: update.version,
				notes: update.body ?? '',
				date: update.date
			});
		} else {
			pendingUpdate = null;
			if (silent) updateState.set({ status: 'idle' });
			else updateState.set({ status: 'uptodate' });
		}
	} catch (e) {
		const message = e instanceof Error ? e.message : String(e);
		if (silent) updateState.set({ status: 'idle' });
		else updateState.set({ status: 'error', message });
	}
}

/**
 * 보관된 Update 를 다운로드 + 설치 + 재시작.
 * available 상태 (pendingUpdate 존재) 에서만 의미 있음.
 */
export async function downloadAndRelaunch(): Promise<void> {
	if (!pendingUpdate) return;
	const version = pendingUpdate.version;
	try {
		let downloaded = 0;
		let total = 0;
		updateState.set({ status: 'downloading', version, pct: null });
		await pendingUpdate.downloadAndInstall((e) => {
			if (e.event === 'Started') {
				total = e.data?.contentLength ?? 0;
				downloaded = 0;
				updateState.set({ status: 'downloading', version, pct: total > 0 ? 0 : null });
			} else if (e.event === 'Progress') {
				downloaded += e.data?.chunkLength ?? 0;
				const pct = total > 0 ? Math.min(100, Math.round((downloaded / total) * 100)) : null;
				updateState.set({ status: 'downloading', version, pct });
			} else if (e.event === 'Finished') {
				updateState.set({ status: 'ready', version });
			}
		});
		updateState.set({ status: 'ready', version });
		// 설치 완료 — 재시작 (NSIS 가 새 버전으로 교체 후 실행).
		const { relaunch } = await import('@tauri-apps/plugin-process');
		await relaunch();
	} catch (e) {
		const message = e instanceof Error ? e.message : String(e);
		updateState.set({ status: 'error', message });
	}
}

/** "나중에" — 상태를 idle 로 (배너 닫기). pendingUpdate 는 유지 (다시 열 수 있음). */
export function dismissUpdate(): void {
	updateState.set({ status: 'idle' });
}
