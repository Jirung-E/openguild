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

export function dismissNotif(id: string) {
	notifications.update((list) => list.filter((x) => x.id !== id));
}

let toastSeq = 1;

/** 텍스트 토스트 표시. durationMs<=0 이면 자동 소멸 안 함. 반환: notif id. */
export function showToast(
	message: string,
	variant: ToastVariant = 'info',
	durationMs = 4000
): string {
	const id = `toast-${toastSeq++}`;
	upsertNotif({ id, kind: 'toast', message, variant });
	if (durationMs > 0) {
		setTimeout(() => dismissNotif(id), durationMs);
	}
	return id;
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
