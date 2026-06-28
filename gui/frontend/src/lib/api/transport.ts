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
function routeToInvoke(req: ApiCall): { cmd: string; args: Record<string, unknown> } | null {
	const { method, path, body } = req;

	// query string 분리
	const qIdx = path.indexOf('?');
	const pathOnly = qIdx >= 0 ? path.slice(0, qIdx) : path;
	const query = qIdx >= 0 ? new URLSearchParams(path.slice(qIdx + 1)) : new URLSearchParams();
	const parts = pathOnly.replace(/^\/+/, '').split('/');
	// parts[0] === 'api'

	// ───── meta ─────
	// DEV-016 (multi-file): 다중 길드 규칙.
	if (pathOnly === '/api/rules') {
		if (method === 'GET') return { cmd: 'list_rules', args: {} };
		if (method === 'POST') {
			const b = (body as { slug?: string; content?: string } | undefined) ?? {};
			return {
				cmd: 'create_rule',
				args: { slug: b.slug ?? '', content: b.content ?? '' }
			};
		}
	}
	if (parts[0] === 'api' && parts[1] === 'rules' && parts[2]) {
		const slug = decodeURIComponent(parts[2]);
		if (method === 'GET') return { cmd: 'get_rule', args: { slug } };
		if (method === 'PUT') {
			const content = (body as { content?: string } | undefined)?.content ?? '';
			return { cmd: 'set_rule', args: { slug, content } };
		}
		if (method === 'PATCH') {
			const newSlug = (body as { new_slug?: string } | undefined)?.new_slug ?? '';
			return { cmd: 'rename_rule', args: { slug, newSlug } };
		}
		if (method === 'DELETE') {
			return { cmd: 'delete_rule', args: { slug } };
		}
	}
	// DEV-016 legacy 단일 — backward compat.
	if (pathOnly === '/api/rules-single') {
		if (method === 'GET') return { cmd: 'get_rules', args: {} };
		if (method === 'PUT') {
			const content = (body as { content?: string } | undefined)?.content ?? '';
			return { cmd: 'set_rules', args: { content } };
		}
	}

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
	if (parts[0] === 'api' && parts[1] === 'quests' && parts[2] === 'by' && parts[3]) {
		const slug = decodeURIComponent(parts[3]);
		// DEV-094: /api/quests/by/{slug}/comments — entry 단위 CRUD.
		if (parts[4] === 'comments') {
			// /comments (목록 / 추가)
			if (!parts[5]) {
				if (method === 'GET') return { cmd: 'list_comments', args: { slug } };
				if (method === 'POST') {
					const b =
						(body as
							| {
									author?: string;
									body?: string;
									parent_id?: number | null;
							  }
							| undefined) ?? {};
					return {
						cmd: 'add_comment',
						args: {
							slug,
							author: b.author ?? '',
							body: b.body ?? '',
							// Tauri 의 parent_id 인자 — Option<u64>. null / undefined 둘 다 None.
							parentId: b.parent_id ?? null
						}
					};
				}
			}
			// /comments/{id}/reactions (DEV-108: 이모지 토글)
			if (parts[5] && /^\d+$/.test(parts[5]) && parts[6] === 'reactions' && method === 'POST') {
				const id = Number(parts[5]);
				const rb = (body as { emoji?: string; author?: string } | undefined) ?? {};
				return {
					cmd: 'toggle_comment_reaction',
					args: { slug, id, emoji: rb.emoji ?? '', author: rb.author ?? '' }
				};
			}
			// /comments/{id}/discussion (DEV-142: 토론 플래그 토글)
			if (parts[5] && /^\d+$/.test(parts[5]) && parts[6] === 'discussion' && method === 'POST') {
				return { cmd: 'toggle_comment_discussion', args: { slug, id: Number(parts[5]) } };
			}
			// /comments/{id}/resolved (DEV-142: 토론 resolve 토글)
			if (parts[5] && /^\d+$/.test(parts[5]) && parts[6] === 'resolved' && method === 'POST') {
				return { cmd: 'toggle_comment_resolved', args: { slug, id: Number(parts[5]) } };
			}
			// /comments/{id} (수정 / 삭제)
			if (parts[5] && /^\d+$/.test(parts[5]) && !parts[6]) {
				const id = Number(parts[5]);
				if (method === 'PATCH') {
					const bodyText = (body as { body?: string } | undefined)?.body ?? '';
					return { cmd: 'update_comment', args: { slug, id, body: bodyText } };
				}
				if (method === 'DELETE') {
					return { cmd: 'delete_comment', args: { slug, id } };
				}
			}
		}
		if (parts[4] === 'memo') {
			if (method === 'GET') return { cmd: 'get_memo', args: { slug } };
			if (method === 'PUT') {
				const content = (body as { content?: string } | undefined)?.content ?? '';
				return { cmd: 'set_memo', args: { slug, content } };
			}
		}
		// DEV-152: .../attachments → add(sidecar 등록) / remove.
		if (parts[4] === 'attachments') {
			if (method === 'POST') {
				const b = (body as { path?: string; name?: string } | undefined) ?? {};
				return {
					cmd: 'add_quest_attachment',
					args: { slug, path: b.path ?? '', name: b.name ?? '' }
				};
			}
			if (method === 'DELETE') {
				return {
					cmd: 'remove_quest_attachment',
					args: { slug, path: query.get('path') ?? '' }
				};
			}
		}
		// 기본 — quest detail by slug.
		if (method === 'GET' && !parts[4]) {
			return { cmd: 'get_quest_by_slug', args: { slug } };
		}
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
					? cascadeStr
							.split(',')
							.map((s) => Number(s.trim()))
							.filter((n) => Number.isFinite(n))
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
		// DEV-076 / BUG-031: 희망 / 필수 기한 설정 / 해제.
		if (sub === 'due' && method === 'PATCH') {
			return { cmd: 'set_quest_due_dates', args: { id, body } };
		}
		// DEV-068: tag 전체 교체. body: { tags: string[] }.
		if (sub === 'tags' && method === 'PATCH') {
			const tags = (body as { tags?: string[] } | undefined)?.tags ?? [];
			return { cmd: 'set_quest_tags', args: { id, tags } };
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
		// DEV-011: quest 가 속한 캠페인 목록.
		if (sub === 'campaigns' && method === 'GET') {
			return { cmd: 'list_campaigns_for_quest', args: { questId: id } };
		}
	}

	// ───── admin ─────
	if (method === 'POST' && pathOnly === '/api/admin/snapshot') {
		return { cmd: 'admin_create_snapshot', args: {} };
	}
	if (method === 'GET' && pathOnly === '/api/admin/snapshots') {
		return { cmd: 'admin_list_snapshots', args: {} };
	}
	// DEV-175: DELETE /api/admin/snapshots/{ts}
	if (
		method === 'DELETE' &&
		parts[0] === 'api' &&
		parts[1] === 'admin' &&
		parts[2] === 'snapshots' &&
		parts[3]
	) {
		return { cmd: 'admin_delete_snapshot', args: { ts: decodeURIComponent(parts[3]) } };
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
	// DEV-162: 런타임 정비.
	if (method === 'POST' && pathOnly === '/api/admin/vacuum') {
		return { cmd: 'admin_vacuum', args: {} };
	}
	if (method === 'GET' && pathOnly === '/api/admin/journal') {
		const c = query.get('count');
		return { cmd: 'admin_journal_tail', args: c != null ? { count: Number(c) } : {} };
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
		// BUG-018: rename 은 update 안으로 통합 (body.new_prefix). 별도 sub-resource 제거.
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
		// BUG-018: rename 은 update 안으로 통합 (body.new_slug).
		if (method === 'PATCH') return { cmd: 'admin_update_status', args: { slug, body } };
		if (method === 'DELETE') return { cmd: 'admin_delete_status', args: { slug } };
	}
	// DEV-068: tag defs.
	if (method === 'GET' && pathOnly === '/api/tag-defs') {
		return { cmd: 'admin_list_tag_defs', args: {} };
	}
	if (method === 'POST' && pathOnly === '/api/tag-defs') {
		return { cmd: 'admin_upsert_tag_def', args: { body } };
	}
	if (parts[0] === 'api' && parts[1] === 'tag-defs' && parts[2]) {
		const slug = decodeURIComponent(parts[2]);
		if (method === 'DELETE') return { cmd: 'admin_delete_tag_def', args: { slug } };
	}

	// ───── DEV-152: 첨부 업로드(bytes 저장) — slug 와 무관, 단일 endpoint. ─────
	// HTTP body 는 snake_case(server Deserialize 와 1:1) — invoke args 는 Tauri
	// 컨벤션대로 camelCase 로 변환.
	if (method === 'POST' && pathOnly === '/api/attachments') {
		const b = (body as { data_base64?: string; ext?: string } | undefined) ?? {};
		return {
			cmd: 'save_attachment',
			args: { dataBase64: b.data_base64 ?? '', ext: b.ext ?? '' }
		};
	}

	// ───── campaigns (DEV-011) ─────
	// summaries — list 보다 먼저 매칭 (slug 자리에 'summaries' 가 옴).
	if (method === 'GET' && pathOnly === '/api/campaigns/summaries/active') {
		return { cmd: 'list_campaign_active_summaries', args: {} };
	}
	if (method === 'GET' && pathOnly === '/api/campaigns/summaries/upcoming') {
		const daysStr = query.get('days');
		const days = daysStr ? Number(daysStr) : undefined;
		return {
			cmd: 'list_campaign_upcoming_summaries',
			args: days !== undefined ? { days } : {}
		};
	}
	if (method === 'GET' && pathOnly === '/api/campaigns') {
		const status = query.get('status');
		return {
			cmd: 'list_campaigns',
			args: status ? { status } : {}
		};
	}
	if (method === 'POST' && pathOnly === '/api/campaigns') {
		return { cmd: 'create_campaign', args: { body } };
	}
	if (parts[0] === 'api' && parts[1] === 'campaigns' && parts[2] && parts[2] !== 'summaries') {
		const slug = decodeURIComponent(parts[2]);
		const sub = parts[3];
		if (!sub) {
			if (method === 'GET') return { cmd: 'get_campaign', args: { slug } };
			if (method === 'PATCH') return { cmd: 'update_campaign', args: { slug, body } };
			if (method === 'DELETE') return { cmd: 'delete_campaign', args: { slug } };
		}
		// .../quests  → link / unlink
		if (sub === 'quests') {
			if (method === 'POST') return { cmd: 'campaign_link_quest', args: { slug, body } };
			if (parts[4] && method === 'DELETE') {
				return {
					cmd: 'campaign_unlink_quest',
					args: { slug, questSlug: decodeURIComponent(parts[4]) }
				};
			}
		}
		// DEV-100: .../comments / .../memo — quest 댓글과 동일 형식.
		if (sub === 'comments') {
			if (!parts[4]) {
				if (method === 'GET') return { cmd: 'list_campaign_comments', args: { slug } };
				if (method === 'POST') {
					const b =
						(body as { author?: string; body?: string; parent_id?: number | null } | undefined) ??
						{};
					return {
						cmd: 'add_campaign_comment',
						args: {
							slug,
							author: b.author ?? '',
							body: b.body ?? '',
							parentId: b.parent_id ?? null
						}
					};
				}
			}
			if (parts[4] && /^\d+$/.test(parts[4]) && parts[5] === 'reactions' && method === 'POST') {
				const id = Number(parts[4]);
				const rb = (body as { emoji?: string; author?: string } | undefined) ?? {};
				return {
					cmd: 'toggle_campaign_comment_reaction',
					args: { slug, id, emoji: rb.emoji ?? '', author: rb.author ?? '' }
				};
			}
			if (parts[4] && /^\d+$/.test(parts[4]) && !parts[5]) {
				const id = Number(parts[4]);
				if (method === 'PATCH') {
					const bodyText = (body as { body?: string } | undefined)?.body ?? '';
					return { cmd: 'update_campaign_comment', args: { slug, id, body: bodyText } };
				}
				if (method === 'DELETE') return { cmd: 'delete_campaign_comment', args: { slug, id } };
			}
		}
		if (sub === 'memo') {
			if (method === 'GET') return { cmd: 'get_campaign_memo', args: { slug } };
			if (method === 'PUT') {
				const content = (body as { content?: string } | undefined)?.content ?? '';
				return { cmd: 'set_campaign_memo', args: { slug, content } };
			}
		}
		// DEV-152: .../attachments → add(sidecar 등록) / remove.
		if (sub === 'attachments') {
			if (method === 'POST') {
				const b = (body as { path?: string; name?: string } | undefined) ?? {};
				return {
					cmd: 'add_campaign_attachment',
					args: { slug, path: b.path ?? '', name: b.name ?? '' }
				};
			}
			if (method === 'DELETE') {
				return {
					cmd: 'remove_campaign_attachment',
					args: { slug, path: query.get('path') ?? '' }
				};
			}
		}
		// .../checklist  → add / set / rm
		if (sub === 'checklist') {
			if (!parts[4] && method === 'POST') {
				return { cmd: 'campaign_checklist_add', args: { slug, body } };
			}
			if (parts[4] && /^\d+$/.test(parts[4])) {
				const index = Number(parts[4]);
				if (method === 'PATCH') {
					// body: { checked: bool }
					const b = (body as { checked?: boolean } | undefined) ?? {};
					return {
						cmd: 'campaign_checklist_set',
						args: { slug, index, checked: b.checked ?? false }
					};
				}
				if (method === 'DELETE') {
					return { cmd: 'campaign_checklist_rm', args: { slug, index } };
				}
			}
		}
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
			const msg = typeof e === 'string' ? e : ((e as { message?: string }).message ?? String(e));
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
