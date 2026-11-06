// DEV-479: 플러그인 확인 창 — 플러그인이 `ask()` 로 물은 것을 가져오고 답한다. 앱은 로컬 길드의 것을,
// 웹 · 원격은 서버의 것(이 창이 일으킨 일에서 물은 것)을 받는다.
import { api } from './client';

export interface PluginAskOptions {
	yes?: string;
	no?: string;
	danger?: boolean;
}

export interface PluginAsk {
	id: number;
	plugin: string;
	text: string;
	options: PluginAskOptions;
}

export const pluginAsksApi = {
	/** 질문들. 이미 보여 주는 것(`known`)과 같으면 바뀔 때까지(최대 25초) 기다렸다 돌아온다. */
	wait: (known: number[]) =>
		api.get<PluginAsk[]>(`/api/plugin-asks?known=${encodeURIComponent(known.join(','))}`),
	answer: (id: number, yes: boolean) => api.post<unknown>(`/api/plugin-asks/${id}`, { yes })
};
