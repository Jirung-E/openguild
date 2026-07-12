// DEV-015 (MVP): 영/한 언어 토글. theme.ts 와 동일 패턴.
//
// 범위: 이 store + t() 사전 + 토글 UI(SettingsQuickMenu) 까지가 1차 — 앱 전역
// 문자열(다른 컴포넌트/CLI/core/server 메시지) 스윕은 후속(DEV-205)에서 점진
// 확장. 영속: localStorage `openguild.locale`. 기본은 'ko' (현재 앱 기본과 동일
// — 기존 사용자 경험 변화 없음).

import { writable } from 'svelte/store';

export type Locale = 'ko' | 'en';

const KEY = 'openguild.locale';

function loadInitial(): Locale {
	if (typeof localStorage === 'undefined') return 'ko';
	try {
		const raw = localStorage.getItem(KEY);
		if (raw === 'ko' || raw === 'en') return raw;
		return 'ko';
	} catch {
		return 'ko';
	}
}

export const locale = writable<Locale>(loadInitial());

locale.subscribe((l) => {
	if (typeof localStorage === 'undefined') return;
	try {
		localStorage.setItem(KEY, l);
	} catch {
		/* 무시 */
	}
});

export function setLocale(l: Locale) {
	locale.set(l);
}

/**
 * DEV-015 (MVP): 번역 사전. 키는 의미 단위 — 컴포넌트가 늘어날 때마다 점진
 * 추가(DEV-205). 누락 키는 ko 텍스트를 그대로 반환(항상 안전한 fallback).
 */
