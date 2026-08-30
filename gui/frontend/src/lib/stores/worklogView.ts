// BUG-259: 작업기록 페이지의 "지금 어떻게 보고 있었나" 상태.
//
// 펼쳐 놓은 문서 그룹과 기간 단위가 컴포넌트 지역 상태로만 있어서, 다른
// 라우트로 가면 컴포넌트가 파괴되고 돌아올 때 전부 초기화됐다(admin 보고).
//
// **왜 `sessionStorage` 인가**
//
// SvelteKit 의 `Snapshot` 은 히스토리 항목별이라 뒤로가기는 잡아도
// **네비게이션 링크로 다시 들어오는 경우**를 못 잡는다. 둘 다 살리려면
// 저장소가 필요하다.
//
// 그리고 `localStorage` 가 아니라 `sessionStorage` 다. 보기 모드(compact/full)
// 처럼 오래 가는 취향은 localStorage 지만, 어느 기간을 보고 있었고 어느 그룹을
// 펼쳐 놨나는 그보다 짧은 성격이다 — 세션 안에서는 유지되고 앱을 새로 켜면
// 기본값으로 돌아오는 것이 맞다(첫인상은 그대로 '오늘 / 전부 접힘').
//
// 값이 깨져 있어도 절대 던지지 않는다. 표시 상태라서, 못 읽으면 기본값으로
// 시작하면 그만이다. 여기서 던지면 페이지 전체가 안 뜬다.

/** 작업기록의 기간 단위. */
export type WorklogUnit = 'day' | 'week' | 'month' | 'range';

export const WORKLOG_UNITS: WorklogUnit[] = ['day', 'week', 'month', 'range'];
export const DEFAULT_UNIT: WorklogUnit = 'day';

const UNIT_KEY = 'openguild.worklogUnit';
const EXPANDED_KEY = 'openguild.worklogExpanded';

/**
 * 기억할 펼침 키의 상한.
 *
 * 키가 `날짜|slug` 라 날짜를 옮겨 다니면 계속 쌓이는데, 오래된 것은 다시 볼
 * 일이 없으면서 저장소만 먹는다. 넘치면 **오래된 쪽부터** 버린다.
 */
export const EXPANDED_MAX = 200;

function session(): Storage | null {
	try {
		// SSR/비활성 환경에서는 접근 자체가 던진다.
		return typeof sessionStorage === 'undefined' ? null : sessionStorage;
	} catch {
		return null;
	}
}

export function loadWorklogUnit(): WorklogUnit {
	const s = session();
	if (!s) return DEFAULT_UNIT;
	try {
		const raw = s.getItem(UNIT_KEY);
		return WORKLOG_UNITS.includes(raw as WorklogUnit) ? (raw as WorklogUnit) : DEFAULT_UNIT;
	} catch {
		return DEFAULT_UNIT;
	}
}

export function saveWorklogUnit(unit: WorklogUnit): void {
	const s = session();
	if (!s) return;
	try {
		s.setItem(UNIT_KEY, unit);
	} catch {
		/* quota / disabled — 표시 상태라 무시해도 된다. */
	}
}

/** 펼쳐 놓은 그룹 키(`날짜|slug`). 읽을 수 없으면 빈 Set. */
export function loadExpandedDocs(): Set<string> {
	const s = session();
	if (!s) return new Set();
	try {
		const raw = s.getItem(EXPANDED_KEY);
		if (!raw) return new Set();
		const parsed: unknown = JSON.parse(raw);
		if (!Array.isArray(parsed)) return new Set();
		return new Set(parsed.filter((v): v is string => typeof v === 'string'));
	} catch {
		return new Set();
	}
}

export function saveExpandedDocs(next: Set<string>): void {
	const s = session();
	if (!s) return;
	try {
		// Set 은 삽입 순서를 지키므로, 뒤에서 자르면 최근 것이 남는다.
		const arr = [...next];
		s.setItem(EXPANDED_KEY, JSON.stringify(arr.length > EXPANDED_MAX ? arr.slice(-EXPANDED_MAX) : arr));
	} catch {
		/* quota / disabled — 무시. */
	}
}

/** `날짜|slug`. 날짜가 다르면 같은 문서라도 다른 그룹이다. */
export function worklogDocKey(date: string, slug: string): string {
	return `${date}|${slug}`;
}
