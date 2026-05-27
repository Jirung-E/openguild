/**
 * BUG-025: 캠페인 정렬 옵션 — 캠페인 목록 / Home 카드 공유.
 *
 * 사용자가 캠페인 목록 페이지에서 정렬 선택 → localStorage 저장 →
 * Home 의 진행 중 / 곧 시작 카드도 같은 옵션으로 정렬.
 *
 * 길드별 분리는 후속 작업 (현재 단일 namespace).
 */

import type { Campaign, CampaignSummary } from '../types';

export type CampaignSortMode = 'recent' | 'remaining' | 'manual';

const SORT_KEY = 'openguild.campaignListSort';

export function loadCampaignSort(): CampaignSortMode {
	try {
		const v = localStorage.getItem(SORT_KEY);
		if (v === 'recent' || v === 'remaining' || v === 'manual') return v;
	} catch {
		/* SSR / private mode 무시 */
	}
	return 'recent';
}

export function saveCampaignSort(mode: CampaignSortMode): void {
	try {
		localStorage.setItem(SORT_KEY, mode);
	} catch {
		/* 무시 */
	}
}

/** Campaign / CampaignSummary 공통 — created_at / ended_at / display_order 만 사용. */
interface SortableCampaign {
	created_at: string;
	ended_at: string | null;
	display_order: number;
}

export function sortCampaigns<T extends SortableCampaign>(
	arr: T[],
	mode: CampaignSortMode,
	now: number = Date.now()
): T[] {
	const out = [...arr];
	if (mode === 'recent') {
		out.sort((a, b) => b.created_at.localeCompare(a.created_at));
	} else if (mode === 'remaining') {
		// 종료일이 가까운 순. 종료일 없으면 가장 뒤로.
		const remaining = (c: SortableCampaign): number => {
			if (!c.ended_at?.trim()) return Number.MAX_SAFE_INTEGER;
			const t = new Date(`${c.ended_at}T23:59:59`).getTime();
			return Number.isNaN(t) ? Number.MAX_SAFE_INTEGER : t - now;
		};
		out.sort((a, b) => remaining(a) - remaining(b));
	} else {
		// manual — display_order ASC, tie-break created_at DESC
		out.sort(
			(a, b) =>
				a.display_order - b.display_order ||
				b.created_at.localeCompare(a.created_at)
		);
	}
	return out;
}

// 타입 재export (Campaign / CampaignSummary 둘 다 SortableCampaign 호환).
export type { Campaign, CampaignSummary };
