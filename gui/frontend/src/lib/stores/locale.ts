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
	'common.cancel': { ko: '취소', en: 'Cancel' }
};

/** 현재 locale 기준 번역. 누락 키는 ko 원문 그대로(안전한 fallback). */
export function t(key: string, l: Locale): string {
	const entry = DICT[key];
	if (!entry) return key;
	return l === 'en' ? entry.en : entry.ko;
}
