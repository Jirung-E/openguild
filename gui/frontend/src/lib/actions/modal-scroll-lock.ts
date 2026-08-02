/**
 * BUG-199 후속: 모달이 떠 있는 동안 배경 페이지 스크롤 잠금 — **오버레이
 * 엘리먼트에 거는** 액션.
 *
 * 1차 수정은 퀘스트 추가 모달에서 `onMount` 로 잠갔는데, 그 방식은 "모달
 * 컴포넌트 자체가 조건부로 마운트될 때"만 맞는다. 어드민의 타입/상태/태그
 * 모달처럼 **섹션 컴포넌트는 계속 살아 있고 오버레이만 `{#if}` 로 나타나는**
 * 경우엔 잠글 시점이 없다(admin 재보고: "'타입 추가' 팝업에서 같은 현상").
 *
 * 오버레이 엘리먼트는 모달이 열려 있는 동안에만 존재하므로, 여기에 액션을
 * 걸면 열림/닫힘과 수명이 정확히 일치한다. 잠금 자체는 참조 계수라(모달 위에
 * 모달이 떠도 안전) 여러 오버레이가 겹쳐도 마지막 하나가 사라질 때 풀린다.
 *
 * 사용: `<div class="ov" use:modalScrollLock>`
 */
import { lockBodyScroll } from '$lib/utils/body-scroll-lock';

export function modalScrollLock(_node: HTMLElement) {
	const release = lockBodyScroll();
	return { destroy: release };
}
