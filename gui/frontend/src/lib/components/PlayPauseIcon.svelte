<!--
  BUG-169: ▶ / ⏸ 문자 아이콘을 SVG 로 교체.

  `⏸`(U+23F8)·`▶`(U+25B6) 는 이모지 계열 코드포인트라 OS/폰트에 따라 **컬러
  이모지**로 렌더된다(사용자 보고: "일부 아이콘이 다른 운영체제에서는 컬러로
  보임" — 홈의 정지 아이콘). 폰트 폴백에 맡기면 모양·크기·색이 환경마다 달라
  버튼 정렬까지 흔들리므로, currentColor 를 쓰는 인라인 SVG 로 고정한다.

  같은 마크업이 CampaignCarousel / CampaignConveyor / QuestNodeConveyor 세 곳에
  중복돼 있었어서 컴포넌트로 뽑았다.
-->
<script lang="ts">
	let { paused = false, size = 11 }: { paused?: boolean; size?: number } = $props();

	// BUG-254: `Icon.svelte` 와 같은 함정 — `size` 는 호출측이 px 숫자로 주는데
	// 그대로 SVG 의 width/height **속성**에 넣으면 px 단위라 UI 배율(root
	// font-size)을 따라가지 않는다. 버튼은 커지는데 안의 아이콘만 그대로였다.
	//
	// 16 으로 나누므로 기본 배율에서는 기존과 정확히 같은 크기다(11 → 0.6875rem).
	const dim = $derived(`${size / 16}rem`);
</script>

{#if paused}
	<!-- 재생(▶) — 삼각형 -->
	<svg
		width={dim}
		height={dim}
		viewBox="0 0 12 12"
		fill="currentColor"
		aria-hidden="true"
		focusable="false"
	>
		<path d="M3.2 1.8 10 6l-6.8 4.2z" />
	</svg>
{:else}
	<!-- 정지(⏸) — 두 개의 세로 막대 -->
	<svg
		width={dim}
		height={dim}
		viewBox="0 0 12 12"
		fill="currentColor"
		aria-hidden="true"
		focusable="false"
	>
		<rect x="2.6" y="2" width="2.6" height="8" rx="0.6" />
		<rect x="6.8" y="2" width="2.6" height="8" rx="0.6" />
	</svg>
{/if}
