// DEV-372: 상태 변경 뒤 화면에 반영하는 규칙을 한 곳에 둔다.
//
// [[BUG-262]] 에서 상세 페이지의 상태 변경이 왕복 세 번(PATCH → 퀘스트 전체
// 재조회 → 이력 재마운트)이던 것을 한 번으로 줄였다. 핵심은 **PATCH 응답을
// 그대로 쓴다**는 것인데, 그 병합이 `+page.svelte` 안에 한 줄로 들어 있어
// 테스트가 걸리지 않았다. 누가 재조회를 되살리거나 병합을 망가뜨려도 아무것도
// 잡지 못한다 — 그래서 여기로 뺀다.

import type { Quest, QuestDetail } from '$lib/types';

/**
 * 상태 변경 응답을 상세에 반영한다.
 *
 * 서버의 `change_status` 는 갱신된 **전체 quest 행**을 돌려준다(HTTP 와 Tauri
 * invoke 가 같은 `ops::change_status` 를 거치므로 두 경로의 모양이 같다).
 * `QuestDetail` 은 `Quest` 를 확장한 것이라, 펼쳐서 덮으면 응답이 가진 기본
 * 필드만 새 값이 되고 관계·태그·첨부처럼 **상태와 무관한 것은 그대로 남는다.**
 *
 * 필드를 손으로 골라 덮는 것보다 안전하다 — 서버가 필드를 늘려도 자동으로
 * 따라온다. 반대로 응답에서 **빠진** 키는 기존 값을 유지한다(서버가
 * `skip_serializing_if` 로 빈 값을 생략하기 때문에 이 성질이 필요하다).
 */
export function applyStatusUpdate(detail: QuestDetail, updated: Quest): QuestDetail {
	return { ...detail, ...updated };
}
