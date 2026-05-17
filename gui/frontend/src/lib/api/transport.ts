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
 * Tauri desktop 환경에서 사용. 현재는 stub — DEV-004 에서 invoke 연결.
 *
 * 매핑 규칙 (DEV-004 진입 시 확정):
 *   - `GET /api/quests` → `invoke('quests_list')`
 *   - `GET /api/quests/123` → `invoke('quests_get', { id: 123 })`
 *   - `POST /api/quests` → `invoke('quests_create', body)`
 *   - 등등. server 의 route → command 명 1:1.
 *
 * 본 stub 의 목적: Tauri 환경이 감지됐는데 invoke 미구현 시 명확한 에러로
 * 디버깅 가능하게.
 */
export class TauriTransport implements Transport {
	readonly kind = 'tauri' as const;

	async call<T>(_req: ApiCall): Promise<T> {
		throw new Error(
			'Tauri transport not yet implemented. DEV-003 (gui/ crate) + DEV-004 (invoke 핸들러) 완료 후 활성화.'
		);
	}
}

// ─────────────────────── default ───────────────────────

/** 모듈 로드 시점에 한 번 결정. SSR 도 안전. */
export const transport: Transport =
	detectEnvironment() === 'tauri' ? new TauriTransport() : new HttpTransport();
