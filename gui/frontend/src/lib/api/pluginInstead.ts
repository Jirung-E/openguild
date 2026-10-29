/**
 * DEV-465: 플러그인이 변경 대신 다른 일을 했다(예: 댓글 `/session` 이 댓글 대신 세션을 연다). 남긴 것은 없지만
 * **실패가 아니다** — 화면은 오류 대신 안내를 띄우고 입력을 비운다. 서버(202 `{"instead"}`) · 앱(꼬리표 문자열)이
 * 같은 것으로 풀린다.
 */
export class PluginInsteadError extends Error {
	constructor(message: string) {
		super(message);
		this.name = 'PluginInsteadError';
	}
}

/** 앱(Tauri) 쪽 오류 문자열의 꼬리표 — gui/src/commands.rs 의 `PLUGIN_INSTEAD_TAG` 와 같다. */
export const PLUGIN_INSTEAD_TAG = 'PLUGIN_INSTEAD::';

/** 앱이 던진 문자열 → 대신 처리했으면 그것, 아니면 null. */
export function pluginInsteadFromInvoke(msg: string): PluginInsteadError | null {
	return msg.startsWith(PLUGIN_INSTEAD_TAG)
		? new PluginInsteadError(msg.slice(PLUGIN_INSTEAD_TAG.length))
		: null;
}

/** 서버의 202 본문 → 대신 처리했으면 그것, 아니면 null. */
export function pluginInsteadFromHttp(status: number, payload: unknown): PluginInsteadError | null {
	if (status !== 202) return null;
	const msg = (payload as { instead?: unknown } | null)?.instead;
	return typeof msg === 'string' ? new PluginInsteadError(msg) : null;
}

export function isPluginInstead(e: unknown): e is PluginInsteadError {
	return e instanceof PluginInsteadError;
}
