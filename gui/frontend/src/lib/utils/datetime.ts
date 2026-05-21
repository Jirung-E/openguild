/**
 * DB / 파일 타임스탬프를 표시용으로 정리.
 *
 * 지원하는 입력 형식:
 * - Git 식 ISO 8601 + offset: `2026-05-22T13:41:10+09:00` (DEV-041 이후 새 데이터)
 * - ISO 8601 UTC: `2026-05-22T04:41:10Z`
 * - Legacy SQLite (공백 구분자, TZ 마커 없음): `2026-05-22 04:41:10`
 *   → UTC 로 가정 (SQLite `datetime('now')` 가 UTC 라서).
 *
 * 출력은 로컬 시간대의 `YYYY-MM-DD HH:mm` (초 생략).
 */

/**
 * 어떤 입력이든 JS Date 가 명확히 파싱할 수 있는 형식으로 정규화.
 *
 * - 이미 TZ 마커 (Z / ±HH:MM / ±HHMM) 있으면 그대로.
 * - 없으면 UTC 로 가정하고 `T` + `Z` 부여.
 */
function normalize(s: string): string {
	// "Z" 또는 ISO 끝에 "+HH:MM"/"-HH:MM"/"+HHMM"/"-HHMM" 가 붙어있는지.
	const hasTz = /(?:Z|[+-]\d{2}:?\d{2})$/.test(s);
	let body = s;
	if (!body.includes('T')) body = body.replace(' ', 'T');
	if (!hasTz) body = `${body}Z`; // 마커 없으면 UTC 가정.
	return body;
}

export function formatTs(s: string | null | undefined): string {
	if (!s) return '';
	const d = new Date(normalize(s));
	if (Number.isNaN(d.getTime())) return s;

	const yyyy = d.getFullYear();
	const mm = String(d.getMonth() + 1).padStart(2, '0');
	const dd = String(d.getDate()).padStart(2, '0');
	const hh = String(d.getHours()).padStart(2, '0');
	const mi = String(d.getMinutes()).padStart(2, '0');
	return `${yyyy}-${mm}-${dd} ${hh}:${mi}`;
}

/**
 * 상대 시간 표현 — "방금", "5분 전", "3시간 전", "2일 전", 그 이상은
 * `formatTs` 결과.
 *
 * Quest Detail / History 에서 absolute + relative 둘 다 보여줄 때 사용.
 */
export function formatRelative(s: string | null | undefined, now: Date = new Date()): string {
	if (!s) return '';
	const d = new Date(normalize(s));
	if (Number.isNaN(d.getTime())) return s;

	const diffMs = now.getTime() - d.getTime();
	const sec = Math.floor(diffMs / 1000);
	if (sec < 0) return formatTs(s); // 미래 — 절대값만
	if (sec < 60) return '방금';
	const min = Math.floor(sec / 60);
	if (min < 60) return `${min}분 전`;
	const hr = Math.floor(min / 60);
	if (hr < 24) return `${hr}시간 전`;
	const day = Math.floor(hr / 24);
	if (day < 7) return `${day}일 전`;
	return formatTs(s);
}
