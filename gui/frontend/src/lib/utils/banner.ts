// DEV-087: 캠페인 배너 이미지 URL 해석.
//
// WebView 는 raw file:// 로드를 차단 (frontend origin 이 http://tauri.localhost
// — cross-origin) — Tauri 모드는 asset protocol (convertFileSrc), 브라우저
// (HTTP) 모드는 서버의 bytes endpoint 사용.

import { detectEnvironment } from '$lib/api/transport';

// 길드 경로는 세션 동안 불변 — 1회 조회 후 메모.
let guildPathPromise: Promise<string> | null = null;
async function guildPath(): Promise<string> {
	if (!guildPathPromise) {
		guildPathPromise = (async () => {
			const { invoke } = await import('@tauri-apps/api/core');
			return await invoke<string>('current_guild_path');
		})();
	}
	return guildPathPromise;
}

/**
 * `image_path` (`.guild/` 상대 — 예 "assets/C-001-banner.png") → 표시 가능 URL.
 * 배너 없으면 null.
 */
export async function campaignBannerUrl(
	slug: string,
	imagePath: string | null | undefined
): Promise<string | null> {
	if (!imagePath) return null;
	if (detectEnvironment() === 'tauri') {
		const { convertFileSrc } = await import('@tauri-apps/api/core');
		const root = await guildPath();
		return convertFileSrc(`${root}/.guild/${imagePath}`);
	}
	return `/api/campaigns/${encodeURIComponent(slug)}/image`;
}

/**
 * DEV-069: `.guild/` 상대 경로 (`attachments/foo.png` / `assets/...`) → 표시
 * 가능 URL. markdown 본문의 로컬 이미지 / 동영상 참조 해석용.
 */
export async function guildFileUrl(relPath: string): Promise<string> {
	if (detectEnvironment() === 'tauri') {
		const { convertFileSrc } = await import('@tauri-apps/api/core');
		const root = await guildPath();
		return convertFileSrc(`${root}/.guild/${relPath}`);
	}
	// 브라우저 모드 — 서버가 attachments/ + assets/ 만 서빙.
	return `/api/guild-files/${relPath}`;
}
