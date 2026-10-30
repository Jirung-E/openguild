// DEV-447 · DEV-452: 쿼리 쓰는 법에 보일 키 — [키, 예, 설명 번역 키]. 값의 예는 CLI `-q` 와 같다.
// REQ-040: 목록 화면과 검색 페이지가 같이 쓴다(한쪽만 고쳐져 갈라지지 않게).

export type QueryKeyRow = readonly [string, string, string];

export const QUEST_QUERY_KEYS: readonly QueryKeyRow[] = [
	['status', 'status:open,in_progress', 'filter.queryHelpStatus'],
	['type', 'type:BUG', 'filter.queryHelpType'],
	['tag', '-tag:wip', 'filter.queryHelpTag'],
	['campaign', 'campaign:C-001', 'filter.queryHelpCampaign'],
	['urgency', 'urgency:<=2', 'filter.queryHelpUrgency'],
	['due', 'due:overdue', 'filter.queryHelpDue'],
	['parent', 'parent:none', 'filter.queryHelpParent'],
	['commented', 'commented:>=2026-10-20', 'filter.queryHelpCommented']
];

export const CAMPAIGN_QUERY_KEYS: readonly QueryKeyRow[] = [
	['status', 'status:active', 'campaignList.queryHelpStatus'],
	['start', 'start:>=2026-10-01', 'campaignList.queryHelpStart'],
	['end', 'end:overdue', 'campaignList.queryHelpEnd'],
	['quest', 'quest:DEV-001', 'campaignList.queryHelpQuest']
];

export const LIBRARY_QUERY_KEYS: readonly QueryKeyRow[] = [
	['tag', 'tag:설계', 'library.queryHelpTag'],
	['folder', '-folder:보관', 'library.queryHelpFolder'],
	['updated', 'updated:>=2026-10-01', 'library.queryHelpUpdated']
];
