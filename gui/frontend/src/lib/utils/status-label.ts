// DEV-015: status 표시 이름 — 언어 반응 fallback.
//
// 표시 사이트(board/detail/list/combobox 등)가 name_en 고정이던 것을 앱
// 언어에 따르게 통일. fallback 순서(DEV-015 메모 그대로):
//   사용자 기본 언어(ko 면 name_ko) → name_en (영원한 fallback).
// name_ko 는 선택 입력이라 빈 문자열일 수 있음 — 그땐 언어와 무관하게 en.
import type { Locale } from '$lib/stores/locale';

export function statusLabel(
	s: { name_en: string; name_ko?: string | null },
	loc: Locale
): string {
	if (loc === 'ko' && s.name_ko && s.name_ko.trim()) return s.name_ko;
	return s.name_en;
}

/** Quest/CampaignLinkedQuest 처럼 status_name_* 필드로 들고 있는 DTO 용. */
export function questStatusLabel(
	q: { status_name_en: string; status_name_ko?: string | null },
	loc: Locale
): string {
	return statusLabel({ name_en: q.status_name_en, name_ko: q.status_name_ko }, loc);
}
