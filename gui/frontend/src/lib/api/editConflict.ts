/**
 * DEV-435: 같은 본문 동시 편집 — 저장이 그사이 남이 한 변경과 부딪혔을 때 서버(409) · 앱(꼬리표 문자열)이 함께
 * 돌려주는 것. 화면이 "무엇이 부딪혔는지" 를 보고 고르게 한다.
 */

export type ConflictSegment =
	| { kind: 'same'; text: string }
	| { kind: 'conflict'; base: string; current: string; mine: string };

export interface EditConflict {
	/** text: 같은 줄을 다르게 고침 · stale: 시작한 뒤 바뀜(번호로만 앎) · renamed: 번호가 바뀜 · deleted: 지워짐 */
	reason: 'text' | 'stale' | 'renamed' | 'deleted';
	message: string;
	/** 지금 저장된 본문 — 다시 시작할 출발점. */
	current?: string | null;
	segments: ConflictSegment[];
	/** renamed 일 때 새 번호. */
	new_id?: string | null;
}

export class EditConflictError extends Error {
	readonly conflict: EditConflict;
	constructor(conflict: EditConflict) {
		super(conflict.message);
		this.name = 'EditConflictError';
		this.conflict = conflict;
	}
}

/** 앱(Tauri) 쪽 오류 문자열의 꼬리표 — gui/src/commands.rs 의 `EDIT_CONFLICT_TAG` 와 같다. */
export const EDIT_CONFLICT_TAG = 'EDIT_CONFLICT::';

/** 앱이 던진 문자열 → 편집 충돌이면 그 오류, 아니면 null. */
export function editConflictFromInvoke(msg: string): EditConflictError | null {
	if (!msg.startsWith(EDIT_CONFLICT_TAG)) return null;
	try {
		return new EditConflictError(JSON.parse(msg.slice(EDIT_CONFLICT_TAG.length)) as EditConflict);
	} catch {
		return null;
	}
}

/** 서버의 409 본문 → 편집 충돌이면 그 오류, 아니면 null. */
export function editConflictFromHttp(status: number, payload: unknown): EditConflictError | null {
	if (status !== 409) return null;
	const c = (payload as { conflict?: EditConflict } | null)?.conflict;
	return c && typeof c.reason === 'string' ? new EditConflictError(c) : null;
}

/** 충돌 구간마다 고른 것으로 글을 다시 잇는다. `choice[i]` 는 i 번째 충돌 구간(충돌만 셈)의 선택. */
export type ConflictChoice = 'current' | 'mine' | 'both';

export function assemble(segments: ConflictSegment[], choice: ConflictChoice[]): string {
	let i = 0;
	return segments
		.map((s) => {
			if (s.kind === 'same') return s.text;
			const c = choice[i++];
			if (c === 'current') return s.current;
			if (c === 'mine') return s.mine;
			const cur = s.current.endsWith('\n') || s.current === '' ? s.current : s.current + '\n';
			return cur + s.mine;
		})
		.join('');
}
