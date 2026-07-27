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
</script>

{#if paused}
	<!-- 재생(▶) — 삼각형 -->
	<svg
		width={size}
		height={size}
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
		width={size}
		height={size}
		viewBox="0 0 12 12"
		fill="currentColor"
		aria-hidden="true"
		focusable="false"
	>
		<rect x="2.6" y="2" width="2.6" height="8" rx="0.6" />
		<rect x="6.8" y="2" width="2.6" height="8" rx="0.6" />
	</svg>
{/if}
