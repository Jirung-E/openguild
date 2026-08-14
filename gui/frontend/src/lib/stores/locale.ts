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
	// DEV-335: 첨부 이미지 HDR 표시 제한.
	'settings.hdrLimit': { ko: '첨부 이미지 HDR', en: 'Attachment image HDR' },
	'settings.hdrLimitOn': { ko: '사용', en: 'On' },
	'settings.hdrLimitConstrained': { ko: '제한', en: 'Constrained' },
	'settings.hdrLimitOff': { ko: '끄기', en: 'Off' },
	'settings.hdrLimitHint': {
		ko: '끄면(제한/끄기) HDR 화면에서도 이미지가 과도하게 밝게 보이지 않습니다. 지원하는 브라우저에서만 표시됩니다.',
		en: 'Constrained/off keeps images from looking overly bright on HDR displays. Only shown when the browser supports it.'
	},
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
	'quest.section.parent': { ko: '부모 퀘스트', en: 'Parent Quest' },
	'quest.section.subQuests': { ko: '서브퀘스트', en: 'Sub-Quests' },
	'quest.section.prerequisites': { ko: '선행 퀘스트', en: 'Prerequisites' },
	'quest.section.campaigns': { ko: '캠페인', en: 'Campaigns' },
	'quest.section.successors': { ko: '후속 퀘스트', en: 'Successors' },
	'quest.section.successorsHint': {
		ko: '이 퀘스트를 선행으로 가진 퀘스트',
		en: 'Quests that list this quest as a prerequisite'
	},
	'quest.section.tags': { ko: '태그', en: 'Tags' },

	// DEV-205(모듈4 선반영): 퀘스트 상세 태그 섹션 + 삭제 모달. 사용자 보고
	// (삭제 다이얼로그 라벨 미전환/버튼 순서 불일치, 태그 섹션 위치/색).
	'quest.tags.add': { ko: '+ 추가', en: '+ Add' },
	'quest.tags.remove': { ko: '태그 제거', en: 'Remove tag' },
	'quest.tags.none': { ko: '태그 없음.', en: 'No tags.' },
	'quest.tags.placeholder': {
		ko: '새 태그 (공백 구분으로 여러 개)',
		en: 'New tag (space-separated for multiple)'
	},
	'quest.tags.newAria': { ko: '새 태그', en: 'New tag' },
	'quest.tags.addSubmit': { ko: '추가', en: 'Add' },
	'quest.delete.msg': {
		ko: '이 퀘스트를 삭제합니다. 되돌릴 수 없습니다.',
		en: 'This quest will be deleted. This cannot be undone.'
	},
	'quest.delete.subTitle': { ko: '서브퀘스트 처리:', en: 'Sub-quest handling:' },
	'quest.delete.selectAll': { ko: '전체 선택', en: 'Select all' },
	'quest.delete.subHelp': {
		ko: '체크한 항목은 함께 삭제됩니다. 체크하지 않은 항목은 부모에서 분리됩니다.',
		en: 'Checked items are deleted too; unchecked items are detached from the parent.'
	},
	'quest.delete.prereqNote': {
		ko: '선행 퀘스트들은 별도의 퀘스트이므로 영향받지 않습니다.',
		en: 'Prerequisite quests are separate and are not affected.'
	},
	'quest.delete.deleting': { ko: '삭제 중…', en: 'Deleting…' },

	// DEV-205(모듈2 선반영): 캠페인 삭제 확인 다이얼로그.
	'campaign.deleteTitle': { ko: '캠페인 삭제', en: 'Delete campaign' },
	'campaign.deleteMsg1': { ko: '캠페인 "', en: 'Delete campaign "' },
	'campaign.deleteMsg2': {
		ko: '" 을(를) 삭제할까요?\n(soft delete — 복원 가능)',
		en: '"?\n(soft delete — restorable)'
	},
	'campaign.checklistDeleteTitle': { ko: '체크리스트 항목 삭제', en: 'Delete checklist item' },
	'campaign.checklistDeleteMsg': {
		ko: '이 체크리스트 항목을 삭제할까요?',
		en: 'Delete this checklist item?'
	},
	// DEV-205 모듈2: 캠페인 상세 나머지.
	'campaign.statusToggle': { ko: '클릭하여 상태 토글', en: 'Click to toggle status' },
	'campaign.banner': { ko: '배너', en: 'Banner' },
	'campaign.bannerRemove': { ko: '배너 제거', en: 'Remove banner' },
	'campaign.notFound': { ko: '캠페인 없음', en: 'Campaign not found' },
	'campaign.start': { ko: '시작', en: 'Start' },
	'campaign.end': { ko: '종료', en: 'End' },
	'campaign.bodyLabel': {
		ko: '본문 (Markdown) — 첨부는 드래그&드랍 / Ctrl+V 또는 아래 첨부 섹션',
		en: 'Body (Markdown) — attach via drag & drop / Ctrl+V or the attachments section below'
	},
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
	'nav.more': { ko: '더 보기', en: 'More' },
	// DEV-266: 알림 스택 상한 초과 축약 칩 — {n} 은 렌더 시 치환.
	'notif.more': { ko: '+{n}개 더 보기', en: '+{n} more' },
	'nav.settingsQuickMenu': { ko: '설정 퀵메뉴', en: 'Settings quick menu' },

	// DEV-205 모듈5: 설정 페이지.
	'settings.title': { ko: '설정', en: 'Settings' },
	'settings.tabInfo': { ko: '정보', en: 'Info' },
	'settings.tabDisplay': { ko: '표시', en: 'Display' },
	'settings.tabEditor': { ko: '편집기', en: 'Editor' },
	'settings.editorHeading': { ko: '편집기', en: 'Editor' },
	'settings.tabBehavior': { ko: 'Tab 동작', en: 'Tab behavior' },
	'settings.tabChar': { ko: '탭 문자', en: 'Tab character' },
	'settings.tabSpace': { ko: '공백', en: 'Space' },
	'settings.indentSize': { ko: '들여쓰기 칸수', en: 'Indent size' },
	// DEV-336: 목록 이어쓰기 / Enter 자동 들여쓰기 / 재들여쓰기 통합 토글.
	'settings.autoFormat': { ko: '자동 서식', en: 'Auto-formatting' },
	'settings.autoFormatHint': {
		ko: '끄면 목록·인용 이어쓰기, Enter 시 자동 들여쓰기, 타이핑 중 재들여쓰기가 모두 꺼집니다.',
		en: 'When off, list/quote continuation, auto-indent on Enter, and re-indent while typing are all disabled.'
	},
	'settings.infoHeading': { ko: '정보', en: 'Info' },
	'settings.appName': { ko: '앱 이름', en: 'App name' },
	'settings.version': { ko: '버전', en: 'Version' },
	'settings.checking': { ko: '확인 중…', en: 'Checking…' },
	'settings.checkUpdate': { ko: '업데이트 확인', en: 'Check for updates' },
	// DEV-305: 업데이트 자동 확인 on/off.
	'settings.autoUpdateCheck': { ko: '업데이트 자동 확인', en: 'Automatic update check' },
	'settings.autoUpdateCheckHint': {
		ko: '끄면 시작할 때와 주기적으로 확인하지 않습니다. 위 「업데이트 확인」은 계속 쓸 수 있습니다.',
		en: 'When off, no check on startup or periodically. The button above still works.'
	},
	'settings.on': { ko: '켜기', en: 'On' },
	'settings.off': { ko: '끄기', en: 'Off' },
	'settings.remoteServer': { ko: '원격 서버', en: 'Remote server' },
	'settings.guildPath': { ko: '길드 경로', en: 'Guild path' },
	'settings.storage': { ko: '저장소', en: 'Storage' },
	'settings.devBrowser': { ko: '개발 (브라우저)', en: 'Dev (browser)' },
	'settings.unknown': { ko: '알 수 없음', en: 'Unknown' },
	'settings.displayHeading': { ko: '표시', en: 'Display' },
	'settings.uiScalePercentAria': { ko: 'UI 크기 (퍼센트)', en: 'UI scale (percent)' },
	'settings.resetTo100': { ko: '100% 로 초기화', en: 'Reset to 100%' },
	'settings.reset': { ko: '초기화', en: 'Reset' },
	'settings.contentWidthPxAria': { ko: '컨텐츠 폭 (픽셀)', en: 'Content width (pixels)' },
	'settings.resetToDefaultPx': { ko: '으로 초기화', en: ' as default' },
	'settings.themeDark': { ko: '다크', en: 'Dark' },
	'settings.themeLight': { ko: '라이트', en: 'Light' },
	'settings.themeSystem': { ko: '시스템', en: 'System' },
	'settings.customBasedOn': { ko: '커스텀 (', en: 'Custom (' },
	'settings.basedSuffix': { ko: ' 기반)', en: ' based)' },
	'settings.scaleHintTokens': {
		ko: 'CSS 토큰 기반 — 시스템 모드는 OS 설정 따라 자동 전환.',
		en: 'CSS-token based — system mode follows OS setting automatically.'
	},
	'settings.customTheme': { ko: '커스텀 테마', en: 'Custom theme' },
	'settings.newPreset': { ko: '+ 새 프리셋', en: '+ New preset' },
	'settings.exportJson': { ko: '내보내기 (JSON 복사)', en: 'Export (copy JSON)' },
	'settings.import': { ko: '가져오기', en: 'Import' },
	'settings.presetNamePlaceholder': { ko: '프리셋 이름', en: 'Preset name' },
	'settings.basedOnTheme': { ko: '기반 테마', en: 'Base theme' },
	'settings.darkBased': { ko: '다크 기반', en: 'Dark-based' },
	'settings.lightBased': { ko: '라이트 기반', en: 'Light-based' },
	'settings.createBtn': { ko: '생성', en: 'Create' },
	'settings.importJsonPlaceholder': {
		ko: '내보내기로 복사한 프리셋 JSON 을 붙여넣으세요',
		en: 'Paste the preset JSON copied from Export'
	},
	'settings.resetToDefaultToken': { ko: '기본값으로 되돌리기', en: 'Reset to default' },
	'settings.showAdvancedTokens': {
		ko: '고급 토큰 표시 (보드 강조색 등)',
		en: 'Show advanced tokens (board accent colors, etc.)'
	},
	'settings.tokenHint': {
		ko: '색을 바꾸면 즉시 적용되고 프리셋에 저장됩니다. 색점 왼쪽 표시는 기본값에서 변경된 토큰. 퀘스트 보드 색은 보드 재진입 시 반영됩니다.',
		en: 'Color changes apply immediately and are saved to the preset. The dot marks tokens changed from default. Quest Board colors apply on re-entering the board.'
	},
	'settings.presetHint': {
		ko: '프리셋을 만들거나 선택하면 토큰별 색 편집기가 나타납니다. 프리셋은 이 PC 에만 저장되며(개인 취향), 공유는 내보내기/가져오기(JSON)로.',
		en: 'Creating or selecting a preset shows the per-token color editor. Presets are stored on this PC only (personal preference); share via export/import (JSON).'
	},
	'settings.languageHint': {
		ko: '앱 전역에 적용됩니다. 표기가 어색한 화면을 발견하면 알려주세요.',
		en: 'Applies app-wide. Let us know if you find a screen where the wording looks off.'
	},
	'settings.deletePresetTitle': { ko: '프리셋 삭제', en: 'Delete preset' },
	'settings.deletePresetMsgSuffix': {
		ko: ' 프리셋을 삭제할까요? 되돌릴 수 없습니다.',
		en: '"? This cannot be undone.'
	},
	'settings.deletePresetMsgPrefix': { ko: "'", en: 'Delete preset "' },
	'settings.presetNameRequired': {
		ko: '프리셋 이름을 입력하세요.',
		en: 'Please enter a preset name.'
	},
	'settings.presetNameExists': {
		ko: '같은 이름의 프리셋이 이미 있습니다.',
		en: 'A preset with the same name already exists.'
	},

	// DEV-205 모듈5: Admin 페이지.
	'admin.title': { ko: '관리자 (Admin)', en: 'Admin' },
	'admin.noAuthWarn': {
		ko: '⚠ 인증 없음 — MVP 단계. 멀티유저로 확장 시 보호 필요.',
		en: '⚠ No authentication — MVP stage. Protection needed for multi-user use.'
	},
	'admin.backupsHeading': { ko: '백업 (Snapshots)', en: 'Backups (Snapshots)' },
	'admin.newBackup': { ko: '+ 새 백업', en: '+ New backup' },
	'admin.refresh': { ko: '새로고침', en: 'Refresh' },
	'admin.noBackups': {
		ko: '백업 없음. "+ 새 백업" 을 눌러 첫 백업을 생성하세요.',
		en: 'No backups. Press "+ New backup" to create the first one.'
	},
	'admin.colTime': { ko: '시간', en: 'Time' },
	'admin.colSize': { ko: '크기', en: 'Size' },
	'admin.restore': { ko: '복원', en: 'Restore' },
	'admin.deleteThisBackup': { ko: '이 백업 삭제', en: 'Delete this backup' },
	'admin.autoBackupHint': {
		ko: '자동 백업: 매 mutation 후 정책 검사 (ops 50 회 OR 24 시간 도달 시).',
		en: 'Auto-backup: policy check after every mutation (at 50 ops OR 24 hours).'
	},
	// BUG-188: 첨부는 백업 대상이 아니다 — 사용자가 백업만 믿고 있으면 안 되므로
	// 백업 화면에서 직접 밝힌다.
	'admin.backupScopeHint': {
		ko: '백업에 포함: 퀘스트·캠페인·규칙·도서관·이력 등 길드 문서. 포함되지 않음: 첨부파일(.guild/attachments) — 크기 제한이 없어 백업이 커지고 실패할 수 있어 제외합니다. 첨부는 git 또는 별도 보관을 권장합니다.',
		en: 'Included: guild documents — quests, campaigns, rules, library, history. Not included: attachments (.guild/attachments) — they have no size limit and would bloat or break backups. Keep them in git or back them up separately.'
	},
	'admin.driftHeading': { ko: 'Drift 검사', en: 'Drift check' },
	'admin.check': { ko: '검사', en: 'Check' },
	'admin.problemFilesPre': { ko: '⚠ 비정상 파일 ', en: '⚠ Problem files: ' },
	'admin.problemFilesPost': { ko: '개 — 캐시에서 제외됨', en: ' — excluded from cache' },
	'admin.driftEmpty': {
		ko: '파일 vs index.db 일치성 검사. 외부 편집 / git pull 후 활용.',
		en: 'Checks file vs index.db consistency. Use after external edits / git pull.'
	},
	'admin.driftOk': { ko: '✓ drift 없음', en: '✓ No drift' },
	'admin.missingInIndex': {
		ko: '파일은 있는데 index 에 없음 (',
		en: 'File exists but missing from index ('
	},
	'admin.staleInIndex': { ko: 'index 에 있는데 파일이 없음 (', en: 'In index but file missing (' },
	'admin.freshFiles': { ko: '파일이 index 보다 새것 (', en: 'File is newer than index (' },
	'admin.reindexHint': {
		ko: 'Reindex 버튼으로 캐시를 파일 기준으로 재구축할 수 있습니다.',
		en: 'Use the Reindex button to rebuild the cache from files.'
	},
	'admin.maintHeading': { ko: '정비 / 진단', en: 'Maintenance / Diagnostics' },
	'admin.vacuum': { ko: '정리 (VACUUM)', en: 'Clean (VACUUM)' },
	'admin.recentOps': { ko: '최근 작업 (저널)', en: 'Recent ops (journal)' },
	'admin.journalEmpty': {
		ko: '저널 비어 있음 (snapshot 직후 또는 mutation 없음)',
		en: 'Journal empty (right after a snapshot or no mutations)'
	},
	'admin.restoreTitle': { ko: '백업 복원', en: 'Restore backup' },
	'admin.deleteTitle': { ko: '백업 삭제', en: 'Delete backup' },
	'admin.latest': { ko: '최신', en: 'latest' },
	'admin.listLoadFailedPre': { ko: '목록 조회 실패: ', en: 'Failed to load list: ' },
	'admin.backupCreatedPre': { ko: '백업 생성: ', en: 'Backup created: ' },
	'admin.backupCreateFailedPre': { ko: '백업 생성 실패: ', en: 'Failed to create backup: ' },
	'admin.restoreDonePre': { ko: '복원 완료: ', en: 'Restore complete: ' },
	'admin.restoreDonePost': {
		ko: ' — 파일 복구 + 재색인 완료. 새로고침합니다.',
		en: ' — files restored + reindexed. Reloading.'
	},
	'admin.restoreFailedPre': { ko: '복원 실패: ', en: 'Restore failed: ' },
	'admin.backupDeletedPre': { ko: '백업 삭제: ', en: 'Backup deleted: ' },
	'admin.backupDeleteFailedPre': { ko: '백업 삭제 실패: ', en: 'Failed to delete backup: ' },
	'admin.driftOkMsg': {
		ko: 'drift 없음 — 파일과 캐시 일치',
		en: 'No drift — files and cache match'
	},
	'admin.driftFoundPost': {
		ko: ' 항목 drift 발견 — 아래 보고서 확인',
		en: ' item(s) of drift found — see report below'
	},
	'admin.driftCheckFailedPre': { ko: 'drift 검사 실패: ', en: 'Drift check failed: ' },
	'admin.reindexConfirm': {
		ko: '파일들로부터 index.db 를 재구축합니다. 계속할까요?',
		en: 'Rebuild index.db from files. Continue?'
	},
	'admin.reindexSkippedPre': { ko: 'reindex — 비정상 파일 ', en: 'reindex — skipped ' },
	'admin.reindexSkippedPost': {
		ko: '개 건너뜀. 새로고침 후 상세 표시.',
		en: ' problem file(s). Details after reload.'
	},
	'admin.reindexDone': {
		ko: 'reindex 완료 — 데이터 새로고침',
		en: 'Reindex complete — reloading data'
	},
	'admin.reindexFailedPre': { ko: 'reindex 실패: ', en: 'Reindex failed: ' },
	'admin.vacuumDonePre': { ko: 'VACUUM 완료 — ', en: 'VACUUM complete — ' },
	'admin.vacuumDoneMid': { ko: ' bytes 회수 (', en: ' bytes reclaimed (' },
	'admin.vacuumDonePost': { ko: '%)', en: '%)' },
	'admin.vacuumNoSpace': {
		ko: 'VACUUM 완료 — 회수 공간 없음 (이미 dense)',
		en: 'VACUUM complete — nothing to reclaim (already dense)'
	},
	'admin.vacuumFailedPre': { ko: 'VACUUM 실패: ', en: 'VACUUM failed: ' },
	'admin.journalEmptyMsg': {
		ko: 'journal.db 비어 있음 (snapshot 직후 또는 mutation 없음)',
		en: 'journal.db empty (right after a snapshot or no mutations)'
	},
	'admin.journalLoadFailedPre': { ko: 'journal 조회 실패: ', en: 'Failed to load journal: ' },
	'admin.problemFilesHint': {
		ko: '아래 파일은 파싱 실패 / 정의되지 않은 status 로 reindex·동기화에서 건너뛰어집니다. 파일을 고치거나 status 를 정의한 뒤 Reindex 하세요.',
		en: 'The files below are skipped during reindex/sync due to parse failure / undefined status. Fix the file or define the status, then Reindex.'
	},
	'admin.maintEmptyHint': {
		ko: 'VACUUM: index.db 의 dead row 공간 회수. 저널: journal.db 의 최근 변경 op (AOF).',
		en: 'VACUUM: reclaims dead-row space in index.db. Journal: recent change ops in journal.db (AOF).'
	},
	'admin.journalTotalPre': { ko: 'journal.db: ', en: 'journal.db: ' },
	'admin.journalTotalMid': { ko: ' op 중 최근 ', en: ' ops — recent ' },
	'admin.journalTotalPost': { ko: ' 개 (오래된 → 최신)', en: ' (oldest → newest)' },
	'admin.restoreConfirmPre': { ko: '정말 "', en: 'Restore backup "' },
	'admin.restoreConfirmMid': { ko: '" 백업으로 복원하시겠습니까?\n\n', en: '"?\n\n' },
	'admin.restoreConfirmPost': {
		ko: '현재 상태가 덮어써집니다 (직전 .pre-restore.db 로 자동 백업됨).',
		en: 'The current state will be overwritten (auto-backed up to .pre-restore.db first).'
	},
	'admin.deleteConfirmPre': { ko: '"', en: 'Delete backup "' },
	'admin.deleteConfirmPost': {
		ko: '" 백업을 삭제할까요?\n\n이 백업 파일이 영구 삭제됩니다 (되돌릴 수 없음).',
		en: '"?\n\nThis backup file will be permanently deleted (cannot be undone).'
	},

	// DEV-205 모듈5: Admin — 타입 관리.
	'adminTypes.newType': { ko: '+ 새 type', en: '+ New type' },
	'adminTypes.empty': { ko: 'type 없음.', en: 'No types.' },
	'adminTypes.colColor': { ko: '색', en: 'Color' },
	'adminTypes.colDesc': { ko: '설명', en: 'Description' },
	'adminTypes.colInUse': { ko: '사용 중', en: 'In use' },
	'adminTypes.prefixTitle': { ko: '대문자/숫자 1~6자', en: 'Uppercase/digits, 1–6 chars' },
	'adminTypes.descPlaceholder': { ko: '(없음)', en: '(none)' },
	'adminTypes.edit': { ko: '수정', en: 'Edit' },
	'adminTypes.renameCascadeHint': {
		ko: 'prefix 자체 rename 은 quest slug cascade 라 지원 안 함.',
		en: 'Renaming prefix itself requires a quest-slug cascade — not supported here.'
	},
	'adminTypes.newTypeTitle': { ko: '새 quest type', en: 'New quest type' },
	'adminTypes.prefixPlaceholder': {
		ko: 'DEV / BUG / REQ 같은 1~6자',
		en: '1–6 chars like DEV / BUG / REQ'
	},
	'adminTypes.descShortPlaceholder': { ko: '(선택) 짧은 설명', en: '(optional) short description' },
	'adminTypes.deleteTypeTitle': { ko: 'Type 삭제', en: 'Delete type' },
	'adminTypes.deleteMsg1': { ko: ' type 을 삭제할까요? ', en: ' type? ' },
	'adminTypes.deleteMsg2': { ko: '디스크의 ', en: 'The file ' },
	'adminTypes.deleteMsg3': { ko: ' 파일도 함께 제거됩니다.', en: ' on disk will also be removed.' },
	'adminTypes.renamePrefixTitle': {
		ko: 'Type prefix 변경 (cascade)',
		en: 'Change type prefix (cascade)'
	},
	'adminTypes.renameConfirmMid': { ko: ' 로 이름 변경.\n\n', en: '.\n\n' },
	'adminTypes.renameConfirmCount1': {
		ko: '이 type 의 모든 quest (',
		en: 'All quests of this type ('
	},
	'adminTypes.renameConfirmCount2': {
		ko: '개) 의 slug 가 cascade 됩니다 (파일명 / frontmatter / DB history).\n\n',
		en: ') will have their slug cascaded (filename / frontmatter / DB history).\n\n'
	},
	'adminTypes.renameConfirmMention1': { ko: "다른 quest 본문 안의 '", en: "Mentions of '" },
	'adminTypes.renameConfirmMention2': {
		ko: "-NNN' mention 은 자동 갱신되지 않습니다 — 직접 검색/수정 필요.\n\n계속할까요?",
		en: "-NNN' in other quest bodies are not updated automatically — search and edit manually.\n\nContinue?"
	},
	'adminTypes.prefixRequired': { ko: 'prefix 를 입력하세요.', en: 'Please enter a prefix.' },
	'adminTypes.updatedCascadePre': { ko: "'", en: "'" },
	'adminTypes.updatedCascadeMid': { ko: "' 갱신 완료 (cascade)", en: "' updated (cascade)" },
	'adminTypes.updatedSimplePost': { ko: "' 갱신됨", en: "' updated" },
	'adminTypes.listLoadFailedPre': { ko: 'type 목록 조회 실패: ', en: 'Failed to load type list: ' },
	'adminTypes.updateFailedPre': { ko: '갱신 실패: ', en: 'Update failed: ' },
	'adminTypes.addedPre': { ko: "'", en: "'" },
	'adminTypes.addedPost': { ko: "' 추가됨", en: "' added" },
	'adminTypes.addFailedPre': { ko: '추가 실패: ', en: 'Failed to add: ' },
	'adminTypes.deletedPre': { ko: "'", en: "'" },
	'adminTypes.deletedPost': { ko: "' 삭제됨", en: "' deleted" },
	'adminTypes.deleteFailedPre': { ko: '삭제 실패: ', en: 'Failed to delete: ' },
	'adminTypes.inUsePre': { ko: '사용 중 quest ', en: 'In use by ' },
	'adminTypes.inUsePost': { ko: '개 — 먼저 이동', en: ' quest(s) — move them first' },

	// DEV-205 모듈5: Admin — 상태 관리.
	'adminStatuses.newStatus': { ko: '+ 새 status', en: '+ New status' },
	'adminStatuses.empty': { ko: 'status 없음.', en: 'No statuses.' },
	'adminStatuses.doneColTitle': {
		ko: "캠페인 진행도 계산 시 '완료' 로 카운트되는 status",
		en: "Status counted as 'Done' for campaign progress"
	},
	'adminStatuses.doneCol': { ko: '완료', en: 'Done' },
	'adminStatuses.slugTitle': {
		ko: "소문자/숫자/'_' 만, 최대 32자",
		en: "Lowercase/digits/'_' only, max 32 chars"
	},
	'adminStatuses.nameEnTitle': {
		ko: "영문자로 시작 + 영문 / 숫자 / 공백 / '-' / '_' 만, 최대 32자",
		en: "Starts with a letter; letters/digits/space/'-'/'_' only, max 32 chars"
	},
	'adminStatuses.countsAsDoneTitle': {
		ko: "캠페인 진행도 계산 시 '완료' 로 카운트",
		en: "Counted as 'Done' for campaign progress"
	},
	'adminStatuses.countsAsDoneMark': {
		ko: '이 status 는 완료로 카운트',
		en: 'This status counts as done'
	},
	'adminStatuses.slugFrozenHint': {
		ko: 'slug 는 frozen — history / 파일 frontmatter 가 참조하므로 rename 안 됨.',
		en: 'slug is frozen — history / file frontmatter reference it, so renaming is not allowed.'
	},
	'adminStatuses.newStatusTitle': { ko: '새 quest status', en: 'New quest status' },
	'adminStatuses.nameEnPlaceholder': {
		ko: 'Blocked / In Review 등',
		en: 'e.g. Blocked / In Review'
	},
	'adminStatuses.nameKoPlaceholder': {
		ko: '(선택) 막힘 / 리뷰 중 등',
		en: '(optional) localized name'
	},
	'adminStatuses.newStatusNote': {
		ko: '새 status 는 목록 맨 뒤에 추가됩니다.',
		en: 'New statuses are added at the end of the list.'
	},
	'adminStatuses.deleteStatusTitle': { ko: 'Status 삭제', en: 'Delete status' },
	'adminStatuses.deleteMsg1': { ko: ' 을 삭제할까요?', en: '?' },
	'adminStatuses.deleteMsg2': { ko: '디스크의 ', en: 'Files under ' },
	'adminStatuses.deleteMsg3': {
		ko: ' 내 파일도 함께 제거됩니다.',
		en: ' on disk will also be removed.'
	},
	'adminStatuses.renameSlugTitle': {
		ko: 'Status slug 변경 (cascade)',
		en: 'Change status slug (cascade)'
	},
	'adminStatuses.renameSlugConfirm1': { ko: ' 로 slug 변경.\n\n', en: '.\n\n' },
	'adminStatuses.renameSlugConfirm2': {
		ko: '개 quest 의 frontmatter status + history 의 old/new value + statuses 파일명 모두 cascade.\n\n계속할까요?',
		en: " quests' frontmatter status + history old/new value + statuses filename all cascade.\n\nContinue?"
	},
	'adminStatuses.listLoadFailedPre': {
		ko: 'status 목록 조회 실패: ',
		en: 'Failed to load status list: '
	},
	'adminStatuses.updateFailedPre': { ko: '갱신 실패: ', en: 'Update failed: ' },
	'adminStatuses.nameEnRequired': { ko: 'name_en 은 필수.', en: 'name_en is required.' },
	'adminStatuses.addFailedPre': { ko: '추가 실패: ', en: 'Failed to add: ' },
	'adminStatuses.deleteFailedPre': { ko: '삭제 실패: ', en: 'Failed to delete: ' },

	// DEV-205 모듈5: Admin — 태그 정의 관리.
	'adminTags.heading': { ko: 'Tag 정의', en: 'Tag definitions' },
	'adminTags.newTagDef': { ko: '+ 새 tag 정의', en: '+ New tag definition' },
	'adminTags.pathHintPost': {
		ko: ' 의 색 / 설명 — quest·도서관·규칙이 공유하는 태그',
		en: ' color / description — tags shared by quests, library, and rules'
	},
	'adminTags.empty': { ko: '정의된 tag 없음.', en: 'No tag definitions.' },
	'adminTags.slugTitle': {
		ko: "소문자 / 숫자 / '_' 만, 최대 32자",
		en: "Lowercase/digits/'_' only, max 32 chars"
	},
	'adminTags.newTagDefTitle': { ko: '새 tag 정의', en: 'New tag definition' },
	'adminTags.slugPlaceholder': { ko: 'frontend / urgent 등', en: 'e.g. frontend / urgent' },
	'adminTags.descPurposePlaceholder': {
		ko: '(선택) 이 tag 의 용도',
		en: '(optional) purpose of this tag'
	},
	'adminTags.savedAsFilePre': { ko: '파일 ', en: 'Saved as file ' },
	'adminTags.savedAsFilePost': { ko: ' 로 저장됩니다.', en: '.' },
	'adminTags.deleteTagDefTitle': { ko: 'Tag 정의 삭제', en: 'Delete tag definition' },
	'adminTags.deleteMsg1': { ko: ' 정의를 삭제할까요?', en: ' definition?' },
	'adminTags.deleteMsg2': {
		ko: '기존 quest 의 tag 사용은 그대로 (fallback 색).',
		en: 'Existing quest tag usages are kept (fallback color).'
	},
	'adminTags.introTail': {
		ko: ' registry. 정의가 없는 tag 도 사용 가능(UI 기본 색으로 표시); 여기서는 색/설명만 미리 정의해둔다.',
		en: ' registry. Tags without a definition can still be used (shown with a default UI color); here you predefine color/description.'
	},
	'adminTags.listLoadFailedPre': {
		ko: 'tag 정의 조회 실패: ',
		en: 'Failed to load tag definitions: '
	},
	'adminTags.updatedPre': { ko: "'", en: "'" },
	'adminTags.updatedPost': { ko: "' 갱신됨", en: "' updated" },
	'adminTags.updateFailedPre': { ko: '갱신 실패: ', en: 'Update failed: ' },
	'adminTags.slugRequired': { ko: 'slug 는 필수.', en: 'slug is required.' },
	'adminTags.slugPattern': {
		ko: 'slug 는 소문자/숫자/_ 만 (최대 32자).',
		en: 'slug must be lowercase/digits/_ only (max 32 chars).'
	},
	'adminTags.addedPre': { ko: "'", en: "'" },
	'adminTags.addedPost': { ko: "' 추가됨", en: "' added" },
	'adminTags.addFailedPre': { ko: '추가 실패: ', en: 'Failed to add: ' },
	'adminTags.deletedDefPre': { ko: "'", en: "'" },
	'adminTags.deletedDefPost': {
		ko: "' 정의 삭제됨 (기존 사용처의 태그는 보존)",
		en: "' definition deleted (existing usages preserved)"
	},
	'adminTags.deleteFailedPre': { ko: '삭제 실패: ', en: 'Failed to delete: ' },

	// DEV-205 모듈5: Rules 페이지.
	'rules.tagSaveFailed': { ko: '태그 저장 실패', en: 'Failed to save tags' },
	'rules.slugRequired': { ko: 'slug 를 입력하세요.', en: 'Please enter a slug.' },
	'rules.createFailed': { ko: '생성 실패', en: 'Failed to create' },
	'rules.deleteFailed': { ko: '삭제 실패', en: 'Failed to delete' },
	'rules.renameFailed': { ko: '이름 변경 실패', en: 'Failed to rename' },
	'rules.listHeading': { ko: '규칙 목록', en: 'Rules' },
	'rules.newRule': { ko: '신규 규칙', en: 'New rule' },
	'rules.newBtn': { ko: '+ 신규', en: '+ New' },
	'rules.tagFilter': { ko: '태그 필터', en: 'Tag filter' },
	'rules.clearTagFilters': { ko: '태그 필터 모두 해제', en: 'Clear all tag filters' },
	'rules.emptyList': {
		ko: '규칙 없음. "+ 신규" 로 만들기.',
		en: 'No rules. Use "+ New" to create one.'
	},
	'rules.emptyFiltered': {
		ko: '태그 필터에 맞는 규칙 없음.',
		en: 'No rules match the tag filter.'
	},
	'rules.slugPlaceholder': { ko: 'slug (예: release-process)', en: 'slug (e.g. release-process)' },
	'rules.create': { ko: '생성', en: 'Create' },
	'rules.writeBtn': { ko: '+ 작성', en: '+ Write' },
	'rules.rename': { ko: '이름 변경', en: 'Rename' },
	'rules.newSlugPlaceholder': { ko: '새 slug', en: 'New slug' },
	'rules.bodyLabel': {
		ko: '본문 (Markdown) — 첨부는 드래그&드랍 / Ctrl+V',
		en: 'Body (Markdown) — attach via drag & drop / Ctrl+V'
	},
	'rules.attachUploadFailed': { ko: '첨부 업로드 실패', en: 'Attachment upload failed' },
	'rules.writeNow': { ko: '지금 작성', en: 'Write now' },
	'rules.deleteTitle': { ko: '규칙 삭제', en: 'Delete rule' },
	'rules.deleteMsgPre': { ko: "'", en: "Delete rule '" },
	'rules.deleteMsgPost': { ko: "' 규칙을 삭제할까요?", en: "'?" },
	'rules.discardEditTitle': { ko: '편집중 이동', en: 'Leave while editing' },
	'rules.discardEditMsg': {
		ko: '편집 중인 변경 사항이 있습니다. 버리고 이동할까요?',
		en: 'You have unsaved edits. Discard and leave?'
	},
	'rules.discardAndLeave': { ko: '버리고 이동', en: 'Discard and leave' },
	'rules.emptyCreateFirst': {
		ko: '"+ 신규" 로 첫 규칙을 만드세요.',
		en: 'Use "+ New" to create your first rule.'
	},
	// BUG-203: 규칙 페이지도 같은 이유(모바일은 목록이 위).
	'rules.emptySelect': { ko: '목록에서 규칙을 선택하세요.', en: 'Select a rule from the list.' },
	'rules.noBodyYet': { ko: '아직 작성된 본문이 없습니다.', en: 'No body written yet.' },
	'settings.jsonError': { ko: 'JSON 형식 오류', en: 'Invalid JSON format' },
	'settings.copiedJson': {
		ko: '프리셋 JSON 을 클립보드에 복사했습니다.',
		en: 'Preset JSON copied to clipboard.'
	},
	'settings.copyFailed': { ko: '클립보드 복사 실패', en: 'Failed to copy to clipboard' },
	'settings.importedPresetsPre': { ko: '프리셋 ', en: 'Imported ' },
	'settings.importedPresetsPost': { ko: '개를 가져왔습니다.', en: ' presets.' },
	'settings.deletePresetBtnSuffix': { ko: "' 삭제", en: '" — Delete' },
	'settings.deletePresetBtnPrefix': { ko: "'", en: 'Delete "' },
	'settings.tabHint': {
		ko: 'Tab 키를 눌렀을 때 탭 문자(\\t)를 넣을지, 공백을 넣을지. 퀘스트 / 캠페인 본문 편집기에 적용.',
		en: 'Whether pressing Tab inserts a tab character (\\t) or spaces. Applies to quest / campaign body editors.'
	},
	'settings.indentUnit': { ko: '칸', en: '' },
	'settings.indentHint': {
		ko: '공백 모드에서 Tab 한 번에 넣을 공백 개수 (탭 문자 모드에선 표시 폭). 2 / 4 중 선택.',
		en: 'Number of spaces inserted per Tab in space mode (display width in tab mode). Choose 2 or 4.'
	},
	'settings.uiScaleHintPre': {
		ko: '전체 UI 의 텍스트 / 여백이 비례 확대·축소됩니다 (',
		en: 'Text/spacing across the whole UI scales proportionally ('
	},
	'settings.uiScaleHintPost': { ko: '%~', en: '%–' },
	'settings.uiScaleHintTail': {
		ko: '%, 1% 단위). 슬라이더 / 숫자 입력 모두 즉시 적용.',
		en: '%, 1% steps). Both slider and number input apply immediately.'
	},
	'settings.contentWidthHintPre': {
		ko: '페이지의 좌우 안전 영역 — 와이드 모니터에서 더 넓게 사용. 범위 ',
		en: 'Safe left/right margin for pages — use wider on wide monitors. Range '
	},
	'settings.contentWidthHintMid': { ko: '~', en: '–' },
	'settings.contentWidthHintTail': { ko: 'px, 5px 단위.', en: 'px, 5px steps.' },
	// DEV-275: 슬라이더 최대값 = 폭 제한 해제(화면 전체).
	'settings.contentWidthFull': { ko: '전체', en: 'Full' },
	'settings.contentWidthFullHint': {
		ko: '맨 오른쪽은 "전체" — 창 폭을 그대로 사용합니다.',
		en: 'The far right is "Full" — uses the entire window width.'
	},
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
	'common.close': { ko: '닫기', en: 'Close' },
	'common.pickDate': { ko: '날짜 선택', en: 'Pick date' },
	'common.today': { ko: '오늘', en: 'Today' },
	'common.copy': { ko: '복사', en: 'Copy' },
	'common.change': { ko: '변경', en: 'Change' },
	'common.clearBtn': { ko: '× 해제', en: '× Clear' },
	'common.clearFilter': { ko: '필터 모두 해제', en: 'Clear all filters' },
	'common.statusChangeFailed': { ko: '상태 변경 실패', en: 'Failed to change status' },
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
	'welcome.pickFolder': { ko: '폴더에서 열기', en: 'Open from folder' },
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
	'welcome.remoteHint1': {
		ko: 'openguild-server 의 주소. 연결하면 아래 "최근 길드" 목록에 등록되어 다음부터는 클릭만으로 다시 열 수 있습니다. ',
		en: 'The openguild-server address. Once connected it is added to the "recent guilds" list below so you can reopen it with a single click. '
	},
	'welcome.remoteHintStrong': {
		ko: '인증이 없으니 신뢰된 네트워크에서만',
		en: 'No authentication — use only on trusted networks'
	},
	'welcome.remoteHint2': { ko: ' 사용하세요.', en: '.' },
	'welcome.uninitTitle': {
		ko: '이 위치를 길드로 초기화할까요?',
		en: 'Initialize this location as a guild?'
	},
	'welcome.uninitDesc1': {
		ko: '지정한 디렉토리에 openguild 마커 파일(',
		en: 'The selected directory has no openguild marker file ('
	},
	'welcome.uninitDesc2': {
		ko: ')이 없습니다. 초기화하면 마커 + ',
		en: '). Initializing creates the marker + '
	},
	'welcome.uninitDesc3': {
		ko: ' 데이터 폴더가 생성되어 바로 작업할 수 있습니다.',
		en: ' data folder so you can start working right away.'
	},
	'welcome.guildName': { ko: '길드 이름', en: 'Guild name' },
	'welcome.initializing': { ko: '초기화 중…', en: 'Initializing…' },
	'welcome.initAndOpen': { ko: '초기화하고 열기', en: 'Initialize and open' },
	'welcome.loading': { ko: '불러오는 중...', en: 'Loading...' },
	'welcome.browserInfo1': {
		ko: 'Recent guild 목록은 desktop 앱 (Tauri) 에서만 동작합니다.',
		en: 'The recent-guild list works only in the desktop app (Tauri).'
	},
	'welcome.browserInfo2': {
		ko: '브라우저 모드에선 현재 server 가 호스팅한 길드만 표시됩니다.',
		en: 'In browser mode, only the guild hosted by the current server is shown.'
	},
	'welcome.empty1': { ko: '아직 열어본 길드가 없습니다.', en: 'No guilds opened yet.' },
	'welcome.empty2': { ko: ' 으로 새 길드를 만들거나, ', en: ' to create a new guild, ' },
	'welcome.empty3': {
		ko: ' 로 기존 길드를 열거나, 위에서 원격 서버에 연결해 보세요.',
		en: ' to open an existing one, or connect to a remote server above.'
	},
	'welcome.pathMissing': {
		ko: '경로를 찾을 수 없습니다 — 이동 / 삭제됐을 수 있음',
		en: 'Path not found — may have been moved / deleted'
	},
	'welcome.openInWindow': {
		ko: '현재 창에서 이 길드를 엽니다',
		en: 'Opens this guild in the current window'
	},
	'welcome.pathNotFound': { ko: '⚠ 경로를 찾을 수 없습니다', en: '⚠ Path not found' },
	'welcome.guildOpening': { ko: '길드 여는 중…', en: 'Opening guild…' },
	'welcome.checkingConn': { ko: '연결 확인 중…', en: 'Checking connection…' },
	'welcome.connectRemote': {
		ko: '이 원격 서버에 연결합니다',
		en: 'Connects to this remote server'
	},
	'welcome.serverUnreachable': { ko: '서버에 연결할 수 없습니다', en: 'Cannot reach server' },
	'welcome.serverUnreachableWarn': {
		ko: '⚠ 서버에 연결할 수 없습니다',
		en: '⚠ Cannot reach server'
	},
	'welcome.removeFromList': { ko: '목록에서 제거', en: 'Remove from list' },
	'welcome.clearAll': { ko: '전체 비우기', en: 'Clear all' },
	'welcome.footerHint': {
		ko: '항목을 클릭하면 현재 창에서 그 길드를 엽니다.',
		en: 'Click an item to open that guild in the current window.'
	},
	'welcome.clearTitle': { ko: '최근 길드 목록 비우기', en: 'Clear recent guilds' },
	'welcome.clearMsg': {
		ko: '최근 길드 목록(로컬 + 원격)을 모두 비울까요? 되돌릴 수 없습니다.',
		en: 'Clear the entire recent-guild list (local + remote)? This cannot be undone.'
	},
	'welcome.clearConfirm': { ko: '비우기', en: 'Clear' },
	'welcome.removeTitle': { ko: '최근 길드에서 제거', en: 'Remove from recent guilds' },
	'welcome.removeSuffix': {
		ko: ' 을(를) 최근 목록에서 제거할까요?',
		en: ' — remove from the recent list?'
	},
	'welcome.removeNoteLocal': {
		ko: '디스크의 길드 파일은 그대로 두고, Recent 목록에서만 빠집니다.',
		en: 'The guild files on disk are kept; only the recent list entry is removed.'
	},
	'welcome.removeNoteRemote': {
		ko: '서버 연결 자체에는 영향 없고, Recent 목록에서만 빠집니다.',
		en: 'The server connection is unaffected; only the recent list entry is removed.'
	},
	'welcome.remove': { ko: '제거', en: 'Remove' },
	'welcome.incompatTitle': { ko: '호환되지 않는 길드', en: 'Incompatible guild' },
	'welcome.updateCheck': { ko: '업데이트 확인', en: 'Check for updates' },
	'welcome.tauriOnly': {
		ko: 'Tauri 데스크톱 앱에서만 동작합니다.',
		en: 'Works only in the Tauri desktop app.'
	},
	'welcome.enterGuildName': { ko: '길드 이름을 입력하세요.', en: 'Please enter a guild name.' },
	'welcome.pickDialogTitle': { ko: '길드 폴더 선택', en: 'Select guild folder' },
	'welcome.badResponse': {
		ko: '서버가 응답했지만 예상한 형식이 아닙니다.',
		en: 'The server responded but not in the expected format.'
	},
	'welcome.notValidDir': {
		ko: '선택된 경로가 유효한 디렉토리가 아닙니다',
		en: 'Selected path is not a valid directory'
	},
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
	// 목록 진행도 라벨 — 체크리스트/퀘스트를 따로 보여준다(admin 결정).
	'campaignList.progressChecklist': { ko: '체크', en: 'Check' },
	'campaignList.progressQuests': { ko: '퀘스트', en: 'Quests' },
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
	'conveyor.upcomingEmpty': {
		ko: '곧 시작 예정인 캠페인이 없습니다.',
		en: 'No upcoming campaigns.'
	},
	'conveyor.overdueCampaigns': { ko: '마감 지난 캠페인', en: 'Overdue campaigns' },
	'conveyor.upcomingCampaigns': { ko: '곧 시작 캠페인', en: 'Upcoming campaigns' },
	'conveyor.overdueQuests': { ko: '마감 지남 퀘스트', en: 'Overdue quests' },
	'conveyor.imminentQuests': { ko: '마감 임박 퀘스트', en: 'Due-soon quests' },

	// DEV-205 모듈2: 작업 기록 요약 카드(홈). worklog 상세는 모듈5/후속.
	'worklogCard.detailTitle': { ko: '작업 기록 상세', en: 'Worklog detail' },
	'worklogCard.title': { ko: '작업 기록', en: 'Worklog' },
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
	'attach.empty': {
		ko: "첨부 없음. '+ 첨부' 로 이미지·동영상·파일을 추가하세요.",
		en: "No attachments. Use '+ Attach' to add images, videos, or files."
	},
	'attach.openPreview': { ko: '열기 / 미리보기', en: 'Open / preview' },
	'attach.remove': { ko: '목록에서 제거', en: 'Remove from list' },
	'attach.removeAria': { ko: '제거', en: 'Remove' },
	'attach.download': { ko: '다운로드', en: 'Download' },
	'attach.saveDir': { ko: '첨부 저장 폴더', en: 'Attachment save folder' },
	'attach.openFailed': { ko: '열기 실패', en: 'Failed to open' },
	'attach.downloadFailed': { ko: '다운로드 실패', en: 'Download failed' },
	'attach.downloadAllFailed': { ko: '전체 다운로드 실패', en: 'Download all failed' },
	// BUG-168: 한도 초과 안내 — axum 원문(413) 노출 대신 크기를 밝힌다.
	'attach.tooLarge': { ko: '첨부 파일이 너무 큽니다', en: 'Attachment file is too large' },
	'attach.pickFile': { ko: '첨부할 파일 선택', en: 'Choose files to attach' },
	// DEV-298: 편집기 인라인 placeholder — 업로드가 끝나면 마크다운으로 치환된다.
	'attach.uploading': { ko: '업로드 중…', en: 'Uploading…' },
	// DEV-321: 브라우저 경로는 전송 전에 파일을 base64 로 바꾸는 구간이 있다 —
	// 그동안은 진행률을 알 수 없어 0% 로 멈춘 것처럼 보이므로 단계를 밝힌다.
	'attach.preparing': { ko: '준비 중…', en: 'Preparing…' },
	// DEV-322: 대기열에서 아직 시작 안 한 항목.
	'attach.queued': { ko: '대기', en: 'Queued' },
	// DEV-323: 업로드 취소.
	'attach.cancelAll': { ko: '취소', en: 'Cancel' },
	'attach.cancelled': { ko: '취소됨', en: 'Cancelled' },
	// DEV-338: 항목별 취소.
	'attach.cancelOne': { ko: '이 항목 취소', en: 'Cancel this item' },

	'history.title': { ko: '변경 이력', en: 'Change history' },
	'history.loading': { ko: '로드 중…', en: 'Loading…' },
	'history.empty': { ko: '변경 이력 없음.', en: 'No change history.' },
	'history.loadFailed': { ko: '이력 로드 실패', en: 'Failed to load history' },
	'history.none': { ko: '(없음)', en: '(none)' },

	'note.heading': { ko: '메모 (Memo)', en: 'Memo' },
	'note.emptyAction': { ko: '메모 작성', en: 'Write memo' },
	'note.emptyHint': {
		ko: '개인 메모. gitignored (팀 공유 X).',
		en: 'Private memo. Gitignored (not shared with the team).'
	},
	'note.help': {
		ko: '본인만 보는 비공개 메모 (`.guild/quests/{slug}.memo.md`, gitignored).',
		en: 'A private memo only you can see (`.guild/quests/{slug}.memo.md`, gitignored).'
	},
	'note.expand': { ko: '메모 펼치기', en: 'Expand memo' },
	'note.collapse': { ko: '메모 접기', en: 'Collapse memo' },
	'note.heightExpand': { ko: '메모를 전체 높이로 펼치기 (확장)', en: 'Expand memo to full height' },
	'note.heightFixed': { ko: '메모를 고정 높이 + 스크롤로 (고정)', en: 'Fixed height with scroll' },
	'note.expandBtn': { ko: '⤢ 확장', en: '⤢ Expand' },
	'note.fixBtn': { ko: '⊟ 고정', en: '⊟ Fixed' },
	'note.helpAttach': {
		ko: '(이미지·동영상은 드래그&드랍 또는 Ctrl+V 로 첨부)',
		en: '(attach images/videos via drag & drop or Ctrl+V)'
	},

	// DEV-205(모듈4): 댓글 섹션 (QuestCommentsSection) — 퀘스트·캠페인 공유.
	'comment.title': { ko: '댓글 (Comments)', en: 'Comments' },
	'comment.empty': { ko: '아직 댓글 없음.', en: 'No comments yet.' },
	'comment.authorOpt': { ko: '작성자 (옵션)', en: 'Author (optional)' },
	'comment.noName': { ko: '(이름 없음)', en: '(no name)' },
	'comment.anonymous': { ko: '(익명)', en: '(anonymous)' },
	'comment.discussion': { ko: '토론', en: 'Discussion' },
	'comment.bodyRequired': { ko: '본문을 입력하세요.', en: 'Please enter a body.' },
	'comment.bodyMarkdown': { ko: '본문 (markdown)', en: 'Body (markdown)' },
	'comment.writePlaceholder': {
		ko: '댓글 작성 (markdown 사용 가능)',
		en: 'Write a comment (markdown supported)'
	},
	'comment.toggleEditor': {
		ko: '입력 방식 전환 (일반 ↔ 마크다운 편집기)',
		en: 'Toggle input (plain ↔ markdown editor)'
	},
	'comment.addComment': { ko: '+ 댓글 추가', en: '+ Add comment' },
	'comment.adding': { ko: '추가…', en: 'Adding…' },
	'comment.addReply': { ko: '답글 추가', en: 'Add reply' },
	'comment.replyToSuffix': { ko: ' 에 답글…', en: ' — reply…' },
	'comment.writeReply': { ko: '답글 쓰기', en: 'Reply' },
	'comment.replies': { ko: '답글', en: 'Replies' },
	'comment.expandReplies': { ko: '답글 펼치기', en: 'Expand replies' },
	'comment.collapseReplies': { ko: '답글 접기', en: 'Collapse replies' },
	'comment.expandBody': { ko: '내용 펼치기', en: 'Expand' },
	'comment.collapseBody': { ko: '내용 접기', en: 'Collapse' },
	'comment.expandContent': { ko: '내용 펼치기', en: 'Expand content' },
	'comment.jumpToComment': { ko: '댓글로 이동', en: 'Jump to comment' },
	'comment.edited': { ko: '(편집됨)', en: '(edited)' },
	'comment.editedTitle': { ko: '편집됨 — ', en: 'Edited — ' },
	'comment.unknownTime': { ko: '(시각 미상)', en: '(unknown time)' },
	'comment.resolved': { ko: '✓ 해결됨', en: '✓ Resolved' },
	'comment.unresolved': { ko: '● 미해결 토론', en: '● Unresolved discussion' },
	'comment.resolvedTitle': {
		ko: '해결됨 — 클릭하면 다시 미해결로',
		en: 'Resolved — click to reopen'
	},
	'comment.unresolvedTitle': {
		ko: '미해결 토론 — 클릭하면 해결 처리 (완료 차단 해제)',
		en: 'Unresolved discussion — click to resolve (unblocks completion)'
	},
	'comment.markDiscussion': {
		ko: '토론으로 표시 — resolve 전까지 완료 차단',
		en: 'Mark as discussion — blocks completion until resolved'
	},
	'comment.unmarkDiscussion': { ko: '토론 표시 해제', en: 'Unmark discussion' },
	'comment.collapsedHasUnresolved': {
		ko: '접힌 답글에 미해결 토론 있음',
		en: 'Collapsed replies contain unresolved discussion'
	},
	'comment.collapsedAllResolved': {
		ko: '접힌 답글의 토론은 전부 해결됨',
		en: 'All discussions in collapsed replies are resolved'
	},
	'comment.addReaction': { ko: '반응 추가', en: 'Add reaction' },
	'comment.removeCustomReaction': {
		ko: '커스텀 반응 목록에서 삭제',
		en: 'Remove from custom reactions'
	},
	'comment.oneEmoji': { ko: '이모지 1개', en: 'One emoji' },
	'comment.emojiOne': { ko: '이모지 1개만 입력하세요.', en: 'Please enter exactly one emoji.' },
	'comment.clickToggle': { ko: '클릭하면 토글', en: 'Click to toggle' },
	'comment.pin': { ko: '상단 고정', en: 'Pin to top' },
	'comment.unpin': { ko: '고정 해제', en: 'Unpin' },
	'comment.expandComments': { ko: '댓글 펼치기', en: 'Expand comments' },
	'comment.collapseComments': { ko: '댓글 접기', en: 'Collapse comments' },
	'comment.unresolvedPre': { ko: '미해결 토론 ', en: 'Unresolved: ' },
	'comment.unresolvedPost': { ko: '개 — 완료 차단 중', en: ' — blocking completion' },
	'comment.discussionOnly': { ko: '토론만', en: 'Discussions' },
	'comment.unresolvedWord': { ko: '미해결', en: 'unresolved' },
	'comment.showAll': { ko: '전체 댓글 보기', en: 'Show all comments' },
	'comment.showDiscussionOnly': {
		ko: '토론 댓글이 있는 스레드만 보기 (비토론 댓글은 흐리게)',
		en: 'Show only threads with discussion (others dimmed)'
	},
	'comment.expandAll': { ko: '⊕ 전체 펼치기', en: '⊕ Expand all' },
	'comment.collapseAll': { ko: '⊖ 전체 접기', en: '⊖ Collapse all' },
	'comment.expandAllTitle': {
		ko: '모든 댓글의 답글·본문 펼치기',
		en: 'Expand all replies and bodies'
	},
	'comment.collapseAllTitle': {
		ko: '모든 댓글의 답글·본문 접기',
		en: 'Collapse all replies and bodies'
	},
	'comment.replyToDeleted': { ko: '↩ 삭제된 댓글에 대한 답글', en: '↩ Reply to a deleted comment' },
	'comment.deleteTitle': { ko: '댓글 삭제', en: 'Delete comment' },
	'comment.deleteMsg': {
		ko: '이 댓글을 삭제할까요? (답글이 있다면 그대로 남고 안내가 표시됩니다)',
		en: 'Delete this comment? (any replies are kept with a notice)'
	},
	'comment.ruleLinkPrefix': { ko: '규칙 · ', en: 'Rule · ' },
	'comment.bookLinkPrefix': { ko: '도서관 · ', en: 'Library · ' },
	'comment.newLink': { ko: '새 링크 (미존재)', en: 'New link (does not exist)' },

	// DEV-205 모듈3: Quest List 툴바.
	'questList.newQuest': { ko: '새 퀘스트', en: 'New quest' },
	'questList.viewMode': { ko: '뷰 모드', en: 'View mode' },
	'questList.treeTitle': {
		ko: '트리 — 부모 아래로 자식 들여쓰기',
		en: 'Tree — children indented under parents'
	},
	'questList.listTitle': { ko: '리스트 — 모든 퀘스트 평면', en: 'List — all quests flat' },
	'questList.sort': { ko: '정렬', en: 'Sort' },
	'questList.sortBy': { ko: '정렬 기준', en: 'Sort by' },
	'questList.sortDir': { ko: '정렬 방향', en: 'Sort direction' },
	'questList.sortDesc': {
		ko: '내림차순 — 클릭 시 오름차순',
		en: 'Descending — click for ascending'
	},
	'questList.sortAsc': {
		ko: '오름차순 — 클릭 시 내림차순',
		en: 'Ascending — click for descending'
	},
	'questList.tagFilter': { ko: '태그 필터', en: 'Tag filter' },
	'questList.filterRemoveSuffix': { ko: ' 필터 해제', en: ' — remove filter' },
	'questList.filterAddSuffix': { ko: ' 필터 추가', en: ' — add filter' },
	'questList.clearTagFilters': { ko: '태그 필터 모두 해제', en: 'Clear all tag filters' },
	'questList.clearAllBtn': { ko: '× 전체 해제', en: '× Clear all' },
	'questList.sortId': { ko: 'ID (생성 순)', en: 'ID (creation order)' },
	'questList.sortUrgency': { ko: '긴급도', en: 'Urgency' },
	'questList.sortStatus': { ko: '상태', en: 'Status' },
	'questList.sortUpdated': { ko: '갱신 시각', en: 'Updated' },
	'questList.sortCreated': { ko: '생성 시각', en: 'Created' },

	// DEV-205 모듈3: Quest List 필터.
	'filter.triAny': { ko: '전체', en: 'All' },
	'filter.triHas': { ko: '있음', en: 'Has' },
	'filter.triNone': { ko: '없음', en: 'None' },
	'filter.search': { ko: '검색', en: 'Search' },
	'filter.searchPlaceholder': { ko: '검색 (제목 / 본문)', en: 'Search (title / body)' },
	'filter.clearSearch': { ko: '검색어 지우기', en: 'Clear search' },
	'filter.titleOnly': { ko: '제목만', en: 'Title only' },
	'filter.advanced': { ko: '고급', en: 'Advanced' },
	// BUG-194: 좁은 화면에서 타입/상태 칩 줄을 접는 토글 라벨.
	'filter.chips': { ko: '필터', en: 'Filters' },
	'filter.urgency': { ko: '긴급도', en: 'Urgency' },
	'filter.prereqLabel': { ko: '선행', en: 'Prereq' },
	'filter.subLabel': { ko: '서브', en: 'Sub' },
	'filter.prereqTitle': {
		ko: '선행 quest 보유 여부 (전체 → 있음 → 없음)',
		en: 'Has prerequisites (all → has → none)'
	},
	'filter.subTitle': {
		ko: '서브 quest 보유 여부 (전체 → 있음 → 없음)',
		en: 'Has sub-quests (all → has → none)'
	},
	'filter.created': { ko: '생성', en: 'Created' },
	'filter.updated': { ko: '갱신', en: 'Updated' },
	'filter.clearAdvanced': { ko: '고급 필터 모두 해제', en: 'Clear all advanced filters' },
	'filter.clearBtn': { ko: '× 해제', en: '× Clear' },

	// DEV-205 모듈3: Quest Board.
	'board.dragToMove': { ko: '드래그하여 이동', en: 'Drag to move' },
	'board.closeEsc': { ko: '닫기 (Esc)', en: 'Close (Esc)' },
	'board.highlightRelated': { ko: '연관 퀘스트 하이라이트', en: 'Highlight related quests' },
	'board.multiSelect': { ko: '(다중 선택 가능)', en: '(multi-select)' },
	'board.gotoDetail': { ko: '퀘스트 상세 페이지로 이동 →', en: 'Go to quest detail page →' },
	'board.selectBtn': { ko: '선택', en: 'Select' },
	'board.arrangeBtn': { ko: '⊞ 정렬', en: '⊞ Arrange' },
	'board.cardNote': {
		ko: "하이라이트는 선택(파란색)과 별개 — '선택' 버튼을 누르면 드래그·상태변경 대상이 됨",
		en: "Highlight is separate from selection (blue) — press 'Select' to make them drag / status-change targets"
	},
	'board.allRelated': { ko: '연관 전체', en: 'All related' },
	'board.hlPre': { ko: '● 선행 퀘스트', en: '● Prerequisites' },
	'board.hlSub': { ko: '● 서브 퀘스트', en: '● Sub-quests' },
	'board.hlNext': { ko: '● 후속 퀘스트', en: '● Successors' },
	'board.hlParent': { ko: '● 부모 퀘스트', en: '● Parent' },
	'board.selectHighlighted': {
		ko: '하이라이트된 노드들을 모두 선택 (드래그·상태변경 대상으로)',
		en: 'Select all highlighted nodes (for drag / status change)'
	},
	'board.arrangeHighlighted': {
		ko: '하이라이트된 노드들을 그룹으로 정렬',
		en: 'Arrange highlighted nodes as a group'
	},
	'board.clearHighlightTitle': { ko: '하이라이트 해제', en: 'Clear highlight' },
	'board.filterActivePre': { ko: '필터 적용 중 — ', en: 'Filter active — ' },
	'board.filterActivePost': { ko: ' 매치', en: ' matched' },
	'board.newQuest': { ko: '새 퀘스트', en: 'New quest' },
	'board.columns': { ko: '세로 열', en: 'Columns' },
	'board.rows': { ko: '가로 행', en: 'Rows' },
	'board.orientationSwitchRows': {
		ko: '상태 레인을 가로 행으로 전환',
		en: 'Switch status lanes to horizontal rows'
	},
	'board.orientationSwitchColumns': {
		ko: '상태 레인을 세로 열로 전환',
		en: 'Switch status lanes to vertical columns'
	},
	'board.gridSnap': {
		ko: '그리드 스냅 — 드래그 종료 시 격자에 정렬 (G)',
		en: 'Grid snap — align to grid on drag end (G)'
	},
	'board.gridCols': {
		ko: '레인 그리드 열 수 (그리드만 갱신)',
		en: 'Lane grid columns (grid only)'
	},
	'board.gridRows': {
		ko: '레인 그리드 행 수 (그리드만 갱신)',
		en: 'Lane grid rows (grid only)'
	},
	'board.colSuffix': { ko: '열', en: ' col' },
	'board.rowSuffix': { ko: '행', en: ' row' },
	'board.settingsTitle': {
		ko: '레인 순서 / 숨김 / 그룹·단독 노드 가리기',
		en: 'Lane order / hide / hide group·solo nodes'
	},
	'board.settings': { ko: '보드 설정', en: 'Board settings' },
	'board.hideHelp': {
		ko: '레인 순서 변경 + 숨김 + 그룹·단독 노드 가리기. ◀ / ▶ 로 좌우 이동, 표시 해제 시 그 레인 전체 숨김.',
		en: 'Reorder lanes + hide + hide group/solo nodes. ◀ / ▶ to move, unchecking Show hides the whole lane.'
	},
	'board.hideHelpRows': {
		ko: '레인 순서 변경 + 숨김 + 그룹·단독 노드 가리기. ▲ / ▼ 로 위아래 이동, 표시 해제 시 그 레인 전체 숨김.',
		en: 'Reorder lanes + hide + hide group/solo nodes. ▲ / ▼ to move, unchecking Show hides the whole lane.'
	},
	'board.arrangeGroupTitle': {
		ko: '모든 노드 정렬 — 연관 그룹은 직사각형 영역으로 묶고, isolated 는 위쪽에 배치',
		en: 'Arrange all nodes — related groups boxed, isolated on top'
	},
	'board.arrangeFlatTitle': {
		ko: '모든 노드 정렬 — 슬러그 순으로 lane 안에서 왼쪽 위부터 채움',
		en: 'Arrange all nodes — fill each lane top-left by slug order'
	},
	'board.arrangeMode': { ko: '정렬 모드', en: 'Arrange mode' },
	// 사용자 지적: 툴바 버튼 라벨이 영어 하드코딩이라 한글 로케일에서도 영문.
	'board.snapBtn': { ko: '스냅', en: 'Snap' },
	'board.arrangeToolbarBtn': { ko: '정렬', en: 'Arrange' },
	'board.arrangeModeGroup': { ko: '그룹', en: 'Group' },
	'board.arrangeModeAll': { ko: '전체', en: 'All' },
	'board.fitView': { ko: '화면에 맞추기 (F)', en: 'Fit view (F)' },
	'board.undo': { ko: '실행 취소 (Ctrl+Z)', en: 'Undo (Ctrl+Z)' },
	'board.redo': { ko: '다시 실행 (Ctrl+Shift+Z)', en: 'Redo (Ctrl+Shift+Z)' },
	'board.toolbarExpand': { ko: '도구바 펼치기', en: 'Expand toolbar' },
	'board.toolbarCollapse': {
		ko: '도구바 접기 — 레인 라벨이 가려질 때',
		en: 'Collapse toolbar — when lane labels are hidden'
	},
	'board.toolbarCollapseShort': { ko: '도구바 접기', en: 'Collapse toolbar' },
	'board.colOrder': { ko: '순서', en: 'Order' },
	'board.colLane': { ko: '레인', en: 'Lane' },
	'board.colShow': { ko: '표시', en: 'Show' },
	'board.colHideGroup': { ko: '그룹 숨김', en: 'Hide group' },
	'board.colHideSolo': { ko: '단독 노드 숨김', en: 'Hide solo' },
	'board.moveLeft': { ko: '왼쪽으로', en: 'Move left' },
	'board.moveRight': { ko: '오른쪽으로', en: 'Move right' },
	'board.moveUp': { ko: '위로', en: 'Move up' },
	'board.moveDown': { ko: '아래로', en: 'Move down' },
	'board.laneShowTitle': {
		ko: '레인 표시 (체크 해제 시 레인 전체 숨김)',
		en: 'Show lane (unchecking hides the whole lane)'
	},
	'board.filter': { ko: '필터', en: 'Filter' },
	'board.filterHelp': {
		ko: '매치 안 되는 노드는 보드에서 흐리게 표시됩니다(숨기지 않음). 여기서 바꾼 필터는 리스트와 공유됩니다.',
		en: 'Non-matching nodes are dimmed (not hidden). Filters set here are shared with the list.'
	},
	'board.laneToggle': { ko: '레인 접기/펼치기', en: 'Collapse/expand lane' },
	'board.laneSortCols': { ko: '이 레인 정렬 열 수', en: 'Sort columns for this lane' },
	'board.laneSortRows': { ko: '이 레인 정렬 행 수', en: 'Sort rows for this lane' },
	'board.laneSortMode': { ko: '이 레인 정렬 모드', en: 'Sort mode for this lane' },
	'board.laneSettingsCollapse': { ko: '레인 설정 접기', en: 'Collapse lane settings' },
	'board.laneSettingsExpand': { ko: '레인 설정 펼치기', en: 'Expand lane settings' },
	'board.confirmChangeSuffix': { ko: ' 상태로 변경할까요?', en: ' — change status?' },
	'board.confirmChangeCountMid': { ko: '개 퀘스트를 ', en: ' quests → ' },
	'board.urgencyClampPre': { ko: 'urgency 원본값 ', en: 'urgency raw value ' },
	'board.urgencyClampPost': {
		ko: ' 가 범위(1-4) 밖 — clamp 표시 중',
		en: ' is out of range (1-4) — showing clamped'
	},

	// DEV-205 모듈4: 퀘스트 상세 고유 문자열.
	'qd.attachFailed': { ko: '첨부 실패', en: 'Attachment failed' },
	'qd.candidateFailed': { ko: '후보 조회 실패', en: 'Failed to load candidates' },
	'qd.addFailed': { ko: '추가 실패', en: 'Failed to add' },
	'qd.detachFailed': { ko: '분리 실패', en: 'Failed to detach' },
	'qd.deleteFailed': { ko: '삭제 실패', en: 'Failed to delete' },
	'qd.campaignLinkFailed': { ko: 'campaign 연결 실패', en: 'Failed to link campaign' },
	'qd.campaignUnlinkFailed': { ko: 'campaign 연결 해제 실패', en: 'Failed to unlink campaign' },
	'qd.urgencyClampPre': { ko: 'urgency 원본값 ', en: 'urgency raw value ' },
	'qd.urgencyClampPost': {
		ko: ' 가 유효 범위(1-4) 밖 — clamp 표시 중. .guild 파일의 urgency 를 1~4 로 정정하세요.',
		en: ' is out of the valid range (1-4) — showing clamped. Fix urgency to 1-4 in the .guild file.'
	},
	'qd.outOfRange': { ko: '⚠ 범위 밖', en: '⚠ Out of range' },
	'qd.requiredDue': { ko: '필수 기한', en: 'Required due' },
	'qd.desiredDue': { ko: '희망 기한', en: 'Desired due' },
	'qd.desiredDueHint': { ko: '(정보성)', en: '(informational)' },
	'qd.requiredDueHint': { ko: '(임박 / Overdue 기준)', en: '(basis for imminent / overdue)' },
	'qd.titleLabel': { ko: '제목', en: 'Title' },
	'qd.typeChange': { ko: '타입 변경', en: 'Change type' },
	'qd.typeChangeHint': {
		ko: '(slug 바뀜 — 즉시 적용)',
		en: '(slug changes — applies immediately)'
	},
	'qd.currentType': { ko: '현재 타입', en: 'Current type' },
	'qd.changeToSuffix': { ko: ' 로 변경 — slug 바뀜', en: ' — changes slug' },
	'qd.descLabel': {
		ko: '설명 (Markdown) — 첨부는 드래그&드랍 / Ctrl+V 또는 아래 첨부 섹션',
		en: 'Description (Markdown) — attach via drag & drop / Ctrl+V or the section below'
	},
	'qd.attachUploadFailed': { ko: '첨부 업로드 실패', en: 'Attachment upload failed' },
	'qd.statusChange': { ko: '상태 변경', en: 'Change status' },
	'qd.noDescription': { ko: '설명 없음.', en: 'No description.' },
	'qd.addDescription': { ko: '설명 추가하기', en: 'Add description' },
	'qd.newSub': { ko: '+ 신규', en: '+ New' },
	'qd.assignExisting': { ko: '+ 기존 지정', en: '+ Assign existing' },
	'qd.detachFromParent': { ko: '부모에서 분리', en: 'Detach from parent' },
	'qd.noSubQuests': { ko: '서브퀘스트 없음.', en: 'No sub-quests.' },
	'qd.addBtn': { ko: '+ 추가', en: '+ Add' },
	// DEV-279: 빈 섹션을 숨기는 대신 하나의 '연관 추가' 버튼에서 종류를 고른다.
	'qd.addRelation': { ko: '+ 연관 추가', en: '+ Add relation' },
	'qd.addRelationTitle': { ko: '추가할 연관 종류', en: 'Relation to add' },
	// DEV-278: 선행/후속도 신규 생성으로 추가할 수 있게 — 종류별 두 갈래.
	'qd.relLinkExisting': { ko: '기존 연결', en: 'Link existing' },
	'qd.relCreateNew': { ko: '새로 만들기', en: 'Create new' },
	'qd.removePrereq': { ko: '선행 퀘스트 제거', en: 'Remove prerequisite' },
	'qd.removeSuccessor': { ko: '후속 연결 해제', en: 'Unlink successor' },
	'qd.noPrereqs': { ko: '선행 퀘스트 없음.', en: 'No prerequisites.' },
	'qd.addSuccessor': { ko: '후속 퀘스트 추가', en: 'Add successor' },
	'qd.noSuccessors': { ko: '후속 퀘스트 없음.', en: 'No successors.' },
	'qd.noLinkableCampaigns': { ko: '연결 가능한 캠페인이 없습니다', en: 'No linkable campaigns' },
	'qd.selectCampaign': { ko: '캠페인 선택', en: 'Select campaign' },
	'qd.linkBtn': { ko: '+ 연결', en: '+ Link' },
	'qd.unlinkCampaign': { ko: '캠페인 연결 해제', en: 'Unlink campaign' },
	'qd.noLinkedCampaigns': { ko: '연결된 캠페인 없음.', en: 'No linked campaigns.' },
	'qd.loadingCandidates': { ko: '후보 조회 중…', en: 'Loading candidates…' },
	'qd.searchByIdTitle': { ko: 'ID 또는 제목으로 검색', en: 'Search by ID or title' },
	'qd.linkCampaignTitle': { ko: '캠페인 연결', en: 'Link campaign' },
	'qd.campaignSearchPlaceholder': { ko: 'C-NNN 또는 캠페인 제목', en: 'C-NNN or campaign title' },
	'qd.changeTypeTitle': { ko: '타입 변경', en: 'Change type' },
	'qd.changeTypeMsg1': { ko: ' 의 타입을 ', en: ' type will change to ' },
	'qd.changeTypeMsg2': { ko: ' 로 변경합니다.', en: '.' },
	'qd.changeTypeMsg3': { ko: ' 형태의 새 번호가 부여됩니다.', en: '-format number.' },
	'qd.changeTypeWarnPre': { ko: '⚠ 다른 퀘스트 본문 안에 ', en: '⚠ Mentions of ' },
	'qd.changeTypeWarnPost': {
		ko: ' 를 직접 언급(예 "참조") 한 부분은 자동으로',
		en: ' (e.g. "ref") in other quest bodies are not'
	},
	'qd.unsavedWarnStrong': {
		ko: '저장하지 않은 제목/설명 편집은 사라집니다.',
		en: 'Unsaved title/description edits will be lost.'
	},
	'qd.unsavedWarnRest': { ko: ' 먼저 저장 후 변경을 권장.', en: ' Save first before changing.' },
	'qd.changing': { ko: '변경 중…', en: 'Changing…' },
	'qd.comboSub': {
		ko: '기존 퀘스트를 서브퀘스트로 지정',
		en: 'Assign existing quest as sub-quest'
	},
	'qd.comboPrereq': { ko: '선행 퀘스트 추가', en: 'Add prerequisite' },
	// DEV-339: 상위(부모) 퀘스트 지정.
	'qd.comboParent': { ko: '상위 퀘스트 지정', en: 'Set parent quest' },
	'qd.comboSuccessor': { ko: '후속 퀘스트 추가', en: 'Add successor' },
	'qd.slugChangeSuffix': {
		ko: ' 슬러그(quest_id) 가 바뀌어',
		en: ' The slug (quest_id) will change and'
	},
	'qd.autoBlockNote': {
		ko: '갱신되지 않습니다. 필요하면 검색해서 직접 수정하세요. 부모/자식/선행 관계의 auto-block 메타는',
		en: 'are not updated automatically. Search and edit manually if needed. Parent/child/prerequisite auto-block metadata is updated automatically.'
	},
	'qd.autoUpdated': { ko: '자동 갱신됩니다.', en: '' },
	'qd.immediateNavWarn': {
		ko: '⚠ 변경 즉시 새 슬러그 페이지로 이동합니다 — ',
		en: '⚠ You will be navigated to the new slug page immediately — '
	},
	'questList.tree': { ko: '트리', en: 'Tree' },
	'questList.list': { ko: '리스트', en: 'List' },

	// DEV-205: 커스텀 타이틀바(DEV-253) 버튼 툴팁 + 메뉴.
	'titlebar.welcome': { ko: '시작 화면', en: 'Welcome' },
	'titlebar.back': { ko: '뒤로', en: 'Back' },
	'titlebar.forward': { ko: '앞으로', en: 'Forward' },
	'titlebar.menu': { ko: '메뉴', en: 'Menu' },
	'titlebar.pin': { ko: '항상 위에 고정', en: 'Pin (always on top)' },
	'titlebar.unpin': { ko: '고정 해제', en: 'Unpin' },
	'titlebar.minimize': { ko: '최소화', en: 'Minimize' },
	'titlebar.maximize': { ko: '최대화', en: 'Maximize' },
	'titlebar.restore': { ko: '이전 크기로 복원', en: 'Restore' },
	'titlebar.close': { ko: '닫기', en: 'Close' },
	'titlebar.search': { ko: '문서 검색', en: 'Search documents' },
	// DEV-276: 최근 본 문서 드롭다운.
	'titlebar.recent': { ko: '최근 본 문서', en: 'Recently viewed' },
	'titlebar.newQuest': { ko: '퀘스트 추가', en: 'New quest' },
	'titlebar.menuCampaigns': { ko: '캠페인 목록', en: 'Campaigns' },
	'titlebar.menuWorklog': { ko: '작업기록', en: 'Worklog' },
	'titlebar.menuTags': { ko: '태그 목록', en: 'Tags' },

	// DEV-205: 새 퀘스트 모달 (NewQuestModal).
	'nqm.newQuest': { ko: '새 퀘스트', en: 'New Quest' },
	'nqm.newSubQuest': { ko: '새 서브퀘스트', en: 'New Sub-Quest' },
	'nqm.noTypes': { ko: '퀘스트 타입이 없습니다', en: 'No quest types' },
	'nqm.noTypesMsg': {
		ko: 'Quest type (DEV / BUG / REQ 같은 prefix) 이 하나도 정의되어 있지 않아 새 퀘스트를 만들 수 없습니다. 먼저 Admin 페이지에서 type 을 추가하세요.',
		en: 'No quest type (prefix like DEV / BUG / REQ) is defined, so a quest cannot be created. Add a type in Admin first.'
	},
	'nqm.close': { ko: '닫기', en: 'Close' },
	'nqm.noStatuses': { ko: '퀘스트 상태가 없습니다', en: 'No quest statuses' },
	'nqm.goToAdmin': { ko: 'Admin 으로 가기', en: 'Go to Admin' },
	'nqm.noStatusMsg1': {
		ko: 'Quest status 가 하나도 정의되어 있지 않아 새 퀘스트를 만들 수 없습니다. 기본 ',
		en: 'No quest statuses are defined, so a quest cannot be created. Create the default '
	},
	'nqm.noStatusMsg2': {
		ko: ' (게시됨, 회색) 을 만들고 계속할까요? 필요하면 그 뒤 Admin 페이지에서 색 / 이름을 바꾸거나 다른 상태를 추가할 수 있습니다.',
		en: ' (Published, gray) and continue? You can change its color/name or add other statuses later in Admin.'
	},
	'nqm.addingStatus': { ko: '추가 중…', en: 'Adding…' },
	'nqm.addDefaultStatus': {
		ko: "기본 'Open' 추가하고 계속",
		en: "Add default 'Open' and continue"
	},
	'nqm.template': { ko: '템플릿', en: 'Template' },
	'nqm.noTemplate': { ko: '(템플릿 없이)', en: '(no template)' },
	'nqm.type': { ko: '타입', en: 'Type' },
	'nqm.status': { ko: '상태', en: 'Status' },
	'nqm.title': { ko: '제목 *', en: 'Title *' },
	'nqm.titlePlaceholder': { ko: '퀘스트 제목을 입력하세요', en: 'Enter a quest title' },
	'nqm.descOptional': { ko: '설명 (선택)', en: 'Description (optional)' },
	'nqm.descPlaceholder': {
		ko: 'Markdown 형식으로 작성할 수 있습니다',
		en: 'You can write in Markdown'
	},
	'nqm.relationsOptional': { ko: '연관관계 (선택)', en: 'Relations (optional)' },
	'nqm.relationsHint': {
		ko: '생성과 함께 상위·하위·선행·후속 퀘스트를 연결합니다.',
		en: 'Link parent, sub-quests, prerequisites, and successors on creation.'
	},
	'nqm.relationAdd': { ko: '+ 선택', en: '+ Select' },
	'nqm.relationRemove': { ko: '연관 제거', en: 'Remove relation' },
	'nqm.relationPartialFailPre': {
		ko: '퀘스트는 생성됐지만 연관관계 ',
		en: 'The quest was created, but '
	},
	'nqm.relationPartialFailPost': {
		ko: '개를 연결하지 못했습니다. 상세 화면에서 다시 추가해주세요.',
		en: ' relation(s) could not be linked. Add them again from the detail page.'
	},
	'nqm.creating': { ko: '생성 중…', en: 'Creating…' },
	'nqm.create': { ko: '퀘스트 생성', en: 'Create quest' },
	'nqm.saveAsTpl': { ko: '템플릿으로 저장', en: 'Save as template' },
	'nqm.closeSaveTpl': { ko: '템플릿 저장 닫기', en: 'Close template save' },
	'nqm.tplNamePlaceholder': {
		ko: '템플릿 이름 (예: bug-report)',
		en: 'Template name (e.g. bug-report)'
	},
	'nqm.tplNameRequired': { ko: '템플릿 이름을 입력하세요.', en: 'Please enter a template name.' },
	'nqm.tplExistsPre': { ko: "'", en: "'" },
	'nqm.tplExistsPost': {
		ko: "' 이미 있음 — 덮어쓰시겠습니까?",
		en: "' already exists — overwrite?"
	},
	'nqm.titleRequired': { ko: '제목을 입력해주세요.', en: 'Please enter a title.' },
	'nqm.typeRequired': { ko: '타입을 선택해주세요.', en: 'Please select a type.' },
	'nqm.noStatusError': {
		ko: '상태가 없습니다. 먼저 상태를 추가하세요.',
		en: 'No status available. Please add a status first.'
	},

	// DEV-205(2차): 업데이트 확인 알림(UpdateBanner) — 우하단 floating toast.
	'update.close': { ko: '닫기', en: 'Close' },
	'update.checking': { ko: '업데이트 확인 중…', en: 'Checking for updates…' },
	'update.uptodate': { ko: '최신 버전입니다.', en: 'You are up to date.' },
	'update.availablePre': { ko: '새 버전 ', en: 'New version ' },
	'update.availablePost': { ko: ' 사용 가능', en: ' available' },
	'update.releaseNotes': { ko: '릴리즈 노트', en: 'Release notes' },
	'update.installBtn': {
		ko: '지금 업데이트 (다운로드 + 재시작)',
		en: 'Update now (download + restart)'
	},
	'update.downloading': { ko: '다운로드 중…', en: 'Downloading…' },
	'update.ready': { ko: '설치 완료 — 재시작 중…', en: 'Install complete — restarting…' },
	'update.error': { ko: '확인 실패', en: 'Check failed' },

	// DEV-205(2차): 작업 기록(worklog) 페이지.
	'worklogPage.unit.day': { ko: '일', en: 'Day' },
	'worklogPage.unit.week': { ko: '주', en: 'Week' },
	'worklogPage.unit.month': { ko: '월', en: 'Month' },
	'worklogPage.unit.range': { ko: '구간', en: 'Range' },
	'worklogPage.badge.status': { ko: '상태', en: 'Status' },
	'worklogPage.badge.type': { ko: '타입', en: 'Type' },
	'worklogPage.badge.comment': { ko: '댓글', en: 'Comment' },
	'worklogPage.badge.created': { ko: '생성', en: 'Created' },
	'worklogPage.badge.discussion': { ko: '토론', en: 'Discussion' },
	// DEV-288: 규칙·도서관 변경 활동.
	'worklogPage.badge.rule': { ko: '규칙', en: 'Rule' },
	'worklogPage.badge.book': { ko: '문서', en: 'Doc' },
	'worklogPage.title': { ko: '작업 기록', en: 'Work Log' },
	'worklogPage.rangeStartAria': { ko: '구간 시작', en: 'Range start' },
	'worklogPage.rangeEndAria': { ko: '구간 끝', en: 'Range end' },
	'worklogPage.prevAria': { ko: '이전', en: 'Previous' },
	'worklogPage.monthSelectAria': { ko: '월 선택', en: 'Select month' },
	'worklogPage.dateSelectAria': { ko: '날짜 선택', en: 'Select date' },
	'worklogPage.nextAria': { ko: '다음', en: 'Next' },
	'worklogPage.today': { ko: '오늘', en: 'Today' },
	'worklogPage.notePre': { ko: '노트 — ', en: 'Note — ' },
	'worklogPage.noteEdit': { ko: '편집', en: 'Edit' },
	'worklogPage.noteWrite': { ko: '작성', en: 'Write' },
	'worklogPage.saving': { ko: '저장…', en: 'Saving…' },
	'worklogPage.save': { ko: '저장', en: 'Save' },
	'worklogPage.cancel': { ko: '취소', en: 'Cancel' },
	'worklogPage.noNotePre': { ko: '노트 없음 — "', en: 'No note — click "' },
	'worklogPage.noNotePost': { ko: '" 으로 남기기.', en: '" to write one.' },
	'worklogPage.notesInRangePre': { ko: '기간 내 노트 ', en: 'Notes in range: ' },
	'worklogPage.notesInRangePost': { ko: '건', en: '' },
	'worklogPage.activitiesPre': { ko: '활동 ', en: 'Activities: ' },
	'worklogPage.activitiesPost': { ko: '건', en: '' },
	'worklogPage.noActivity': { ko: '활동 없음.', en: 'No activity.' },
	'worklogPage.summary.statusChanges': { ko: '상태변경', en: 'status changes' },
	'worklogPage.summary.comments': { ko: '댓글', en: 'comments' },
	'worklogPage.summary.created': { ko: '생성', en: 'created' },
	'worklogPage.summary.discussion': { ko: '토론 전환', en: 'discussion toggles' },
	'worklogPage.summary.docChanges': { ko: '문서 변경', en: 'doc changes' },
	'worklogPage.summary.doneTransitions': { ko: 'done 전환', en: 'done transitions' },

	// DEV-205(2차): TagPills 공용 컴포넌트(규칙/도서관 태그 편집).
	'tagPills.add': { ko: '+ 추가', en: '+ Add' },
	'tagPills.cancel': { ko: '취소', en: 'Cancel' },
	'tagPills.removeTitle': { ko: '태그 제거', en: 'Remove tag' },
	'tagPills.none': { ko: '태그 없음.', en: 'No tags.' },
	'tagPills.newPlaceholder': {
		ko: '새 태그 (공백 구분으로 여러 개)',
		en: 'New tag(s), space-separated'
	},
	'tagPills.newAria': { ko: '새 태그', en: 'New tag' },

	// DEV-205(2차): 퀘스트/캠페인 콤보박스(검색 결과 없음).
	'combobox.questPlaceholder': {
		ko: '퀘스트 검색 (ID 또는 제목)',
		en: 'Search quests (ID or title)'
	},
	'combobox.campaignPlaceholder': {
		ko: '캠페인 검색 (C-NNN 또는 제목)',
		en: 'Search campaigns (C-NNN or title)'
	},
	'combobox.noResults': { ko: '결과 없음', en: 'No results' },

	// DEV-205(2차): 도서관 페이지.
	'library.title': { ko: '도서관', en: 'Library' },
	'library.tagSaveFail': { ko: '태그 저장 실패', en: 'Failed to save tags' },
	'library.titleRequired': { ko: '제목을 입력하세요.', en: 'Please enter a title.' },
	'library.createFail': { ko: '생성 실패', en: 'Creation failed' },
	'library.deleteFail': { ko: '삭제 실패', en: 'Deletion failed' },
	'library.retitleFail': { ko: '제목 변경 실패', en: 'Failed to rename' },
	'library.moveFail': { ko: '이동 실패', en: 'Failed to move' },
	'library.folderNameRequired': {
		ko: '폴더 이름을 입력하세요.',
		en: 'Please enter a folder name.'
	},
	'library.createFolderFail': { ko: '폴더 생성 실패', en: 'Failed to create folder' },
	'library.deleteFolderFail': {
		ko: '폴더 삭제 실패 — 비어 있는지 확인하세요',
		en: 'Failed to delete folder — check that it is empty'
	},
	'library.treeView': { ko: '트리 보기', en: 'Tree view' },
	'library.iconView': { ko: '아이콘 보기', en: 'Icon view' },
	'library.newFolder': { ko: '+ 폴더', en: '+ Folder' },
	'library.newDoc': { ko: '+ 신규', en: '+ New' },
	'library.searchPlaceholder': { ko: '제목/본문 검색', en: 'Search title/body' },
	'library.sortAria': { ko: '문서 정렬', en: 'Sort documents' },
	'library.sortTitle': { ko: '정렬 기준', en: 'Sort by' },
	'library.sortDesc': { ko: '내림차순 — 클릭 시 오름차순', en: 'Descending — click for ascending' },
	'library.sortAsc': { ko: '오름차순 — 클릭 시 내림차순', en: 'Ascending — click for descending' },
	'library.sortDirAria': { ko: '정렬 방향', en: 'Sort direction' },
	'library.tagFilterAria': { ko: '태그 필터', en: 'Tag filter' },
	'library.tagFilterOffPre': { ko: '', en: '' },
	'library.tagFilterOffPost': { ko: ' 필터 해제', en: ' filter off' },
	'library.tagFilterOnPost': { ko: ' 필터 추가', en: ' filter on' },
	'library.clearTagFiltersTitle': { ko: '태그 필터 모두 해제', en: 'Clear all tag filters' },
	'library.clearTagFilters': { ko: '× 전체 해제', en: '× Clear all' },
	'library.upToParent': { ko: '상위 폴더로', en: 'Up to parent folder' },
	'library.deleteCurrentFolder': { ko: '현재 폴더 삭제', en: 'Delete current folder' },
	'library.newFolderPlaceholder': { ko: '새 폴더 이름', en: 'New folder name' },
	'library.newFolderPathPlaceholder': {
		ko: '새 폴더 경로 (예: 아키텍처/서브)',
		en: 'New folder path (e.g. architecture/sub)'
	},
	'library.create': { ko: '생성', en: 'Create' },
	'library.cancel': { ko: '취소', en: 'Cancel' },
	'library.docTitlePlaceholder': { ko: '문서 제목', en: 'Document title' },
	'library.noSearchResults': { ko: '검색 결과 없음.', en: 'No search results.' },
	'library.emptyFolder': {
		ko: '비어 있음. "+ 신규" 또는 "+ 폴더" 로 만들기.',
		en: 'Empty. Create with "+ New" or "+ Folder".'
	},
	'library.emptyRoot': {
		ko: '문서 없음. "+ 신규" 로 만들기.',
		en: 'No documents. Create with "+ New".'
	},
	'library.topLevel': { ko: '(최상위)', en: '(top level)' },
	'library.pickFirstDoc': {
		ko: '"+ 신규" 로 첫 문서를 만드세요.',
		en: 'Create your first document with "+ New".'
	},
	// BUG-203: 모바일은 목록이 위에 오므로 '좌측에서' 가 맞지 않는다 — 방향을 뺀다.
	'library.pickDocFromList': {
		ko: '목록에서 문서를 선택하세요.',
		en: 'Select a document from the list.'
	},
	'library.backToList': { ko: '← 목록', en: '← List' },
	'library.editDoc': { ko: '✎ 편집', en: '✎ Edit' },
	'library.writeDoc': { ko: '+ 작성', en: '+ Write' },
	'library.retitle': { ko: '제목 변경', en: 'Rename' },
	'library.moveFolder': { ko: '폴더 이동', en: 'Move folder' },
	'library.delete': { ko: '삭제', en: 'Delete' },
	'library.created': { ko: '생성', en: 'Created' },
	'library.updated': { ko: '변경', en: 'Updated' },
	'library.newTitlePlaceholder': { ko: '새 제목', en: 'New title' },
	'library.change': { ko: '변경', en: 'Change' },
	'library.move': { ko: '이동', en: 'Move' },
	'library.bodyHint': {
		ko: '본문 (Markdown) — 첨부는 드래그&드랍 / Ctrl+V',
		en: 'Body (Markdown) — attach via drag & drop / Ctrl+V'
	},
	'library.attachUploadFail': { ko: '첨부 업로드 실패: ', en: 'Attachment upload failed: ' },
	'library.noBodyYet': { ko: '아직 작성된 본문이 없습니다.', en: 'No content written yet.' },
	'library.writeNow': { ko: '지금 작성', en: 'Write now' },
	'library.deleteDocTitle': { ko: '문서 삭제', en: 'Delete document' },
	'library.deleteDocMessage': {
		ko: '문서를 삭제할까요? (번호는 재사용되지 않습니다)',
		en: 'Delete this document? (the number is never reused)'
	},
	'library.deleteFolderTitle': { ko: '폴더 삭제', en: 'Delete folder' },
	'library.editingMoveTitle': { ko: '편집중 이동', en: 'Leave while editing' },
	'library.editingMoveMessage': {
		ko: '편집 중인 변경 사항이 있습니다. 버리고 이동할까요?',
		en: 'You have unsaved changes. Discard and leave?'
	},
	'library.discardAndMove': { ko: '버리고 이동', en: 'Discard and leave' },
	'library.folderExpand': { ko: '폴더 펼치기', en: 'Expand folder' },
	'library.folderCollapse': { ko: '폴더 접기', en: 'Collapse folder' },
	'library.folderDeleteTitle': {
		ko: '폴더 삭제 (비어 있을 때만)',
		en: 'Delete folder (only if empty)'
	},
	'library.sort.number': { ko: '번호', en: 'Number' },
	'library.sort.title': { ko: '이름', en: 'Title' },
	'library.sort.updated': { ko: '수정', en: 'Updated' },
	'library.deleteFolderMessagePre': { ko: "폴더 '", en: "Delete folder '" },
	'library.deleteFolderMessagePost': {
		ko: "' 를 삭제할까요? (비어 있어야 삭제 가능)",
		en: "'? (must be empty to delete)"
	},

	// DEV-205(3차): 검색 팔레트(SearchPalette) + 미리보기 팝업.
	'kind.quest': { ko: '퀘스트', en: 'Quest' },
	'kind.campaign': { ko: '캠페인', en: 'Campaign' },
	'kind.rule': { ko: '규칙', en: 'Rule' },
	'kind.book': { ko: '도서관', en: 'Library' },
	'palette.closeAria': { ko: '검색 닫기', en: 'Close search' },
	'palette.dialogAria': { ko: '문서 검색', en: 'Search documents' },
	'palette.scopeOnly': { ko: '만', en: ' only' },
	'palette.placeholder': {
		ko: '검색어 · 범위(rules: · quest: …) · #태그',
		en: 'Search · scope (rules: · quest: …) · #tag'
	},
	// DEV-294: recent 모드 — 같은 팔레트를 최근 본 문서 목록으로 재사용.
	'palette.placeholderRecent': {
		ko: '최근 본 문서 · 입력하면 전체 검색',
		en: 'Recently viewed · type to search everything'
	},
	'palette.noRecent': { ko: '최근 본 문서 없음', en: 'No recently viewed documents' },
	'palette.loading': { ko: '불러오는 중…', en: 'Loading…' },
	'palette.noResults': { ko: '검색 결과 없음', en: 'No results' },
	'palette.preview': { ko: '미리보기', en: 'Preview' },
	'palette.openWindow': { ko: '새 창으로 열기', en: 'Open in new window' },
	'palette.goPage': { ko: '페이지로 이동', en: 'Go to page' },
	'palette.backToListTitle': { ko: '목록으로 (Esc)', en: 'Back to list (Esc)' },
	'palette.backToList': { ko: '← 목록', en: '← List' },
	'palette.goPageArrow': { ko: '페이지로 이동 →', en: 'Go to page →' },
	'palette.emptyBody': { ko: '_(본문 없음)_', en: '_(no content)_' },
	'palette.previewLoadFail': {
		ko: '_미리보기를 불러오지 못했습니다._',
		en: '_Failed to load preview._'
	},
	'palette.resizeHandle': { ko: '아래로 드래그해 크기 조절', en: 'Drag down to resize' }
};

/** 현재 locale 기준 번역. 누락 키는 ko 원문 그대로(안전한 fallback). */
export function t(key: string, l: Locale): string {
	const entry = DICT[key];
	if (!entry) return key;
	return l === 'en' ? entry.en : entry.ko;
}
