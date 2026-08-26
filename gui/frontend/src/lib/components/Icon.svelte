<!--
  BUG-169: UI 크롬 아이콘용 인라인 SVG 세트.

  📁 📄 💬 📝 🗑 🖼 🌐 📌 ⬆ 같은 코드포인트는 **기본 emoji presentation** 이라
  OS/폰트에 따라 컬러 이모지로 렌더된다(사용자 보고: "일부 아이콘이 다른
  운영체제에서는 컬러로 보임"). 폰트 폴백에 맡기면 환경마다 모양·크기·색·기준선이
  달라져 정렬까지 흔들리므로, `currentColor` 를 쓰는 SVG 로 고정한다.

  **의도적으로 이모지를 유지하는 곳**은 교체 대상이 아니다:
   - 댓글 반응(QuestCommentsSection 의 REACTION_SET) — 반응 자체가 이모지다.
   - 사용자가 입력한 본문/제목 안의 이모지.

  사용: `<Icon name="folder" />`, 크기는 `size`(px, 기본 14).
  세로 정렬은 이 컴포넌트가 스스로 맞춘다(아래 `vertical-align: middle`) —
  호출부에서 챙길 필요 없다. 다만 글자와의 **간격**이 필요하면 부모에
  `display:inline-flex; align-items:center; gap` 을 주는 편이 낫다.
-->
<script lang="ts">
	// DEV-302: 도형은 `utils/icon-paths.ts` 단일 출처. 보드 노드처럼 SVG 를
	// 문자열로 조립하는 코드도 같은 도형을 써야 하므로 여기 인라인하지 않는다.
	import { ICON_SHAPES, type IconName } from '$lib/utils/icon-paths';

	let { name, size = 14, title }: { name: IconName; size?: number; title?: string } = $props();

	// BUG-254: `size` 는 호출측이 px 숫자로 준다(예: `size={12}`). 그대로 SVG 의
	// width/height 속성에 넣으면 **px 단위**라 UI 배율(root font-size)을 따라가지
	// 않는다 — 버튼은 커지는데 안의 아이콘만 그대로였다.
	//
	// 호출부를 전부 고치는 대신 여기서 rem 으로 환산한다. 16 으로 나누므로
	// 기본 배율에서는 **기존과 정확히 같은 크기**다(size=14 → 0.875rem = 14px).
	const px = $derived(`${size / 16}rem`);
</script>

<svg
	width={px}
	height={px}
	viewBox="0 0 16 16"
	fill="none"
	stroke="currentColor"
	stroke-width="1.3"
	stroke-linecap="round"
	stroke-linejoin="round"
	role={title ? 'img' : 'presentation'}
	aria-hidden={title ? undefined : 'true'}
	aria-label={title}
	focusable="false"
>
	{#if title}<title>{title}</title>{/if}
	<!-- eslint-disable-next-line svelte/no-at-html-tags — 도형은 icon-paths.ts 의 고정 상수(사용자 입력 아님) -->
	{@html ICON_SHAPES[name]}
</svg>

<style>
	/*
	  BUG-254 후속: 아이콘이 글자와 나란히 있을 때 세로 기준선이 어긋나던 문제.

	  인라인 SVG 의 기본 `vertical-align` 은 `baseline` 이라 **도형의 아래
	  모서리가 글자의 기준선에 얹힌다**. 글자에는 기준선 아래로 descender 가
	  더 있으므로 아이콘만 위로 떠 보인다 — 아이콘이 클수록 더 벌어진다.

	  이 파일 주석은 원래 "부모에 inline-flex; align-items:center 를 주면
	  맞는다" 고 안내했는데, 실제로는 호출부 상당수가 그걸 안 지켰다(퀘스트·
	  캠페인 상세의 삭제/배너 버튼 등). 호출부를 하나씩 고치는 대신 여기서
	  중앙 정렬을 기본값으로 준다.

	  부모가 이미 flex 인 곳에서는 SVG 가 flex item 이 되어 `vertical-align`
	  이 무시되므로 **기존 정렬에 영향이 없다** — 순수하게 추가 동작이다.
	*/
	svg {
		vertical-align: middle;
	}
</style>
