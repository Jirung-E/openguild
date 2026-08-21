/**
 * BUG-240: "최초 적용에는 페이드를 걸지 않는다" 계약.
 *
 * **별도 파일인 이유**: vitest 는 테스트 *파일* 단위로 모듈을 격리하지만, 한
 * 파일 안에서는 `vi.resetModules()` 를 불러도 이 모듈의 최초-적용 플래그가
 * 초기화되지 않는 것을 확인했다(theme.test.ts 의 페이드 테스트가 앞 테스트의
 * 상태를 물려받아 통과하고 있었다). "이 문서에서 처음 적용하는 순간" 을
 * 정직하게 검증하려면 모듈이 진짜로 새것이어야 하므로 파일을 분리한다.
 */
import { describe, it, expect, vi, beforeEach } from 'vitest';

describe('applyThemeToDocument — 최초 적용 (BUG-240)', () => {
	beforeEach(() => {
		localStorage.clear();
		document.documentElement.removeAttribute('data-theme');
		document.documentElement.classList.remove('theme-switching');
	});

	it('첫 적용은 페이드 없이 즉시 — 이후 전환부터 페이드', async () => {
		vi.useFakeTimers();
		try {
			const m = await import('./theme');

			// 1) 앱 기동 시의 최초 적용. app.html 이 이미 맞춰둔 값을 재적용하는
			//    것뿐이라 애니메이션이 보이면 안 된다.
			m.applyThemeToDocument('light');
			expect(document.documentElement.getAttribute('data-theme')).toBe('light');
			expect(document.documentElement.classList.contains('theme-switching')).toBe(false);

			// 2) 사용자가 실제로 바꾸는 전환 — 여기서는 페이드가 걸려야 한다(BUG-239).
			m.applyThemeToDocument('dark');
			expect(document.documentElement.getAttribute('data-theme')).toBe('dark');
			expect(document.documentElement.classList.contains('theme-switching')).toBe(true);

			vi.advanceTimersByTime(400);
			expect(document.documentElement.classList.contains('theme-switching')).toBe(false);
		} finally {
			vi.useRealTimers();
		}
	});
});
