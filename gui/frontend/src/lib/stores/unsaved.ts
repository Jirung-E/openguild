/**
 * DEV-153: 미저장 변경 통합 가드.
 *
 * 편집기(quest 본문/메타, campaign 본문, 댓글, 메모, 규칙 등)가 자신의 "편집 중
 * (= 저장 안 한 변경 가능성)" 상태를 고유 key 로 보고하면, +layout 의
 * beforeNavigate / beforeunload 가 이를 보고 이탈(라우트 이동·뒤로/앞으로·새로
 * 고침·길드 전환·창 닫기)을 막고 공용 확인 모달을 띄운다.
 *
 * dirty 판정은 보수적으로 "편집 모드 진입" 기준 — 실제 변경 비교 없이도 안전.
 * (편집을 열어두고 이탈하면 경고. 과경고가 누락보다 안전.) 저장/취소 시 각
 * 편집기가 false 로 보고해 해제한다.
 */
import { writable, derived, get } from 'svelte/store';

const dirty = writable<Set<string>>(new Set());

/** key 의 미저장 상태 보고. 컴포넌트 unmount / 저장 / 취소 시 false 로 정리할 것. */
export function setUnsaved(key: string, isDirty: boolean): void {
	dirty.update((s) => {
		const has = s.has(key);
		if (isDirty && !has) {
			s.add(key);
			return new Set(s);
		}
		if (!isDirty && has) {
			s.delete(key);
			return new Set(s);
		}
		return s;
	});
}

/** 하나라도 미저장이면 true (반응형). */
export const hasUnsaved = derived(dirty, (s) => s.size > 0);

/** 동기 조회 — beforeNavigate / beforeunload 핸들러용. */
export function anyUnsaved(): boolean {
	return get(dirty).size > 0;
}

/** 전체 해제 — 사용자가 '버리고 이동' 확정 시. */
export function clearUnsaved(): void {
	dirty.set(new Set());
}
