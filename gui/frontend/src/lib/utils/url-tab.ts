// BUG-310: 탭을 **주소에 적는다**.
//
// 관리자·설정 페이지의 탭이 화면 안의 변수로만 있었다. 그래서 퀘스트 상세 같은 다른
// 페이지로 갔다 돌아오면 늘 첫 탭이었고(변수가 사라지니까), 탭을 옮겨도 뒤로 가기로
// 되짚을 수 없었다(주소가 안 바뀌니까). 주소에 적으면 둘이 한꺼번에 풀린다.
//
// 규칙 페이지가 이미 같은 일을 한다([[BUG-104]]) — 그쪽은 규칙 하나하나가 주소를 가지므로
// 로직이 그 페이지에 붙어 있다. 탭은 관리자·설정 둘이 똑같이 필요해서, 둘이 갈라지지 않게
// 여기로 뺐다. 화면을 안 그리니 시험도 쉽다.

/** 주소의 `?tab=` 이 아는 탭이면 그것, 아니면 첫 탭. */
export function readTab<T extends string>(url: URL, tabs: readonly T[], fallback: T): T {
	const got = url.searchParams.get('tab');
	return (tabs as readonly string[]).includes(got ?? '') ? (got as T) : fallback;
}

/**
 * 그 탭으로 가는 주소. **다른 조건은 건드리지 않는다** — 탭만 바꾼다.
 *
 * 첫 탭은 `?tab=` 을 아예 안 적는다. 주소가 짧아지기도 하지만, 그래야 `/admin` 과
 * `/admin?tab=structure` 가 같은 자리가 된다 — 같은 자리가 기록에 둘로 쌓이면 뒤로 가기가
 * 한 번 헛돈다.
 */
export function tabUrl(url: URL, tab: string, fallback: string): string {
	const q = new URLSearchParams(url.search);
	if (tab === fallback) q.delete('tab');
	else q.set('tab', tab);
	const s = q.toString();
	return s ? `${url.pathname}?${s}` : url.pathname;
}

/** 이미 그 탭이면 기록을 안 쌓는다 — 같은 탭을 두 번 눌러 뒤로 가기가 헛도는 일이 없게. */
export function needsNavigation<T extends string>(
	url: URL,
	tab: T,
	tabs: readonly T[],
	fallback: T
): boolean {
	return readTab(url, tabs, fallback) !== tab;
}
