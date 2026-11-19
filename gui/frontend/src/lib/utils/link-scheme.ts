// DEV-485: 문서 속 링크의 주소 종류 — 앱 밖(다른 앱 · 브라우저)으로 넘기는 링크는 종류를 가리지 않는다
// (`vscode://` · `obsidian://` 등). 무엇을 열지는 그 앱 몫이다 — 메신저처럼 길드가 거르지 않는다.
// 막는 것은 앱 **안에서** 도는 주소뿐: `javascript:` · `vbscript:` 는 앱 화면에서 스크립트로 돌고, `data:` 는
// 앱 창 안에서 페이지를 연다(이미지 `src` 의 data: 는 DOMPurify 가 따로 허용한다).

/** 앱 안에서 도는 주소. */
const IN_APP = /^(?:javascript|vbscript|data):/i;

/**
 * DOMPurify 의 `ALLOWED_URI_REGEXP` — 위 셋이 아니면 통과. DOMPurify 는 공백 · 제어 문자를 뺀 뒤 견주므로
 * `java\tscript:` 도 걸린다.
 */
export const ALLOWED_URI_REGEXP = /^(?!(?:javascript|vbscript|data):)/i;

/** 앱 창 안에서 여는 주소(앱 자신의 것) — 바깥으로 넘기지 않는다. */
const APP_OWN = /^(?:tauri|asset|blob|ipc|about):/i;

/** 누르면 바깥 앱으로 넘길 링크인가 — 주소 종류(`xxx:`)가 있고, 앱 안에서 도는 것 · 앱 자신의 것이 아니다. */
export function isExternalHref(href: string): boolean {
	const h = href.trim();
	if (!/^[a-z][a-z0-9+.-]*:/i.test(h)) return false; // `/quests/..` · `#..` · `?..` — 앱 안 이동
	if (IN_APP.test(h) || APP_OWN.test(h)) return false;
	// 앱이 떠 있는 주소(`http://tauri.localhost/..`)는 앱 안 이동이다.
	if (typeof location !== 'undefined' && /^https?:/i.test(h)) {
		try {
			if (new URL(h).origin === location.origin) return false;
		} catch {
			/* 깨진 주소 — 바깥으로 넘겨 그쪽이 판단하게 */
		}
	}
	return true;
}