const DICT: Record<string, { ko: string; en: string }> = {
	'settings.theme': { ko: '테마', en: 'Theme' },
	'settings.theme.system': { ko: '시스템', en: 'System' },
	'settings.theme.light': { ko: '라이트', en: 'Light' },
	'settings.theme.dark': { ko: '다크', en: 'Dark' },
	'settings.uiScale': { ko: 'UI 크기', en: 'UI Scale' },
	'settings.contentWidth': { ko: '컨텐츠 폭', en: 'Content Width' },
	'settings.language': { ko: '언어', en: 'Language' },
	'settings.all': { ko: '전체 설정 →', en: 'All settings →' },

	// DEV-205 / REQ-001: 퀘스트·캠페인 상세 공통 액션 + 섹션 라벨. 두 화면이
	// 영/한 혼재(예: Quest 'Edit'/'Sub-Quests' vs Campaign '편집'/'연결된 퀘스트')
	// 였던 것을 같은 사전 키로 통일 — 언어 토글에도 함께 전환.
	'detail.edit': { ko: '편집', en: 'Edit' },
	'detail.delete': { ko: '삭제', en: 'Delete' },
	'detail.back': { ko: '뒤로', en: 'Back' },
	'quest.section.parent': { ko: '부모', en: 'Parent' },
	'quest.section.subQuests': { ko: '서브퀘스트', en: 'Sub-Quests' },
	'quest.section.prerequisites': { ko: '선행 퀘스트', en: 'Prerequisites' },
	'quest.section.campaigns': { ko: '캠페인', en: 'Campaigns' },
	'quest.section.successors': { ko: '후속 퀘스트', en: 'Successors' },
	'quest.section.successorsHint': { ko: '이 퀘스트를 선행으로 가진 퀘스트', en: 'Quests that list this quest as a prerequisite' },
	'quest.section.tags': { ko: '태그', en: 'Tags' },

	// DEV-205(모듈4 선반영): 퀘스트 상세 태그 섹션 + 삭제 모달. 사용자 보고
	// (삭제 다이얼로그 라벨 미전환/버튼 순서 불일치, 태그 섹션 위치/색).
	'quest.tags.add': { ko: '+ 추가', en: '+ Add' },
	'quest.tags.remove': { ko: '태그 제거', en: 'Remove tag' },
	'quest.tags.none': { ko: '태그 없음.', en: 'No tags.' },
	'quest.tags.placeholder': { ko: '새 태그 (공백 구분으로 여러 개)', en: 'New tag (space-separated for multiple)' },
	'quest.tags.newAria': { ko: '새 태그', en: 'New tag' },
	'quest.tags.addSubmit': { ko: '추가', en: 'Add' },
	'quest.delete.msg': { ko: '이 퀘스트를 삭제합니다. 되돌릴 수 없습니다.', en: 'This quest will be deleted. This cannot be undone.' },
	'quest.delete.subTitle': { ko: '서브퀘스트 처리:', en: 'Sub-quest handling:' },
	'quest.delete.selectAll': { ko: '전체 선택', en: 'Select all' },
	'quest.delete.subHelp': { ko: '체크한 항목은 함께 삭제됩니다. 체크하지 않은 항목은 부모에서 분리됩니다.', en: 'Checked items are deleted too; unchecked items are detached from the parent.' },
	'quest.delete.prereqNote': { ko: '선행 퀘스트들은 별도의 퀘스트이므로 영향받지 않습니다.', en: 'Prerequisite quests are separate and are not affected.' },
	'quest.delete.deleting': { ko: '삭제 중…', en: 'Deleting…' },

	// DEV-205(모듈2 선반영): 캠페인 삭제 확인 다이얼로그.
	'campaign.deleteTitle': { ko: '캠페인 삭제', en: 'Delete campaign' },
	'campaign.deleteMsg1': { ko: '캠페인 "', en: 'Delete campaign "' },
	'campaign.deleteMsg2': { ko: '" 을(를) 삭제할까요?\n(soft delete — 복원 가능)', en: '"?\n(soft delete — restorable)' },
	'campaign.checklistDeleteTitle': { ko: '체크리스트 항목 삭제', en: 'Delete checklist item' },
	'campaign.checklistDeleteMsg': { ko: '이 체크리스트 항목을 삭제할까요?', en: 'Delete this checklist item?' },
	// DEV-205 모듈2: 캠페인 상세 나머지.
	'campaign.statusToggle': { ko: '클릭하여 상태 토글', en: 'Click to toggle status' },
	'campaign.banner': { ko: '배너', en: 'Banner' },
	'campaign.bannerRemove': { ko: '배너 제거', en: 'Remove banner' },
	'campaign.notFound': { ko: '캠페인 없음', en: 'Campaign not found' },
	'campaign.start': { ko: '시작', en: 'Start' },
	'campaign.end': { ko: '종료', en: 'End' },
	'campaign.bodyLabel': { ko: '본문 (Markdown) — 첨부는 드래그&드랍 / Ctrl+V 또는 아래 첨부 섹션', en: 'Body (Markdown) — attach via drag & drop / Ctrl+V or the attachments section below' },
	'campaign.noBody': { ko: '본문 없음.', en: 'No body.' },
	'campaign.addBody': { ko: '본문 추가', en: 'Add body' },
	'campaign.checklist': { ko: '체크리스트', en: 'Checklist' },
	'campaign.noItems': { ko: '항목 없음.', en: 'No items.' },
	'campaign.newChecklistItem': { ko: '새 체크리스트 항목...', en: 'New checklist item...' },
	'campaign.linkedQuests': { ko: '연결된 퀘스트', en: 'Linked quests' },
	'campaign.noLinkedQuests': { ko: '연결된 퀘스트 없음.', en: 'No linked quests.' },
	'campaign.unlinkQuest': { ko: '연결 해제', en: 'Unlink' },
	'campaign.linkQuest': { ko: '+ 퀘스트 연결', en: '+ Link quest' },
	'campaign.linkQuestTitle': { ko: '퀘스트 연결', en: 'Link quest' },
	'campaign.searchPlaceholder': { ko: 'ID 또는 제목으로 검색', en: 'Search by ID or title' },
	'campaign.attachFailed': { ko: '첨부 실패', en: 'Attachment failed' },
	'campaign.attachUploadFailed': { ko: '첨부 업로드 실패', en: 'Attachment upload failed' },
	'campaign.imageFilter': { ko: '이미지', en: 'Images' },
	'campaign.bannerSetFailed': { ko: '배너 설정 실패', en: 'Failed to set banner' },
	'campaign.bannerRemoveFailed': { ko: '배너 제거 실패', en: 'Failed to remove banner' },

	// DEV-205 모듈1: Nav 탭 + 액션.
	'nav.home': { ko: '홈', en: 'Home' },
	'nav.board': { ko: '퀘스트 보드', en: 'Quest Board' },
	'nav.list': { ko: '퀘스트 목록', en: 'Quest List' },
	'nav.admin': { ko: '관리', en: 'Admin' },
	'nav.rules': { ko: '규칙', en: 'Rules' },
	'nav.library': { ko: '도서관', en: 'Library' },
	'nav.settings': { ko: '설정', en: 'Settings' },
	'nav.currentGuild': { ko: '현재 길드', en: 'Current guild' },
	'nav.remote': { ko: '원격', en: 'Remote' },
	'nav.remoteConnected': { ko: '원격 서버에 연결됨', en: 'Connected to remote server' },
	'nav.reindex.hint': {
		ko: '캐시 정합 — 외부 편집 / git pull 후 한 번 클릭',
		en: 'Sync cache — click once after external edits / git pull'
	},
	'nav.reindex.done': { ko: '✓ Reindex 완료', en: '✓ Reindex done' },
	'nav.reindex.failed': { ko: 'Reindex 실패', en: 'Reindex failed' },
	'nav.reindex.error': { ko: 'reindex 실패', en: 'reindex failed' },

	// DEV-205 모듈1: 공통 확인 모달 기본 라벨.
	'common.confirm': { ko: '확인', en: 'Confirm' },
	'common.cancel': { ko: '취소', en: 'Cancel' },
	'common.no': { ko: '아니요', en: 'No' },
	'common.save': { ko: '저장', en: 'Save' },
	'common.saving': { ko: '저장…', en: 'Saving…' },
	'common.add': { ko: '추가', en: 'Add' },
	'common.created': { ko: '생성', en: 'Created' },
	'common.updated': { ko: '변경', en: 'Updated' },
	'common.doneMark': { ko: '✓ 완료', en: '✓ Done' },
	'common.countSuffix': { ko: '개', en: '' },
	// 우하단 floating 점프 버튼 (퀘스트/캠페인 상세 공통).
	'common.jumpTop': { ko: '맨 위로', en: 'Back to top' },
	'common.jumpTopShort': { ko: '위', en: 'Top' },
	'common.jumpComments': { ko: '댓글로 이동', en: 'Jump to comments' },
	'common.jumpCommentsShort': { ko: '댓글', en: 'Comments' },
	'common.jumpMemo': { ko: '메모로 이동', en: 'Jump to memo' },
	'common.jumpMemoShort': { ko: '메모', en: 'Memo' },

	// DEV-205 모듈1: Welcome 페이지.
	'welcome.sub': { ko: '최근 작업한 길드', en: 'Recently opened guilds' },
	'welcome.opening': { ko: '여는 중…', en: 'Opening…' },
	'welcome.pickFolder': { ko: '📁 폴더에서 열기', en: '📁 Open from folder' },
	'welcome.pickHint': {
		ko: '기존 길드 폴더를 선택하면 바로 열고, 길드가 아닌 폴더면 초기화 안내가 표시됩니다.',
		en: 'Pick an existing guild folder to open it, or a non-guild folder to see init guidance.'
	},
	'welcome.remotePlaceholder': {
		ko: '원격 서버 주소 — http://192.168.1.10:3000',
		en: 'Remote server address — http://192.168.1.10:3000'
	},
	'welcome.remoteAria': { ko: '원격 서버 URL', en: 'Remote server URL' },
	'welcome.checking': { ko: '확인 중…', en: 'Checking…' },
	'welcome.checkConn': { ko: '연결 확인', en: 'Check connection' },
	'welcome.connect': { ko: '연결', en: 'Connect' },
	'welcome.connOk': { ko: '✓ 연결 확인됨.', en: '✓ Connection verified.' },
	'welcome.connFail': { ko: '연결 실패', en: 'Connection failed' },
	'welcome.remoteHint1': { ko: 'openguild-server 의 주소. 연결하면 아래 "최근 길드" 목록에 등록되어 다음부터는 클릭만으로 다시 열 수 있습니다. ', en: 'The openguild-server address. Once connected it is added to the "recent guilds" list below so you can reopen it with a single click. ' },
	'welcome.remoteHintStrong': { ko: '인증이 없으니 신뢰된 네트워크에서만', en: 'No authentication — use only on trusted networks' },
	'welcome.remoteHint2': { ko: ' 사용하세요.', en: '.' },
	'welcome.uninitTitle': { ko: '이 위치를 길드로 초기화할까요?', en: 'Initialize this location as a guild?' },
	'welcome.uninitDesc1': { ko: '지정한 디렉토리에 openguild 마커 파일(', en: 'The selected directory has no openguild marker file (' },
	'welcome.uninitDesc2': { ko: ')이 없습니다. 초기화하면 마커 + ', en: '). Initializing creates the marker + ' },
	'welcome.uninitDesc3': { ko: ' 데이터 폴더가 생성되어 바로 작업할 수 있습니다.', en: ' data folder so you can start working right away.' },
	'welcome.guildName': { ko: '길드 이름', en: 'Guild name' },
	'welcome.initializing': { ko: '초기화 중…', en: 'Initializing…' },
	'welcome.initAndOpen': { ko: '초기화하고 열기', en: 'Initialize and open' },
	'welcome.loading': { ko: '불러오는 중...', en: 'Loading...' },
	'welcome.browserInfo1': { ko: 'Recent guild 목록은 desktop 앱 (Tauri) 에서만 동작합니다.', en: 'The recent-guild list works only in the desktop app (Tauri).' },
	'welcome.browserInfo2': { ko: '브라우저 모드에선 현재 server 가 호스팅한 길드만 표시됩니다.', en: 'In browser mode, only the guild hosted by the current server is shown.' },
	'welcome.empty1': { ko: '아직 열어본 길드가 없습니다.', en: 'No guilds opened yet.' },
	'welcome.empty2': { ko: ' 으로 새 길드를 만들거나, ', en: ' to create a new guild, ' },
	'welcome.empty3': { ko: ' 로 기존 길드를 열거나, 위에서 원격 서버에 연결해 보세요.', en: ' to open an existing one, or connect to a remote server above.' },
	'welcome.pathMissing': { ko: '경로를 찾을 수 없습니다 — 이동 / 삭제됐을 수 있음', en: 'Path not found — may have been moved / deleted' },
	'welcome.openInWindow': { ko: '현재 창에서 이 길드를 엽니다', en: 'Opens this guild in the current window' },
	'welcome.pathNotFound': { ko: '⚠ 경로를 찾을 수 없습니다', en: '⚠ Path not found' },
	'welcome.guildOpening': { ko: '길드 여는 중…', en: 'Opening guild…' },
	'welcome.checkingConn': { ko: '연결 확인 중…', en: 'Checking connection…' },
	'welcome.connectRemote': { ko: '이 원격 서버에 연결합니다', en: 'Connects to this remote server' },
	'welcome.serverUnreachable': { ko: '서버에 연결할 수 없습니다', en: 'Cannot reach server' },
	'welcome.serverUnreachableWarn': { ko: '⚠ 서버에 연결할 수 없습니다', en: '⚠ Cannot reach server' },
	'welcome.removeFromList': { ko: '목록에서 제거', en: 'Remove from list' },
	'welcome.clearAll': { ko: '전체 비우기', en: 'Clear all' },
	'welcome.footerHint': { ko: '항목을 클릭하면 현재 창에서 그 길드를 엽니다.', en: 'Click an item to open that guild in the current window.' },
	'welcome.clearTitle': { ko: '최근 길드 목록 비우기', en: 'Clear recent guilds' },
	'welcome.clearMsg': { ko: '최근 길드 목록(로컬 + 원격)을 모두 비울까요? 되돌릴 수 없습니다.', en: 'Clear the entire recent-guild list (local + remote)? This cannot be undone.' },
	'welcome.clearConfirm': { ko: '비우기', en: 'Clear' },
	'welcome.removeTitle': { ko: '최근 길드에서 제거', en: 'Remove from recent guilds' },
	'welcome.removeSuffix': { ko: ' 을(를) 최근 목록에서 제거할까요?', en: ' — remove from the recent list?' },
	'welcome.removeNoteLocal': { ko: '디스크의 길드 파일은 그대로 두고, Recent 목록에서만 빠집니다.', en: 'The guild files on disk are kept; only the recent list entry is removed.' },
	'welcome.removeNoteRemote': { ko: '서버 연결 자체에는 영향 없고, Recent 목록에서만 빠집니다.', en: 'The server connection is unaffected; only the recent list entry is removed.' },
	'welcome.remove': { ko: '제거', en: 'Remove' },
	'welcome.incompatTitle': { ko: '호환되지 않는 길드', en: 'Incompatible guild' },
	'welcome.updateCheck': { ko: '업데이트 확인', en: 'Check for updates' },
	'welcome.tauriOnly': { ko: 'Tauri 데스크톱 앱에서만 동작합니다.', en: 'Works only in the Tauri desktop app.' },
	'welcome.enterGuildName': { ko: '길드 이름을 입력하세요.', en: 'Please enter a guild name.' },
	'welcome.pickDialogTitle': { ko: '길드 폴더 선택', en: 'Select guild folder' },
	'welcome.badResponse': { ko: '서버가 응답했지만 예상한 형식이 아닙니다.', en: 'The server responded but not in the expected format.' },
	'welcome.notValidDir': { ko: '선택된 경로가 유효한 디렉토리가 아닙니다', en: 'Selected path is not a valid directory' },
	'welcome.markerExample': { ko: '이름.guild', en: 'name.guild' },

	// DEV-205 모듈2: 홈(Home) 대시보드.
	'home.activeCampaigns': { ko: '진행 중 캠페인', en: 'Active campaigns' },
	'home.upcomingCampaigns': { ko: '곧 시작되는 캠페인', en: 'Upcoming campaigns' },
	'home.overdueCampaigns': { ko: '마감 지난 캠페인', en: 'Overdue campaigns' },
	'home.overdueCampaignsEmpty': { ko: '마감 지난 캠페인 없음.', en: 'No overdue campaigns.' },
	'home.campaignList': { ko: '캠페인 목록', en: 'Campaign list' },
	'home.addCampaign': { ko: '+ 캠페인 추가', en: '+ Add campaign' },
	'home.overdueQuests': { ko: '마감 지난 퀘스트', en: 'Overdue quests' },
	'home.discussionComments': { ko: '토론 댓글', en: 'Discussion comments' },
	'home.imminentQuests': { ko: '마감 임박 퀘스트', en: 'Due soon' },
	'home.recentQuests': { ko: '최근 추가/수정된 퀘스트', en: 'Recently added / updated quests' },
	'home.noQuests': { ko: '아직 퀘스트가 없습니다.', en: 'No quests yet.' },

	// DEV-205 모듈2: 캠페인 목록 페이지.
	'campaignList.title': { ko: '캠페인', en: 'Campaigns' },
	'campaignList.new': { ko: '+ 새 캠페인', en: '+ New campaign' },
	'campaignList.statusLabel': { ko: '상태', en: 'Status' },
	'campaignList.statusAll': { ko: '전체', en: 'All' },
	'campaignList.statusActive': { ko: '진행 중', en: 'Active' },
	'campaignList.statusDone': { ko: '완료', en: 'Done' },
	'campaignList.sortLabel': { ko: '정렬', en: 'Sort' },
	'campaignList.sortRecent': { ko: '최근 추가 순', en: 'Recently added' },
	'campaignList.sortRemaining': { ko: '남은 날짜 순', en: 'Time remaining' },
	'campaignList.sortManual': { ko: '수동 (display_order)', en: 'Manual (display_order)' },
	'campaignList.empty': { ko: '캠페인 없음.', en: 'No campaigns.' },
	'campaignList.periodUndefined': { ko: '기간 미정', en: 'No period set' },
	'campaignList.orderChangeFailed': { ko: 'order 변경 실패', en: 'Failed to change order' },

	// DEV-205 모듈2: 홈 카드/컨베이어 자식 컴포넌트.
	'common.done': { ko: '완료', en: 'Done' },
	'card.noChecklist': { ko: '체크리스트 없음', en: 'No checklist' },
	'card.noLinkedQuests': { ko: '링크된 퀘스트 없음', en: 'No linked quests' },
	'card.checkProgress': { ko: '체크리스트 진행률', en: 'Checklist progress' },
	'card.questProgress': { ko: '링크된 퀘스트의 완료 비율', en: 'Completion of linked quests' },
	'card.check': { ko: '체크', en: 'Check' },
	'card.quest': { ko: '퀘스트', en: 'Quests' },
	'card.justPassed': { ko: '방금 지남', en: 'just now' },
	'card.hoursPassed': { ko: '시간 지남', en: 'h ago' },
	'card.daysPassed': { ko: '일 지남', en: 'd ago' },
	'carousel.activeEmpty': { ko: '진행 중인 캠페인이 없습니다.', en: 'No active campaigns.' },
	'carousel.active': { ko: '진행 중 캠페인', en: 'Active campaigns' },
	'carousel.prev': { ko: '이전', en: 'Previous' },
	'carousel.next': { ko: '다음', en: 'Next' },
	'carousel.play': { ko: '재생', en: 'Play' },
	'carousel.pause': { ko: '정지', en: 'Pause' },
	'carousel.autoPlay': { ko: '자동 회전 재생', en: 'Resume auto-rotate' },
	'carousel.autoPause': { ko: '자동 회전 정지', en: 'Pause auto-rotate' },
	'conveyor.upcomingEmpty': { ko: '곧 시작 예정인 캠페인이 없습니다.', en: 'No upcoming campaigns.' },
	'conveyor.overdueCampaigns': { ko: '마감 지난 캠페인', en: 'Overdue campaigns' },
	'conveyor.upcomingCampaigns': { ko: '곧 시작 캠페인', en: 'Upcoming campaigns' },
	'conveyor.overdueQuests': { ko: '마감 지남 퀘스트', en: 'Overdue quests' },
	'conveyor.imminentQuests': { ko: '마감 임박 퀘스트', en: 'Due-soon quests' },

	// DEV-205 모듈2: 작업 기록 요약 카드(홈). worklog 상세는 모듈5/후속.
	'worklogCard.detailTitle': { ko: '작업 기록 상세', en: 'Worklog detail' },
	'worklogCard.title': { ko: '🕘 작업 기록', en: '🕘 Worklog' },
	'worklogCard.rangePre': { ko: '최근 ', en: 'Last ' },
	'worklogCard.rangeWeeks': { ko: '주 · 총 ', en: ' weeks · ' },
	'worklogCard.rangeActivities': { ko: ' 활동 ›', en: ' activities ›' },
	'worklogCard.heatmapAria': { ko: '최근 활동 히트맵', en: 'Recent activity heatmap' },
	'worklogCard.today': { ko: '오늘', en: 'Today' },
	'worklogCard.statusChanges': { ko: '상태변경', en: 'status changes' },
	'worklogCard.comments': { ko: '댓글', en: 'comments' },
	'worklogCard.created': { ko: '생성', en: 'created' },
	'worklogCard.lastActivity': { ko: '마지막 활동 ', en: 'Last activity ' },
	'worklogCard.activityUnit': { ko: '활동 ', en: '' },
	'worklogCard.activityCount': { ko: '건', en: '' },

	// DEV-205: 상세 공통 섹션(첨부/이력/메모) — 퀘스트·캠페인 공유 컴포넌트.
	'attach.title': { ko: '첨부파일', en: 'Attachments' },
	'attach.add': { ko: '+ 첨부', en: '+ Attach' },
	'attach.processing': { ko: '처리 중…', en: 'Processing…' },
	'attach.downloadAll': { ko: '모든 첨부 다운로드', en: 'Download all attachments' },
	'attach.downloadAllBtn': { ko: '전체 다운로드', en: 'Download all' },
	'attach.empty': { ko: "첨부 없음. '+ 첨부' 로 이미지·동영상·파일을 추가하세요.", en: "No attachments. Use '+ Attach' to add images, videos, or files." },
	'attach.openPreview': { ko: '열기 / 미리보기', en: 'Open / preview' },
	'attach.remove': { ko: '목록에서 제거', en: 'Remove from list' },
	'attach.removeAria': { ko: '제거', en: 'Remove' },
	'attach.download': { ko: '다운로드', en: 'Download' },
	'attach.saveDir': { ko: '첨부 저장 폴더', en: 'Attachment save folder' },
	'attach.openFailed': { ko: '열기 실패', en: 'Failed to open' },
	'attach.downloadFailed': { ko: '다운로드 실패', en: 'Download failed' },
	'attach.downloadAllFailed': { ko: '전체 다운로드 실패', en: 'Download all failed' },

	'history.title': { ko: '변경 이력', en: 'Change history' },
	'history.loading': { ko: '로드 중…', en: 'Loading…' },
	'history.empty': { ko: '변경 이력 없음.', en: 'No change history.' },
	'history.loadFailed': { ko: '이력 로드 실패', en: 'Failed to load history' },
	'history.none': { ko: '(없음)', en: '(none)' },

	'note.heading': { ko: '메모 (Memo)', en: 'Memo' },
	'note.emptyAction': { ko: '메모 작성', en: 'Write memo' },
	'note.emptyHint': { ko: '개인 메모. gitignored (팀 공유 X).', en: 'Private memo. Gitignored (not shared with the team).' },
	'note.help': { ko: '본인만 보는 비공개 메모 (`.guild/quests/{slug}.memo.md`, gitignored).', en: 'A private memo only you can see (`.guild/quests/{slug}.memo.md`, gitignored).' },
	'note.expand': { ko: '메모 펼치기', en: 'Expand memo' },
	'note.collapse': { ko: '메모 접기', en: 'Collapse memo' },
	'note.heightExpand': { ko: '메모를 전체 높이로 펼치기 (확장)', en: 'Expand memo to full height' },
	'note.heightFixed': { ko: '메모를 고정 높이 + 스크롤로 (고정)', en: 'Fixed height with scroll' },
	'note.expandBtn': { ko: '⤢ 확장', en: '⤢ Expand' },
	'note.fixBtn': { ko: '⊟ 고정', en: '⊟ Fixed' },
	'note.helpAttach': { ko: '(이미지·동영상은 드래그&드랍 또는 Ctrl+V 로 첨부)', en: '(attach images/videos via drag & drop or Ctrl+V)' }
};

/** 현재 locale 기준 번역. 누락 키는 ko 원문 그대로(안전한 fallback). */
export function t(key: string, l: Locale): string {
	const entry = DICT[key];
	if (!entry) return key;
	return l === 'en' ? entry.en : entry.ko;
}
