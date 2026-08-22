// DEV-087: 캠페인 배너 이미지 URL 해석.
//
// WebView 는 raw file:// 로드를 차단 (frontend origin 이 http://tauri.localhost
// — cross-origin) — Tauri-local 모드는 asset protocol (convertFileSrc), 그 외
// (브라우저, 또는 BUG-097: Tauri + 원격 연결)는 서버의 bytes endpoint 사용.

import { detectEnvironment } from '$lib/api/transport';
import { getRemoteServerUrl } from '$lib/stores/remoteServer';

// BUG-097(사용자 보고: "이미지 첨부한게 표시가 안된다"): 이전엔
// `detectEnvironment() === 'tauri'` 만 보고 분기해, Tauri + 원격 연결 상태
// 에서도 무조건 "로컬" asset 경로(현재 invoke 로 얻는 Rust 의 로컬 Store
// 경로 — 원격 연결 시엔 보통 미연결/placeholder)로 `convertFileSrc` 를
// 만들어버렸다. 존재하지 않는 로컬 경로라 이미지가 깨짐. Nav 길드 이름
// (DEV-113 후속)/board bounce guard(BUG-095) 와 동일한 패턴의 버그.
function isTauriLocal(): boolean {
	return detectEnvironment() === 'tauri' && !getRemoteServerUrl();
}

/** Tauri + 원격 연결 시 HTTP 요청에 붙일 base — 그 외(브라우저)는 빈 문자열(같은 origin). */
function httpBase(): string {
	if (detectEnvironment() === 'tauri') return getRemoteServerUrl() ?? '';
	return '';
}

// DEV-113 후속: Welcome 에서 같은 프로세스 안에 다른 로컬 길드로 전환할 수
// 있게 되어 "길드 경로는 세션 동안 불변" 가정이 깨졌다 — 매번 새로 조회
// (Tauri invoke 는 가벼운 로컬 호출이라 캐싱 불필요).
async function localGuildPath(): Promise<string> {
	const { invoke } = await import('@tauri-apps/api/core');
	return await invoke<string>('current_guild_path');
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
	if (isTauriLocal()) {
		const { convertFileSrc } = await import('@tauri-apps/api/core');
		const root = await localGuildPath();
		return convertFileSrc(`${root}/.guild/${imagePath}`);
	}
	return `${httpBase()}/api/campaigns/${encodeURIComponent(slug)}/image`;
}

/**
 * DEV-069: `.guild/` 상대 경로 (`attachments/foo.png` / `assets/...`) → 표시
 * 가능 URL. markdown 본문의 로컬 이미지 / 동영상 참조 해석용.
 */
/**
 * BUG-241: 문서의 첨부 전체를 zip 으로 받는 URL.
 *
 * 폴더에 직접 쓰는 경로(File System Access)를 쓸 수 없는 환경 — 폰에서
 * `http://<LAN IP>` 로 접속하면 평문 HTTP 라 보안 컨텍스트가 아니고 모바일
 * 브라우저는 그 API 를 지원하지도 않는다 — 에서 첨부를 전부 받는 유일한 방법이
 * 다운로드를 1건으로 만드는 것이다.
 *
 * base URL 계산은 이 파일 안에 모아둔다(`httpBase` 는 여기 전용).
 */
export function guildAttachmentsZipUrl(
	scope: 'quest' | 'campaign' | 'library',
	slug: string
): string {
	const id = encodeURIComponent(slug);
	const base = httpBase();
	if (scope === 'campaign') return `${base}/api/campaigns/${id}/attachments.zip`;
	if (scope === 'library') return `${base}/api/library/${id}/attachments.zip`;
	return `${base}/api/quests/by/${id}/attachments.zip`;
}

export async function guildFileUrl(relPath: string): Promise<string> {
	if (isTauriLocal()) {
		const { convertFileSrc } = await import('@tauri-apps/api/core');
		const root = await localGuildPath();
		return convertFileSrc(`${root}/.guild/${relPath}`);
	}
	// 브라우저 모드(base='') 또는 Tauri+원격(base=원격 URL) — 서버가
	// attachments/ + assets/ 만 서빙.
	return `${httpBase()}/api/guild-files/${relPath}`;
}
