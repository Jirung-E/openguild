// DEV-479: 이 창의 이름 — 서버로 가는 요청마다 붙인다. 서버의 플러그인이 `ask()` 하면 그 일을 일으킨 창에 묻는다.
// 창(페이지)마다 하나. 새로 고치면 새 이름이다 — 그 사이에 온 질문은 서버가 "창이 닫혔다" 로 보고 아니요로 친다.

export const ASK_CLIENT_HEADER = 'x-openguild-client';

function make(): string {
	const rand =
		typeof crypto !== 'undefined' && 'randomUUID' in crypto
			? crypto.randomUUID()
			: `${Date.now().toString(16)}-${Math.random().toString(16).slice(2)}`;
	return `web-${rand}`.slice(0, 64);
}

export const askClientId = make();
