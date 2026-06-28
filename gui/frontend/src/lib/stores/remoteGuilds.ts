// DEV-113 후속: 원격 길드 "최근 목록" — 로컬 recents(core::recents, Rust 측
// recents.json)와 같은 역할이지만 원격 URL 은 파일 경로가 아니라서 그 쪽에
// 합칠 수 없음. 같은 LRU 패턴(최대 10개, 최근 = 0번째)을 frontend 전용
// localStorage 로 재현 — Welcome 페이지가 local recents 와 이 목록을 합쳐
// 하나의 리스트로 보여준다(사용자 피드백: "원격 길드도 등록하면 일반
// 길드처럼 접속할 수 있어야").
//
// `remoteServer.ts` 의 `remoteServerUrl` 은 "지금 활성화된 연결" 하나만 추적
// (transport.ts 가 매 호출마다 읽음) — 이 모듈은 "예전에 연결했던 것들의
// 기록"을 추적. 역할이 다르므로 분리.

const KEY = 'openguild.remoteGuilds';
const MAX_REMOTE_GUILDS = 10;

export interface RemoteGuild {
	url: string;
	name: string;
	last_opened: string;
}

function load(): RemoteGuild[] {
	if (typeof localStorage === 'undefined') return [];
	try {
		const raw = localStorage.getItem(KEY);
		if (!raw) return [];
		const parsed = JSON.parse(raw);
		return Array.isArray(parsed) ? parsed : [];
	} catch {
		return [];
	}
}

function save(list: RemoteGuild[]): void {
	if (typeof localStorage === 'undefined') return;
	try {
		localStorage.setItem(KEY, JSON.stringify(list));
	} catch {
		/* 무시 */
	}
}

/** `http://192.168.1.10:3000` → `192.168.1.10:3000` — 이름 기본값. */
function deriveName(url: string): string {
	return url.replace(/^[a-z][a-z0-9+.-]*:\/\//i, '').replace(/\/+$/, '');
}

/** 최근 원격 길드 목록 (LRU 순, 최대 10). */
export function listRemoteGuilds(): RemoteGuild[] {
	return load();
}

/**
 * 연결 성공 시 호출 — core::recents::add() 의 원격판. 이미 있으면 최상단으로,
 * 없으면 추가. `core::recents::add` 가 `open_guild_in_current_window` 성공 시
 * 자동 호출되는 것과 동등하게, `connectRemote()` / 목록에서 재연결할 때 호출.
 */
export function registerRemoteGuild(url: string): RemoteGuild {
	const normalized = url.trim().replace(/\/+$/, '');
	const entry: RemoteGuild = {
		url: normalized,
		name: deriveName(normalized) || normalized,
		last_opened: new Date().toISOString()
	};
	const list = load().filter((g) => g.url !== normalized);
	list.unshift(entry);
	save(list.slice(0, MAX_REMOTE_GUILDS));
	return entry;
}

/** 단일 항목 제거 (url 기준). */
export function removeRemoteGuild(url: string): void {
	save(load().filter((g) => g.url !== url));
}

/** 전체 비우기. */
export function clearRemoteGuilds(): void {
	save([]);
}
