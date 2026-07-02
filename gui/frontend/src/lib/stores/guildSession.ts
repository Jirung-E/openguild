// DEV-207 후속(사용자 보고: "길드를 열었다가 welcome으로 돌아가서 확인했을때"
// 설정 페이지가 여전히 길드가 열려있는 것처럼 표시됨): Rust 의 launch_mode
// 는 로컬 길드가 한 번이라도 열리면 "guild"로 고정되고, Welcome 으로
// 돌아가도 리셋되지 않는다(Nav 로고 → /welcome 이동은 순수 frontend
// 라우팅이라 Rust invoke 자체가 없음) — 원격(remoteServerUrl/
// isRemoteSessionActive)도 마찬가지로 Welcome 방문으로 안 풀린다.
//
// "지금 실제로 어떤 길드가 화면에 표시 중인가"는 Rust 상태가 아니라
// frontend 라우팅 사실(보드가 마지막으로 bounce 없이 마운트됐는가)로
// 판단해야 정확하다. sessionStorage(프로세스 재시작마다 리셋)로 추적 —
// board 의 onMount 가 bounce 안 하면 active 로, welcome 의 onMount 는
// 항상 inactive 로 마크.

const KEY = 'openguild.guildContextActive';

export function markGuildContextActive(): void {
	if (typeof sessionStorage === 'undefined') return;
	try {
		sessionStorage.setItem(KEY, '1');
	} catch {
		/* 무시 */
	}
}

export function markGuildContextInactive(): void {
	if (typeof sessionStorage === 'undefined') return;
	try {
		sessionStorage.removeItem(KEY);
	} catch {
		/* 무시 */
	}
}

export function isGuildContextActive(): boolean {
	if (typeof sessionStorage === 'undefined') return false;
	try {
		return sessionStorage.getItem(KEY) === '1';
	} catch {
		return false;
	}
}
