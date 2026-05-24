/**
 * API 호출 transport 추상화.
 *
 * 두 환경 지원:
 * - **브라우저 / web GUI**: HTTP (fetch). 현재 동작.
 * - **Tauri desktop** (DEV-001 ~ DEV-006 진입 후): invoke. Rust 측 핸들러 직접 호출.
 *
 * 본 모듈은 환경 감지 + 분기만. 상위 api 모듈 (quests / meta / admin) 의 호출 사이트는
 * `transport.get/post/...` 만 알면 됨.
 *
 * Tauri 측 구현은 DEV-004 에서 실제 invoke 핸들러와 함께 채움. 현재는 명확한 stub.
 */

import type { HttpMethod } from './types';

/** transport 가 처리하는 한 호출의 명세. */
export interface ApiCall {
	method: HttpMethod;
	path: string;
	body?: unknown;
}

/** 백엔드 호출 추상 인터페이스. */
export interface Transport {
	readonly kind: 'http' | 'tauri';
	call<T>(req: ApiCall): Promise<T>;
}

/**
 * 환경 감지 — Tauri 글로벌이 있으면 Tauri, 그 외엔 HTTP.
 * Tauri 2.x 는 `__TAURI_INTERNALS__` 를 우선, 1.x 호환을 위해 `__TAURI__` 도 검사.
 */
export function detectEnvironment(): 'http' | 'tauri' {
	if (typeof window === 'undefined') {
		return 'http'; // SSR / Node 환경
	}
	const w = window as unknown as Record<string, unknown>;
	if ('__TAURI_INTERNALS__' in w || '__TAURI__' in w) {
		return 'tauri';
	}
	return 'http';
}

// ─────────────────────── HTTP transport ───────────────────────

/**
 * API base URL.
 *
 * 우선순위:
 *   1. `VITE_API_URL` env (build 시점). 설정 시 그 URL.
 *   2. 미설정 시 빈 문자열 → 상대경로 → 같은 origin 서버.
 *
 * 개발: `.env.development` 에서 `VITE_API_URL=http://localhost:3000`.
 * 프로덕션: 같은 도메인이면 미설정으로 OK. 별도 호스팅이면 build 시 지정.
 */
const HTTP_BASE = (import.meta.env.VITE_API_URL as string | undefined) ?? '';

export class HttpTransport implements Transport {
	readonly kind = 'http' as const;

	async call<T>(req: ApiCall): Promise<T> {
		const res = await fetch(`${HTTP_BASE}${req.path}`, {
			method: req.method,
			headers: { 'Content-Type': 'application/json' },
			body: req.body !== undefined ? JSON.stringify(req.body) : undefined
		});

		if (!res.ok) {
			const err = await res.json().catch(() => ({ error: res.statusText }));
			throw new Error((err as { error?: string }).error ?? 'request failed');
		}

		// 빈 본문 처리 — 204 또는 content-length 0.
		if (res.status === 204) return undefined as T;
		const contentLength = res.headers.get('content-length');
		if (contentLength === '0') return undefined as T;
		const text = await res.text();
		if (!text) return undefined as T;
		return JSON.parse(text) as T;
	}
}

// ─────────────────────── Tauri transport (stub) ───────────────────────

/**
 * Tauri desktop 환경에서 사용. `@tauri-apps/api` 의 `invoke` 로 Rust 측 핸들러 호출.
 *
 * 라우팅 (DEV-004 확정):
 * HTTP path / method → Tauri invoke 명 + arg 매핑 — `routeToInvoke` 참조.
 * server 의 axum route 정의와 1:1 (cmd 명만 다름). 새 route 추가 시 양쪽 동시 갱신.
 *
 * 에러 처리:
 * - Tauri invoke 가 throw 한 문자열을 `Error` 로 감싸 던짐.
 * - frontend 상위 (api 모듈) 는 환경 무지하게 try/catch.
 */
import { invoke } from '@tauri-apps/api/core';

