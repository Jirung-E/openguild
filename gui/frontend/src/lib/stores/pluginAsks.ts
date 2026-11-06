// DEV-479: 플러그인 확인 창 — 플러그인이 `ask()` 로 물은 것을 가져와 하나씩 띄우고, 답을 돌려준다.
//
// 질문은 계속 다시 가져온다(한 번에 최대 25초 기다리는 요청). 다른 창이 먼저 답했거나 시간이 지난 질문은
// 다음 목록에서 빠지므로 이 창에서도 닫힌다.
import { writable } from 'svelte/store';
import { pluginAsksApi, type PluginAsk } from '$lib/api/pluginAsks';

/** 지금 답을 기다리는 질문 — 앞의 것부터 띄운다. */
export const pluginAsks = writable<PluginAsk[]>([]);

let current: PluginAsk[] = [];
pluginAsks.subscribe((v) => (current = v));

/** 실패하면 잠깐 쉬었다 다시 — 옛 서버(경로 없음)에 쉬지 않고 두드리지 않게. */
export const RETRY_MS = 5000;

const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));

/** 질문을 가져오기 시작한다. 돌려준 함수를 부르면 멈춘다. */
export function startPluginAsks(): () => void {
	let running = true;
	void (async () => {
		while (running) {
			try {
				const got = await pluginAsksApi.wait(current.map((q) => q.id));
				if (running) pluginAsks.set(got);
			} catch {
				if (running) await sleep(RETRY_MS);
			}
		}
	})();
	return () => {
		running = false;
	};
}

/** 답한다. 이미 끝난 질문이면(다른 창이 답함 · 시간 지남) 조용히 넘어간다 — 그 질문은 어차피 닫힌다. */
export async function answerPluginAsk(ask: PluginAsk, yes: boolean): Promise<void> {
	pluginAsks.update((list) => list.filter((q) => q.id !== ask.id));
	try {
		await pluginAsksApi.answer(ask.id, yes);
	} catch {
		/* 끝난 질문 */
	}
}
