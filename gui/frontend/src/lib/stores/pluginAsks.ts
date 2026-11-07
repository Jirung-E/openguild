// DEV-479: 플러그인 확인 창 — 플러그인이 `ask()` 로 물은 것을 가져와 하나씩 띄우고, 답을 돌려준다.
//
// 질문은 계속 다시 가져온다(한 번에 최대 25초 기다리는 요청). 다른 창이 먼저 답했거나 시간이 지난 질문은
// 다음 목록에서 빠지므로 이 창에서도 닫힌다.
import { writable } from 'svelte/store';
import { pluginAsksApi, type PluginAsk } from '$lib/api/pluginAsks';
import { remoteServerUrl } from './remoteServer';

/** 지금 답을 기다리는 질문 — 앞의 것부터 띄운다. */
export const pluginAsks = writable<PluginAsk[]>([]);

let current: PluginAsk[] = [];
pluginAsks.subscribe((v) => (current = v));

/** 실패하면 잠깐 쉬었다 다시 — 옛 서버(경로 없음)에 쉬지 않고 두드리지 않게. */
export const RETRY_MS = 5000;

const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));

/** 질문을 가져오기 시작한다. 돌려준 함수를 부르면 멈춘다.
 *
 * 붙은 곳(로컬 길드 ↔ 원격 서버)이 바뀌면 기다리던 요청을 버리고 새 곳에서 바로 다시 가져온다 — 옛 곳의 기다림
 * (최대 25초)이 끝나기를 기다리면, 그사이 새 서버는 이 창이 질문을 가지러 오지 않으니 닫힌 창으로 보고 아니요로
 * 답했다(원격으로 바꾼 직후의 첫 확인 창이 안 떴다). */
export function startPluginAsks(): () => void {
	let running = true;
	let kick: () => void = () => {};
	let first = true;
	const stopWatch = remoteServerUrl.subscribe(() => {
		if (first) {
			first = false;
			return;
		}
		pluginAsks.set([]);
		kick();
	});
	void (async () => {
		while (running) {
			const switched = new Promise<'switched'>((r) => (kick = () => r('switched')));
			try {
				const got = await Promise.race([pluginAsksApi.wait(current.map((q) => q.id)), switched]);
				if (got === 'switched') continue;
				if (running) pluginAsks.set(got);
			} catch {
				if (running) await Promise.race([sleep(RETRY_MS), switched]);
			}
		}
	})();
	return () => {
		running = false;
		stopWatch();
		kick();
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
