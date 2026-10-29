// DEV-468: 자식 창(새 창으로 연 문서)에서 본 문서도 '최근 본 문서'에.
//
// 최근 목록은 창마다 sessionStorage 에 있고(DEV-276), '최근' 단추는 메인 창에만 있다 — 자식 창은 상단바를
// 숨긴다. 그래서 자식 창 안에서 옮겨 다닌 문서는 어디에도 남지 않았다(BUG-352 는 새 창을 **여는** 순간만
// 여는 쪽이 기록한다). 자식 창은 문서를 볼 때마다 메인 창에 알리고, 메인 창이 자기 목록에 올린다 — 목록은
// 하나, 앱을 끄면 지금처럼 비워진다. 웹(브라우저 탭)에는 자식 창이 없으니 아무것도 안 한다.
import { detectEnvironment } from '$lib/api/transport';
import { recordRecentDocHref } from '$lib/stores/recentDocs';

/** 창끼리 주고받는 이벤트 이름. */
export const RECENT_DOC_EVENT = 'og-recent-doc';
/** 메인 창 label — `capabilities/default.json` 의 `main`. */
const MAIN_LABEL = 'main';

/** 자식 창 → 메인 창: 이 문서를 봤다. 메인 창이 없거나 보내기가 실패해도 조용히 넘긴다(보조 기능). */
export async function forwardRecentDoc(href: string): Promise<void> {
	if (detectEnvironment() !== 'tauri') return;
	try {
		const { emitTo } = await import('@tauri-apps/api/event');
		await emitTo(MAIN_LABEL, RECENT_DOC_EVENT, href);
	} catch {
		/* 메인 창이 닫히는 중 등 — 최근 목록 하나를 놓치는 것뿐이다. */
	}
}

/** 메인 창: 자식 창이 알린 문서를 최근 목록에 올린다. 돌려준 함수로 그만 듣는다. */
export async function listenRecentDocs(): Promise<() => void> {
	if (detectEnvironment() !== 'tauri') return () => {};
	try {
		const { listen } = await import('@tauri-apps/api/event');
		return await listen<unknown>(RECENT_DOC_EVENT, (e) => {
			if (typeof e.payload === 'string') recordRecentDocHref(e.payload);
		});
	} catch {
		return () => {};
	}
}
