// DEV-444: `openguild-gui <길드> <문서번호>` — 시동 인자로 받은 문서번호를 화면 주소로.
//
// 문서번호는 본문의 `[[…]]` 링크와 같은 규칙으로 푼다 — `DEV-001` · `C-001` · `BOOK-001` · 규칙
// 이름, 그리고 `rule:이름` 같은 접두. 그래야 "링크로 쓸 수 있는 것은 밖에서도 열 수 있다" 가 된다.

import { loadQuestIndex, refHref, resolveCrossLinkToken } from '$lib/stores/questIndex';

/** 문서번호 → 화면 주소. 길드에 없는 것이면 `null`. */
export async function launchDocHref(token: string): Promise<string | null> {
	await loadQuestIndex();
	const r = resolveCrossLinkToken(token.trim());
	if (!r.ref) return null;
	const kind = r.kind ?? r.ref.kind;
	return refHref(kind === 'rule' ? r.id : r.id.toUpperCase(), kind, r.ref.slug);
}
