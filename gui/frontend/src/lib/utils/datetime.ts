/**
 * DB 의 ISO-like 타임스탬프 ("2026-05-15 16:13:09" 또는 "2026-05-15T16:13:09Z") 를
 * 표시용으로 정리.
 *
 * - 공백 구분자 → `T` 로 정규화 후 Date 파싱.
 * - 잘못된 형식이면 원본 그대로 반환.
 * - 출력은 로컬 시간대의 `YYYY-MM-DD HH:mm` (초 생략 — 표시 영역 절약).
 */
export function formatTs(s: string | null | undefined): string {
	if (!s) return '';
	// "2026-05-15 16:13:09" → "2026-05-15T16:13:09" (Z 없으면 로컬 해석)
	const normalized = s.includes('T') ? s : s.replace(' ', 'T');
	const d = new Date(normalized);
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
	const normalized = s.includes('T') ? s : s.replace(' ', 'T');
	const d = new Date(normalized);
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
