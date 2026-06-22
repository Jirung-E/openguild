/**
 * DEV-093 fix2: 캠페인 "완료" 판정 — 체크리스트 + 연결 quest 양쪽 100% 일 때만.
 *
 * 이전 버그: `completed` 가 checklist 100% 만 판정했음 → 체크리스트는 다
 * 끝났지만 연결 quest 가 아직 done 이 아닐 때도 "✓ 완료" 표시 + overdue 필터
 * 에서 사라지던 문제.
 *
 * 새 규칙:
 *   - 체크리스트 + 연결 quest 둘 다 있음 → 둘 다 100% 일 때만 완료.
 *   - 체크리스트만 있음           → 체크리스트 100%.
 *   - 연결 quest 만 있음          → quest 100%.
 *   - 둘 다 없음                  → 완료 판정 안 함 (정보 부족 → false).
 *
 * `CampaignCard.completed` 와 `Home.overdueCampaigns` 가 공유.
 */
export interface CampaignProgressShape {
	checklist_total: number;
	checklist_checked: number;
	quest_total?: number | null;
	quest_done?: number | null;
}

export function isCampaignDone(c: CampaignProgressShape): boolean {
	const hasChecklist = c.checklist_total > 0;
	const hasQuests = (c.quest_total ?? 0) > 0;
	if (!hasChecklist && !hasQuests) return false;
	const checklistDone = hasChecklist && c.checklist_checked === c.checklist_total;
	const questsDone = hasQuests && (c.quest_done ?? 0) === (c.quest_total ?? 0);
	if (hasChecklist && hasQuests) return checklistDone && questsDone;
	if (hasChecklist) return checklistDone;
	return questsDone; // hasQuests 만 남음
}
