// DEV-335: 첨부 이미지 HDR 표시 제어.
//
// Chromium 의 `dynamic-range-limit` CSS 속성으로 이미지가 디스플레이의 HDR
// 헤드룸을 얼마나 쓸지 제한한다 — admin 보고: 폰 화면에서 첨부 이미지가
// 과도하게 밝게(HDR) 보임.
//
// 값:
//   - 'no-limit'    HDR 전부 사용 (기본 — 지금까지의 동작과 동일)
//   - 'constrained' 제한적으로 사용
//   - 'standard'    HDR 안 씀 (SDR 로 톤매핑)
//
// 지원 여부는 화면/브라우저에 달렸다 — `isHdrLimitSupported()` 로 확인해
// 미지원이면 설정 UI 자체를 숨긴다(`CSS.supports` 미지원 시 항목을 켜봤자
// 아무 효과 없이 사용자만 혼란).
//
// 적용: `applyHdrLimitToDocument` 가 `<html>` 에 `--hdr-limit` custom property
// 를 써서, `.md :global(img)`(MarkdownView) / `.thumb img`(AttachmentSection)
// 양쪽이 같은 값을 `dynamic-range-limit: var(--hdr-limit, no-limit)` 로 참조.

import { writable } from 'svelte/store';

export type HdrLimit = 'standard' | 'constrained' | 'no-limit';

const KEY = 'openguild.hdrLimit';
export const DEFAULT_HDR_LIMIT: HdrLimit = 'no-limit';
const VALID: readonly HdrLimit[] = ['standard', 'constrained', 'no-limit'];

function loadInitial(): HdrLimit {
	if (typeof localStorage === 'undefined') return DEFAULT_HDR_LIMIT;
	try {
		const raw = localStorage.getItem(KEY);
		return (VALID as readonly string[]).includes(raw ?? '') ? (raw as HdrLimit) : DEFAULT_HDR_LIMIT;
	} catch {
		return DEFAULT_HDR_LIMIT;
	}
}

export const hdrLimit = writable<HdrLimit>(loadInitial());

hdrLimit.subscribe((v) => {
	if (typeof localStorage === 'undefined') return;
	try {
		localStorage.setItem(KEY, v);
	} catch {
		/* storage full / disabled — 무시. */
	}
});

export function setHdrLimit(v: HdrLimit) {
	hdrLimit.set(v);
}

/** 이 브라우저/엔진이 `dynamic-range-limit` 을 지원하는지. 미지원이면 설정 UI 숨김용. */
export function isHdrLimitSupported(): boolean {
	return (
		typeof CSS !== 'undefined' &&
		typeof CSS.supports === 'function' &&
		CSS.supports('dynamic-range-limit', 'standard')
	);
}

/** `<html>` 의 `--hdr-limit` custom property 갱신 — img 스타일이 이를 참조. */
export function applyHdrLimitToDocument(v: HdrLimit) {
	if (typeof document === 'undefined') return;
	document.documentElement.style.setProperty('--hdr-limit', v);
}
