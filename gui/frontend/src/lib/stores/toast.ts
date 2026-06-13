// 앱 공용 toast 알림 — alert() 대신 일관된 UI 로 메시지 표시.
//
// 사용: import { showToast } from '$lib/stores/toast'; showToast('실패', 'error');
// ToastHost (layout 에 1회 마운트) 가 구독해서 렌더 + 자동 소멸.

import { writable } from 'svelte/store';

export type ToastVariant = 'error' | 'success' | 'info';

export interface Toast {
	id: number;
	message: string;
	variant: ToastVariant;
}

export const toasts = writable<Toast[]>([]);
let nextId = 1;

/** toast 표시. durationMs<=0 이면 자동 소멸 안 함 (클릭으로만 닫힘). 반환: id. */
export function showToast(message: string, variant: ToastVariant = 'info', durationMs = 4000): number {
	const id = nextId++;
	toasts.update((list) => [...list, { id, message, variant }]);
	if (durationMs > 0) {
		setTimeout(() => dismissToast(id), durationMs);
	}
	return id;
}

export function dismissToast(id: number) {
	toasts.update((list) => list.filter((t) => t.id !== id));
}