/** path + method → (invoke 명, args). 매칭 실패 시 null. */
function routeToInvoke(
	req: ApiCall
): { cmd: string; args: Record<string, unknown> } | null {
	const { method, path, body } = req;

	// query string 분리
	const qIdx = path.indexOf('?');
	const pathOnly = qIdx >= 0 ? path.slice(0, qIdx) : path;
	const query = qIdx >= 0 ? new URLSearchParams(path.slice(qIdx + 1)) : new URLSearchParams();
	const parts = pathOnly.replace(/^\/+/, '').split('/');
	// parts[0] === 'api'

	// ───── meta ─────
	if (method === 'GET' && pathOnly === '/api/quest-types') {
		return { cmd: 'list_quest_types', args: {} };
	}
	if (method === 'GET' && pathOnly === '/api/quest-statuses') {
		return { cmd: 'list_quest_statuses', args: {} };
	}

	// ───── list level ─────
	if (method === 'GET' && pathOnly === '/api/quests') {
		return { cmd: 'list_quests', args: {} };
	}
	if (method === 'POST' && pathOnly === '/api/quests') {
		return { cmd: 'create_quest', args: { body } };
	}
	if (method === 'GET' && pathOnly === '/api/quest-positions') {
		return { cmd: 'list_quest_positions', args: {} };
	}
	if (method === 'GET' && pathOnly === '/api/quest-dependencies') {
		return { cmd: 'list_quest_dependencies', args: {} };
	}
	if (method === 'GET' && pathOnly === '/api/deleted-quests') {
		return { cmd: 'list_deleted_quests', args: {} };
	}

	// ───── /api/quests/by/{slug} ─────
	if (method === 'GET' && parts[0] === 'api' && parts[1] === 'quests' && parts[2] === 'by' && parts[3]) {
		return { cmd: 'get_quest_by_slug', args: { slug: decodeURIComponent(parts[3]) } };
	}

	// ───── /api/quests/{id}/... ─────
	if (parts[0] === 'api' && parts[1] === 'quests' && parts[2] && /^\d+$/.test(parts[2])) {
		const id = Number(parts[2]);
		const sub = parts[3];
		if (!sub) {
			if (method === 'GET') return { cmd: 'get_quest', args: { id } };
			if (method === 'PATCH') return { cmd: 'update_quest', args: { id, body } };
			if (method === 'DELETE') {
				const cascadeStr = query.get('cascade');
				const cascade = cascadeStr
					? cascadeStr.split(',').map((s) => Number(s.trim())).filter((n) => Number.isFinite(n))
					: undefined;
				return { cmd: 'delete_quest', args: { id, cascade } };
			}
		}
		if (sub === 'status' && method === 'PATCH') {
			return { cmd: 'change_quest_status', args: { id, body } };
		}
		if (sub === 'parent' && method === 'PATCH') {
			return { cmd: 'change_quest_parent', args: { id, body } };
		}
		// DEV-055: quest type 변경 (slug 가 바뀜, 다른 quest 파일들도 cascade).
		if (sub === 'type' && method === 'PATCH') {
			return { cmd: 'change_quest_type', args: { id, body } };
		}
		if (sub === 'restore' && method === 'PATCH') {
			return { cmd: 'restore_quest', args: { id } };
		}
		if (sub === 'candidates' && method === 'GET') {
			return { cmd: 'list_quest_candidates', args: { id, relation: query.get('relation') ?? '' } };
		}
		if (sub === 'prerequisites' && method === 'POST') {
			return { cmd: 'add_prerequisite', args: { id, body } };
		}
		if (sub === 'prerequisites' && parts[4] && /^\d+$/.test(parts[4]) && method === 'DELETE') {
			return { cmd: 'remove_prerequisite', args: { id, prereqId: Number(parts[4]) } };
		}
		if (sub === 'position' && method === 'PUT') {
			return { cmd: 'update_quest_position', args: { id, body } };
		}
		if (sub === 'history' && method === 'GET') {
			return { cmd: 'list_quest_history', args: { id } };
		}
	}

	// ───── admin ─────
	if (method === 'POST' && pathOnly === '/api/admin/snapshot') {
		return { cmd: 'admin_create_snapshot', args: {} };
	}
	if (method === 'GET' && pathOnly === '/api/admin/snapshots') {
		return { cmd: 'admin_list_snapshots', args: {} };
	}
	if (method === 'POST' && pathOnly === '/api/admin/restore') {
		return { cmd: 'admin_restore', args: { args: body ?? {} } };
	}
	if (method === 'GET' && pathOnly === '/api/admin/drift') {
		return { cmd: 'admin_check_drift', args: {} };
	}
	if (method === 'POST' && pathOnly === '/api/admin/reindex') {
		return { cmd: 'admin_reindex', args: {} };
	}

	// ───── admin meta (DEV-014) — types ─────
	if (method === 'GET' && pathOnly === '/api/admin/types') {
		return { cmd: 'admin_list_types', args: {} };
	}
	if (method === 'POST' && pathOnly === '/api/admin/types') {
		return { cmd: 'admin_create_type', args: { body } };
	}
	if (parts[0] === 'api' && parts[1] === 'admin' && parts[2] === 'types' && parts[3]) {
		const prefix = decodeURIComponent(parts[3]);
		if (method === 'PATCH') return { cmd: 'admin_update_type', args: { prefix, body } };
		if (method === 'DELETE') return { cmd: 'admin_delete_type', args: { prefix } };
	}
	// ───── admin meta (DEV-014) — statuses ─────
	if (method === 'GET' && pathOnly === '/api/admin/statuses') {
		return { cmd: 'admin_list_statuses', args: {} };
	}
	if (method === 'POST' && pathOnly === '/api/admin/statuses') {
		return { cmd: 'admin_create_status', args: { body } };
	}
	if (parts[0] === 'api' && parts[1] === 'admin' && parts[2] === 'statuses' && parts[3]) {
		const slug = decodeURIComponent(parts[3]);
		if (method === 'PATCH') return { cmd: 'admin_update_status', args: { slug, body } };
		if (method === 'DELETE') return { cmd: 'admin_delete_status', args: { slug } };
	}

	return null;
}

export class TauriTransport implements Transport {
	readonly kind = 'tauri' as const;

	async call<T>(req: ApiCall): Promise<T> {
		const mapped = routeToInvoke(req);
		if (!mapped) {
			throw new Error(
				`TauriTransport: ${req.method} ${req.path} 에 매핑된 invoke 핸들러 없음. ` +
					`transport.ts 의 routeToInvoke 와 gui/src/commands.rs 둘 다 갱신 필요.`
			);
		}
		try {
			// invoke 가 unit (`()`) 반환 시 null. T 로 그대로 캐스팅.
			const result = await invoke<T>(mapped.cmd, mapped.args);
			return result as T;
		} catch (e) {
			// Tauri 가 throw 한 메시지는 보통 string. Error 로 감싸기.
			const msg = typeof e === 'string' ? e : (e as { message?: string }).message ?? String(e);
			throw new Error(msg);
		}
	}
}

/** 테스트용 export — routeToInvoke 매핑이 server route 와 일치하는지 검증. */
export const __test_only = { routeToInvoke };

// ─────────────────────── default ───────────────────────

/** 모듈 로드 시점에 한 번 결정. SSR 도 안전. */
export const transport: Transport =
	detectEnvironment() === 'tauri' ? new TauriTransport() : new HttpTransport();
