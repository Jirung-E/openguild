<!--
  DEV-417: 목록 한 줄의 **세 가지 여는 법** — 미리보기 / 새 창 / 페이지 이동.

  검색 팔레트가 먼저 이 모양을 썼다(DEV-255). 관계 목록·연관 문서도 결국 "같은 문서를 가리키는
  목록" 이라, 팔레트에서 되는 것이 여기서는 안 되면 말이 안 된다(admin). 그래서 아이콘·설명·순서를
  **한 곳에** 두고 양쪽이 같이 쓴다 — 한쪽만 고쳐져 갈라지지 않게.

  줄의 나머지(제목·칩)를 누르는 것은 여전히 페이지 이동이다. 이 버튼들은 그 위에 얹는 선택지다.
-->
<script lang="ts">
	import { locale, t } from '$lib/stores/locale';

	let {
		onpreview,
		onwindow,
		onpage
	}: { onpreview: () => void; onwindow: () => void; onpage: () => void } = $props();

	// 줄 전체가 링크라 버튼 클릭이 그 링크까지 타고 올라간다 — 여기서 끊는다.
	function only(fn: () => void) {
		return (e: MouseEvent) => {
			e.preventDefault();
			e.stopPropagation();
			fn();
		};
	}
</script>

<span class="row-actions">
	<button
		class="row-act"
		type="button"
		onclick={only(onpreview)}
		title={t('palette.preview', $locale)}
		aria-label={t('palette.preview', $locale)}
	>
		<svg
			viewBox="0 0 16 16"
			fill="none"
			stroke="currentColor"
			stroke-width="1.3"
			stroke-linecap="round"
			stroke-linejoin="round"
			aria-hidden="true"
		>
			<path d="M1.5 8S4 3.5 8 3.5 14.5 8 14.5 8 12 12.5 8 12.5 1.5 8 1.5 8Z" />
			<circle cx="8" cy="8" r="1.7" />
		</svg>
	</button>
	<button
		class="row-act"
		type="button"
		onclick={only(onwindow)}
		title={t('palette.openWindow', $locale)}
		aria-label={t('palette.openWindow', $locale)}
	>
		<svg
			viewBox="0 0 16 16"
			fill="none"
			stroke="currentColor"
			stroke-width="1.3"
			stroke-linecap="round"
			stroke-linejoin="round"
			aria-hidden="true"
		>
			<path d="M6 3H3.3a.8.8 0 0 0-.8.8v8.4a.8.8 0 0 0 .8.8h8.4a.8.8 0 0 0 .8-.8V10" />
			<path d="M9 2.5h4.5V7" />
			<path d="M13.5 2.5 7.2 8.8" />
		</svg>
	</button>
	<button
		class="row-act"
		type="button"
		onclick={only(onpage)}
		title={t('palette.goPage', $locale)}
		aria-label={t('palette.goPage', $locale)}
	>
		<svg
			viewBox="0 0 16 16"
			fill="none"
			stroke="currentColor"
			stroke-width="1.3"
			stroke-linecap="round"
			stroke-linejoin="round"
			aria-hidden="true"
		>
			<!-- BUG-244: 막대 끝을 촉 꼭짓점에 맞춘다 — 안 맞으면 `─` 와 `>` 를 겹쳐 놓은 것처럼 보인다. -->
			<path d="M3 8h9.8" />
			<path d="M9 4.2 12.8 8 9 11.8" />
		</svg>
	</button>
</span>

<style>
	.row-actions {
		flex: none;
		display: inline-flex;
		align-items: center;
		gap: 0.1rem;
	}
	.row-act {
		display: inline-flex;
		align-items: center;
		justify-content: center;
		/* BUG-244: rem 이라 UI 배율(DEV-101)을 따라간다 — px 로 두면 배율을 올릴수록 줄이 어긋난다. */
		width: 1.375rem;
		height: 1.375rem;
		color: var(--text-faint);
		background: transparent;
		border: none;
		border-radius: var(--r-sm);
		cursor: pointer;
	}
	.row-act svg {
		width: 0.8125rem;
		height: 0.8125rem;
	}
	.row-act:hover {
		background: var(--nav-hover-bg);
		color: var(--text);
	}
</style>
