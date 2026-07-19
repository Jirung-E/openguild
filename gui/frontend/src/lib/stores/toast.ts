// DEV-259: 앱의 모든 알림을 우하단 단일 스택으로 통합.
//
// 예전엔 toast(showToast) / UpdateBanner / SchemaAheadBanner 가 각자
// position:fixed 로 같은 우하단에 따로 떠서 겹쳤다. 이제 하나의
// `notifications` 배열(도착순)로 관리하고, NotificationHost(ToastHost)가 한
// 컬럼으로 렌더한다 — 새 알림은 배열 끝에 append → 컬럼 맨 아래(코너)에 뜨고
// 기존 알림은 위로 밀린다. persistent 알림(update/schema)은 안정적 id 로
// upsert 해 상태가 바뀌어도 새로 쌓지 않고 제자리에서 갱신.
//
// 사용(하위호환 유지): import { showToast } from '$lib/stores/toast';

import { writable } from 'svelte/store';

export type ToastVariant = 'error' | 'success' | 'info';
export type NotifKind = 'toast' | 'update' | 'schema';

interface NotifBase {
	id: string;
	kind: NotifKind;
}
export interface ToastNotif extends NotifBase {
	kind: 'toast';
	message: string;
	variant: ToastVariant;
	/** DEV-266: 중복 억제 — 같은 message+variant 재발생 시 새 카드 대신 이
	 *  값이 올라간다(카드에 ×N 뱃지). */
	count: number;
}
export interface UpdateNotif extends NotifBase {
	kind: 'update';
}
export interface SchemaNotif extends NotifBase {
	kind: 'schema';
	binaryVersion: string;
	aheadVersions: number[];
	latestKnown: number | null;
}
export type Notif = ToastNotif | UpdateNotif | SchemaNotif;

export const notifications = writable<Notif[]>([]);

/** 같은 id 면 제자리에서 교체(도착 순서 유지), 없으면 끝에 추가(=우하단 코너). */
export function upsertNotif(n: Notif) {
	notifications.update((list) => {
		const i = list.findIndex((x) => x.id === n.id);
		if (i >= 0) {
			const copy = [...list];
			copy[i] = n;
			return copy;
		}
		return [...list, n];
	});
}

// DEV-266: 토스트별 자동 소멸 타이머 — 중복 억제 시 수명 리셋에 필요.
const toastTimers = new Map<string, ReturnType<typeof setTimeout>>();

export function dismissNotif(id: string) {
	const timer = toastTimers.get(id);
	if (timer) {
		clearTimeout(timer);
		toastTimers.delete(id);
	}
	notifications.update((list) => list.filter((x) => x.id !== id));
}

let toastSeq = 1;

/**
 * 텍스트 토스트 표시. durationMs<=0 이면 자동 소멸 안 함. 반환: notif id.
 *
 * DEV-266 중복 억제: 같은 message+variant 의 토스트가 이미 떠 있으면 새
 * 카드를 쌓는 대신 그 카드의 count 를 올리고(×N 뱃지) 수명을 리셋한다 —
 * 루프에서 같은 에러가 연발할 때 스택이 같은 문구로 도배되는 것 방지.
 */
export function showToast(
	message: string,
	variant: ToastVariant = 'info',
	durationMs = 4000
): string {
	let id: string | null = null;
	notifications.update((list) => {
		const i = list.findIndex(
			(x) => x.kind === 'toast' && x.message === message && x.variant === variant
		);
		if (i < 0) return list;
		id = list[i].id;
		const copy = [...list];
		const prev = copy[i] as ToastNotif;
		copy[i] = { ...prev, count: prev.count + 1 };
		return copy;
	});
	if (id === null) {
		id = `toast-${toastSeq++}`;
		upsertNotif({ id, kind: 'toast', message, variant, count: 1 });
	}
	const old = toastTimers.get(id);
	if (old) clearTimeout(old);
	if (durationMs > 0) {
		const fixedId = id;
		toastTimers.set(
			fixedId,
			setTimeout(() => dismissNotif(fixedId), durationMs)
		);
	}
	return id;
}

// ── DEV-266: 표시 상한 + 우선순위 ────────────────────────────────────
// 동시 다발 알림이 스택을 무한정 키우지 않게 최대 표시 개수를 두고,
// 넘치면 "+N개 더" 축약 칩으로 접는다.
export const MAX_VISIBLE_NOTIFS = 5;

/**
 * 표시 목록 계산 (호스트/테스트 공용 순수 함수). 반환 배열 = 위→아래(코너).
 *
 * 정렬: persistent(update/schema)는 항상 코너쪽(맨 아래) — 오래 떠 있는
 * 카드가 토스트 출입에 따라 위치가 튀지 않게 고정. 토스트는 도착순(새것이
 * persistent 바로 위).
 *
 * 상한: expanded 가 아니면 MAX_VISIBLE_NOTIFS 개까지만. 초과분은 우선순위
 * 낮은 것부터 숨김 — 오래된 info/success 토스트 순. error 토스트와
 * persistent 는 숨기지 않는다(상한을 넘더라도 유지 — error > warning/
 * persistent > info 정책).
 */
export function computeVisible(
	list: Notif[],
	expanded: boolean
): { visible: Notif[]; hidden: number } {
	const toasts = list.filter((n) => n.kind === 'toast') as ToastNotif[];
	const persistent = list.filter((n) => n.kind !== 'toast');
	const ordered: Notif[] = [...toasts, ...persistent];
	if (expanded || ordered.length <= MAX_VISIBLE_NOTIFS) {
		return { visible: ordered, hidden: 0 };
	}
	const overBy = ordered.length - MAX_VISIBLE_NOTIFS;
	const hideIds = new Set<string>();
	for (const tn of toasts) {
		if (hideIds.size >= overBy) break;
		if (tn.variant !== 'error') hideIds.add(tn.id);
	}
	return { visible: ordered.filter((n) => !hideIds.has(n.id)), hidden: hideIds.size };
}

// ── SchemaAheadBanner(BUG-041/BUG-139) 닫힘 영속화 ──────────────────
// 같은 ahead 셋 + 같은 binary 에 대해 한 번 닫으면 다시 안 뜨게 signature 저장.
const SCHEMA_DISMISS_KEY = 'openguild.schemaAheadBannerDismissed';

export function schemaSig(binaryVersion: string, ahead: number[]): string {
	return `${binaryVersion}|${ahead.join(',')}`;
}
export function isSchemaDismissed(sig: string): boolean {
	try {
		return localStorage.getItem(SCHEMA_DISMISS_KEY) === sig;
	} catch {
		return false;
	}
}
/** 스키마 알림 닫기 — signature 저장 + 스택에서 제거. */
export function dismissSchema(sig: string) {
	try {
		localStorage.setItem(SCHEMA_DISMISS_KEY, sig);
	} catch {
		/* ignore */
	}
	dismissNotif('schema');
}
