/**
 * 상위 API 모듈 (quests / meta / admin) 이 사용하는 verb-별 헬퍼.
 *
 * 내부에선 `transport` (HTTP / Tauri 자동 분기) 호출.
 * 호출 사이트는 환경 무지 — 같은 코드가 web 과 desktop 양쪽에서 동작.
 *
 * 환경 감지 / Tauri 변환 로직은 `./transport.ts` 참조 (DEV-002).
 */

import { transport } from './transport';

export const api = {
	get: <T>(path: string) => transport.call<T>({ method: 'GET', path }),
	post: <T>(path: string, body: unknown) => transport.call<T>({ method: 'POST', path, body }),
	patch: <T>(path: string, body: unknown) => transport.call<T>({ method: 'PATCH', path, body }),
	put: <T>(path: string, body: unknown) => transport.call<T>({ method: 'PUT', path, body }),
	delete: (path: string) => transport.call<void>({ method: 'DELETE', path })
};
