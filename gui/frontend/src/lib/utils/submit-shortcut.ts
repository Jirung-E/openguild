// DEV-424: 입력칸에서 **Ctrl+Enter**(맥은 ⌘Enter 도)로 바로 보낸다 — 손을 키보드에서 안 떼고.
// 받는 곳은 ComposeDock 하나다(댓글 · 답글 · 메모가 같이 쓴다).

type Keyish = Pick<
	KeyboardEvent,
	'key' | 'ctrlKey' | 'metaKey' | 'shiftKey' | 'altKey' | 'isComposing' | 'keyCode'
>;

/**
 * Ctrl+Enter / ⌘Enter 인가. Shift · Alt 가 같이 눌렸으면 아니다 — 다른 단축키가 쓸 수 있다.
 *
 * **한글 조합 중의 Enter 는 아니다.** 그 Enter 는 글자를 확정하는 키라, 보내기로 받으면 쓰다 만 댓글이
 * 달린다. `keyCode` 229 는 조합 중임을 알리는 옛 방식이다(`isComposing` 을 안 싣는 웹뷰가 있다).
 */
export function isSubmitShortcut(e: Keyish): boolean {
	if (e.key !== 'Enter') return false;
	if (e.isComposing || e.keyCode === 229) return false;
	return (e.ctrlKey || e.metaKey) && !e.shiftKey && !e.altKey;
}

/** 단추 툴팁에 붙일 이름 — 단축키는 보이지 않으면 없는 것과 같다. */
export function submitShortcutLabel(
	ua = typeof navigator === 'undefined' ? '' : navigator.userAgent
): string {
	return /Mac|iPhone|iPad/.test(ua) ? '⌘Enter' : 'Ctrl+Enter';
}
