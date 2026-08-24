/**
 * BUG-245: "최근 길드" 목록 정렬.
 *
 * 두 소스가 섞인다 — 로컬 recents 는 Rust 가 만드는 **초 단위**
 * (`2026-08-17T00:12:33Z`), 원격 길드는 JS `toISOString()` 의 **밀리초**
 * (`2026-08-17T00:12:33.123Z`). 문자열로 비교하면 `'.'`(0x2E) < `'Z'`(0x5A)
 * 이라 같은 초에 기록된 원격 항목이 로컬보다 오래된 것으로 밀린다.
 *
 * 또 예전 비교자(`a < b ? 1 : -1`)는 같은 값에도 `-1` 을 돌려주는 비일관
 * 비교자였다 — 값이 같은 항목이 있으면 정렬 결과가 구현 정의로 흐트러진다.
 */
export interface HasLastOpened {
	last_opened: string;
}

/** 최근이 먼저. 파싱 불가한 값은 맨 뒤로. */
export function byLastOpenedDesc(a: HasLastOpened, b: HasLastOpened): number {
	const ta = Date.parse(a.last_opened);
	const tb = Date.parse(b.last_opened);
	const aBad = Number.isNaN(ta);
	const bBad = Number.isNaN(tb);
	if (aBad && bBad) return 0;
	if (aBad) return 1;
	if (bBad) return -1;
	return tb - ta;
}
