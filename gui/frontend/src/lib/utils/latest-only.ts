/**
 * REQ-004: 비동기 결과를 커밋하기 전에 **"내가 아직 현재 대상인가"** 를
 * 확인하는 가드.
 *
 * 저장소에 이미 올바른 사례가 있다 — `routes/quests/[id]/+page.svelte` 의
 * slug effect 는 결과 적용 전에 `slug === currentSlug` 를 재확인한다. 그런데
 * 이력 패널 3종·미리보기 팝업은 그 확인 없이 `.then(list => entries = list)`
 * 로 **무조건** 대입한다.
 *
 * 그러면 A 문서를 열고 응답이 오기 전에 B 로 이동했을 때, 늦게 도착한 A 응답이
 * 화면을 덮어 **나머지는 B 인데 이력만 A** 인 상태가 된다.
 *
 * `LatestQuery`(enhanced-search.ts)와 목적이 겹치지만 그쪽은 디바운스가 함께
 * 있는 검색 전용이다. 이쪽은 디바운스 없이 **가드만** 필요한 경우를 위한 것.
 */

/**
 * 세대 번호로 최신 호출만 통과시키는 게이트.
 *
 * ```ts
 * const gen = new Generation();
 * $effect(() => {
 *   const mine = gen.next();
 *   api.load(id).then((v) => { if (gen.isCurrent(mine)) value = v; });
 * });
 * ```
 */
export class Generation {
	#n = 0;

	/** 새 세대를 시작하고 그 번호를 돌려준다. 이전 세대는 즉시 stale 이 된다. */
	next(): number {
		return ++this.#n;
	}

	/** `token` 이 아직 최신 세대인가. */
	isCurrent(token: number): boolean {
		return token === this.#n;
	}

	/** 진행 중인 모든 세대를 stale 로 만든다(언마운트 등). */
	cancel(): void {
		this.#n++;
	}
}
