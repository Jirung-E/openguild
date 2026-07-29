// See https://svelte.dev/docs/kit/types#app.d.ts
// for information about these interfaces
declare global {
	namespace App {
		// interface Error {}
		// interface Locals {}
		// interface PageData {}
		// BUG-176: 히스토리 항목이 어느 길드의 것인지 — 길드를 바꾼 뒤 뒤로가기로
		// 이전 길드의 항목에 도달했는지 판별하는 데 쓴다(URL 에는 길드 정보가
		// 없다). SvelteKit 의 shallow routing state 에 실어 항목마다 남긴다.
		interface PageState {
			guild?: { kind: 'local'; path: string } | { kind: 'remote'; url: string };
		}
		// interface Platform {}
	}
}

export {};
