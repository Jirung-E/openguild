/**
 * API base URL.
 *
 * 우선순위:
 *   1. `VITE_API_URL` env (build 시점). 설정 시 그 URL 사용.
 *   2. 미설정 시 빈 문자열 → 모든 fetch 가 상대경로 (`/api/...`) 로 발사 → 같은 origin 의 서버.
 *
 * 개발: `gui/frontend/.env.development` 에서 `VITE_API_URL=http://localhost:3000` 설정.
 * 프로덕션: 별도 정적 호스팅이면 build 시 env 지정 / 서버가 정적 자산까지 서빙하면 미설정으로 OK.
 */
const API_BASE = (import.meta.env.VITE_API_URL as string | undefined) ?? '';

async function request<T>(path: string, options?: RequestInit): Promise<T> {
	const res = await fetch(`${API_BASE}${path}`, {
		headers: {
			'Content-Type': 'application/json',
			...options?.headers
		},
		...options
	});

	if (!res.ok) {
		const err = await res.json().catch(() => ({ error: res.statusText }));
		throw new Error((err as { error?: string }).error ?? 'request failed');
	}

	// 204 No Content 또는 빈 body 응답 처리
	// (예: 201 Created with empty body)
	if (res.status === 204) return undefined as T;
	const contentLength = res.headers.get('content-length');
	if (contentLength === '0') return undefined as T;
	const text = await res.text();
	if (!text) return undefined as T;
	return JSON.parse(text) as T;
}

export const api = {
	get: <T>(path: string) => request<T>(path),
	post: <T>(path: string, body: unknown) =>
		request<T>(path, { method: 'POST', body: JSON.stringify(body) }),
	patch: <T>(path: string, body: unknown) =>
		request<T>(path, { method: 'PATCH', body: JSON.stringify(body) }),
	put: <T>(path: string, body: unknown) =>
		request<T>(path, { method: 'PUT', body: JSON.stringify(body) }),
	delete: (path: string) => request<void>(path, { method: 'DELETE' })
};
