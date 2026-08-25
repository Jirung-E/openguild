/**
 * REQ-009 강화 검색을 UI 에서 쓸 때 반복되는 두 가지 — **디바운스**와
 * **stale-async 가드** — 를 한 곳에 모은다.
 *
 * 강화 검색은 기본 검색과 달리 키 입력마다 서버를 거친다. 그래서 매번 두
 * 함정이 따라온다:
 *
 * 1. 글자마다 요청을 던지면 안 된다 (디바운스).
 * 2. **늦게 온 응답이 최신 결과를 덮어쓰면 안 된다.** '수달' 을 치다가 '수' 에서
 *    나간 요청이 '수달' 응답보다 늦게 도착하면 화면이 '수' 의 결과로 되돌아간다.
 *    REQ-004 가 지적한 결함과 같은 종류다.
 *
 * 퀘스트 목록(REQ-010)·도서관(REQ-011)이 각자 구현해 두 벌이 됐고, 검색
 * 팔레트(REQ-012)·규칙(REQ-013)까지 네 벌이 될 참이라 여기로 뺐다. 룬을 쓰지
 * 않는 순수 TS 라 단위 테스트가 붙는다 — 컴포넌트는 자기 `$effect` 에서
 * 이걸 호출한다.
 */
import type { SearchHit } from '$lib/api/search';

/** 검색 결과에서 한 종류만 추려 id 집합으로. */
export function matchIdsOf(hits: SearchHit[], kind: SearchHit['kind']): Set<string> {
	return new Set(hits.filter((h) => h.kind === kind).map((h) => h.id));
}

export interface LatestQueryOptions {
	/** 디바운스 지연(ms). 기본 250 — 도서관/퀘스트 목록이 쓰던 값. */
	debounceMs?: number;
}

/**
 * "마지막 질의만 반영" 실행기.
 *
 * `run()` 을 여러 번 불러도 마지막 것만 실제로 나가고, 그보다 앞서 나간 요청의
 * 응답은 도착해도 **버려진다**. `cancel()` 은 대기 중인 요청을 취소하고 이후
 * 도착하는 응답도 무시하게 만든다(토글을 끄거나 검색어를 지웠을 때).
 */
export class LatestQuery<Arg, Res> {
	#fetcher: (arg: Arg) => Promise<Res>;
	#debounceMs: number;
	#timer: ReturnType<typeof setTimeout> | null = null;
	#seq = 0;

	constructor(fetcher: (arg: Arg) => Promise<Res>, opts: LatestQueryOptions = {}) {
		this.#fetcher = fetcher;
		this.#debounceMs = opts.debounceMs ?? 250;
	}

	/**
	 * 디바운스 후 조회. 이 호출이 아직 최신일 때만 `onResult` 가 불린다.
	 * `onError` 는 실패 시 — 호출측은 보통 "기존(로컬) 동작으로 되돌림" 을 한다.
	 */
	run(arg: Arg, onResult: (res: Res) => void, onError?: (e: unknown) => void): void {
		if (this.#timer) clearTimeout(this.#timer);
		const seq = ++this.#seq;
		this.#timer = setTimeout(() => {
			this.#fetcher(arg)
				.then((res) => {
					if (seq !== this.#seq) return; // 늦게 온 응답 — 버린다.
					onResult(res);
				})
				.catch((e) => {
					if (seq !== this.#seq) return;
					onError?.(e);
				});
		}, this.#debounceMs);
	}

	/** 대기 중인 요청 취소 + 이미 나간 요청의 응답도 무시. */
	cancel(): void {
		if (this.#timer) clearTimeout(this.#timer);
		this.#timer = null;
		this.#seq++; // 진행 중인 요청을 stale 로 만든다.
	}
}
