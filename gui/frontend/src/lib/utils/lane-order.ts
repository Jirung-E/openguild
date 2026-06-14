// DEV-059: 사용자 정의 status 레인 순서 — '보여지는 순서' 만. 파일 / DB / 다른
// quest 영향 X. 보드(QuestBoard)에서 ◀▶ 로 바꾼 순서를 localStorage 에 길드별로
// 저장한다.
//
// 이 헬퍼로 추출한 이유(상태순서 통일): 이전엔 loadLaneOrder/saveLaneOrder 가
// QuestBoard 내부에만 있어, 상세페이지 status 드롭다운은 DB sort_order 만 따랐다.
// 그래서 보드에서 바꾼 순서가 상세에 반영되지 않는 불일치가 있었다. 공유 헬퍼로
// 빼서 상세 드롭다운도 같은 laneOrder 를 따르게 한다(없으면 sort_order fallback).

import { guildKey } from './guild-storage';

const LANE_ORDER_SUFFIX = 'laneOrder';

/** 저장된 레인 순서(status slug 배열). 없거나 손상 시 빈 배열. */
export function loadLaneOrder(prefix: string): string[] {
	try {
		const raw = localStorage.getItem(guildKey(prefix, LANE_ORDER_SUFFIX));
		if (!raw) return [];
		const arr = JSON.parse(raw);
		return Array.isArray(arr) ? arr.filter((s) => typeof s === 'string') : [];
	} catch {
		return [];
	}
}

/** 레인 순서 저장(status slug 배열). */
export function saveLaneOrder(prefix: string, slugs: string[]): void {
	try {
		localStorage.setItem(guildKey(prefix, LANE_ORDER_SUFFIX), JSON.stringify(slugs));
	} catch {
		/* quota / disabled — 무시 */
	}
}

/**
 * status 배열을 laneOrder 우선으로 정렬. laneOrder 에 있는 slug 는 그 순서대로,
 * 없는 slug(레인 순서 변경 이후 추가된 status 등)는 fallback 비교자(보통
 * sort_order) 순으로 뒤에 붙는다. laneOrder 가 비어 있으면 fallback 만 적용.
 */
export function orderStatusesByLane<T extends { slug: string }>(
	statuses: T[],
	laneOrder: string[],
	fallbackCmp: (a: T, b: T) => number
): T[] {
	if (laneOrder.length === 0) return [...statuses].sort(fallbackCmp);
	const rank = new Map(laneOrder.map((s, i) => [s, i] as const));
	return [...statuses].sort((a, b) => {
		const ra = rank.get(a.slug) ?? Infinity;
		const rb = rank.get(b.slug) ?? Infinity;
		if (ra !== rb) return ra - rb;
		return fallbackCmp(a, b);
	});
}
