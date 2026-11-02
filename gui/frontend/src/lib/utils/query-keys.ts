// DEV-447 · DEV-452 · REQ-040: 쿼리 쓰는 법에 보일 키 — [키, 예, 설명 번역 키]. 값의 예는 CLI `-q` 와 같다.
// REQ-040: 목록 화면과 검색 페이지가 같이 쓴다(한쪽만 고쳐져 갈라지지 않게).

export type QueryKeyRow = readonly [string, string, string];

export const QUEST_QUERY_KEYS: readonly QueryKeyRow[] = [
	['id', 'id:DEV-1..=DEV-9', 'query.helpId'],
	['status', 'status:[open, in_progress]', 'filter.queryHelpStatus'],
	['type', 'type:BUG', 'filter.queryHelpType'],
	['tag', 'tag:ui-*', 'filter.queryHelpTag'],
	['campaign', 'campaign:C-001', 'filter.queryHelpCampaign'],
	['urgency', 'urgency:1..=2', 'filter.queryHelpUrgency'],
	['due', 'due:2026-11-01..2026-12-01', 'filter.queryHelpDue'],
	['required_due', 'required_due:overdue', 'filter.queryHelpRequiredDue'],
	['desired_due', 'desired_due:overdue', 'filter.queryHelpDesiredDue'],
	['campaign_end', 'campaign_end:overdue', 'filter.queryHelpCampaignEnd'],
	['parent', 'parent:none', 'filter.queryHelpParent'],
	['child', 'child:none', 'filter.queryHelpChild'],
	['prereq', 'prereq:none', 'filter.queryHelpPrereq'],
	['successor', 'successor:DEV-001', 'filter.queryHelpSuccessor'],
	['discussion', 'discussion:unresolved', 'filter.queryHelpDiscussion'],
	['created', 'created:2026-10-01..', 'query.helpCreated'],
	['updated', 'updated:>=2026-10-20', 'filter.queryHelpUpdated'],
	['commented', 'commented:>=2026-10-20', 'filter.queryHelpCommented'],
	['title', 'title:="로그인 화면"', 'query.helpTitle'],
	['body', 'body:/TODO|FIXME/', 'query.helpBody']
];

export const CAMPAIGN_QUERY_KEYS: readonly QueryKeyRow[] = [
	['id', 'id:C-001', 'query.helpId'],
	['status', 'status:active', 'campaignList.queryHelpStatus'],
	['start', 'start:>=2026-10-01', 'campaignList.queryHelpStart'],
	['end', 'end:overdue', 'campaignList.queryHelpEnd'],
	['quest', 'quest:DEV-001', 'campaignList.queryHelpQuest'],
	['checklist', 'checklist:open', 'campaignList.queryHelpChecklist'],
	['created', 'created:2026-10-01..', 'query.helpCreated'],
	['updated', 'updated:>=2026-10-20', 'query.helpUpdated'],
	['title', 'title:출시*', 'query.helpTitle'],
	['body', 'body:/v\\d+/', 'query.helpBody']
];

export const COMMENT_QUERY_KEYS: readonly QueryKeyRow[] = [
	['id', 'id:>=2', 'comments.queryHelpId'],
	['quest', 'quest:DEV-1..=DEV-9', 'comments.queryHelpQuest'],
	['campaign', 'campaign:C-001', 'comments.queryHelpCampaign'],
	['author', 'author:claude', 'comments.queryHelpAuthor'],
	['parent', 'parent:none', 'comments.queryHelpParent'],
	['discussion', 'discussion:unresolved', 'comments.queryHelpDiscussion'],
	['created', 'created:2026-10-01..', 'query.helpCreated'],
	['updated', 'updated:>=2026-10-20', 'query.helpUpdated'],
	['body', 'body:/TODO|FIXME/', 'query.helpBody']
];

export const LIBRARY_QUERY_KEYS: readonly QueryKeyRow[] = [
	['id', 'id:BOOK-001', 'query.helpId'],
	['tag', 'tag:설계', 'library.queryHelpTag'],
	['folder', '-folder:보관', 'library.queryHelpFolder'],
	['created', 'created:2026-10-01..', 'query.helpCreated'],
	['updated', 'updated:2026-10-01..', 'library.queryHelpUpdated'],
	['title', 'title:설계*', 'query.helpTitle'],
	['body', 'body:/^# /', 'query.helpBody']
];
