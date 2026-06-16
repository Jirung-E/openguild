// DEV-060: 퀘스트 템플릿 (`.guild/templates/{name}.md`) — Tauri 전용.
// 템플릿은 local 파일 기반 (HTTP 미지원 — CLI 와 동일 정책). 브라우저
// 모드에선 호출하지 말 것 (caller 가 env 가드).

export interface QuestTemplate {
	name: string;
	title: string | null;
	type: string | null;
	urgency: number | null;
	tags: string[];
	body: string;
}

export interface SaveTemplateInput {
	name: string;
	title: string | null;
	type: string | null;
	urgency: number | null;
	tags: string[];
	body: string;
	/** 같은 이름이 이미 있으면 덮어쓰기 허용. */
	force: boolean;
}

export const templatesApi = {
	list: async (): Promise<QuestTemplate[]> => {
		const { invoke } = await import('@tauri-apps/api/core');
		return await invoke<QuestTemplate[]>('list_templates');
	},
	// DEV-158: 현재 입력을 템플릿으로 저장. 반환: 쓰여진 파일 경로.
	save: async (input: SaveTemplateInput): Promise<string> => {
		const { invoke } = await import('@tauri-apps/api/core');
		return await invoke<string>('save_template', { ...input });
	}
};
