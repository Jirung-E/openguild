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
  텍스트와 함께 쓸 때는 부모에 `display:inline-flex; align-items:center; gap`
  을 주면 기준선이 맞는다.
-->
<script lang="ts">
	type IconName =
		| 'folder'
		| 'doc'
		| 'comment'
		| 'memo'
		| 'trash'
		| 'image'
		| 'globe'
		| 'pin'
		| 'up'
		| 'clock'
		| 'link'
		| 'tag';

	let {
		name,
		size = 14,
		title
	}: { name: IconName; size?: number; title?: string } = $props();
</script>

<svg
	width={size}
	height={size}
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
	{#if name === 'folder'}
		<path d="M2 4.6a1 1 0 0 1 1-1h3l1.3 1.4H13a1 1 0 0 1 1 1v5.4a1 1 0 0 1-1 1H3a1 1 0 0 1-1-1z" />
	{:else if name === 'doc'}
		<path d="M4 2.2h5L12.4 5.6v8.2H4z" />
		<path d="M9 2.2v3.4h3.4" />
	{:else if name === 'comment'}
		<path d="M2.4 3.6a1 1 0 0 1 1-1h9.2a1 1 0 0 1 1 1v5.6a1 1 0 0 1-1 1H6.4L3.2 13V10.2h-.8a1 1 0 0 1-1-1z" />
	{:else if name === 'memo'}
		<path d="M3.4 2.4h6.2l3 3v8.2h-9.2z" />
		<path d="M5.4 7.4h5.2M5.4 9.8h5.2" />
	{:else if name === 'trash'}
		<path d="M2.8 4.4h10.4M6.2 4.4V2.9h3.6v1.5" />
		<path d="M4.2 4.4l.7 8.7h6.2l.7-8.7" />
	{:else if name === 'image'}
		<rect x="2.2" y="3.2" width="11.6" height="9.6" rx="1" />
		<circle cx="5.8" cy="6.4" r="1.1" />
		<path d="M2.6 11.4 6.4 8l2.4 2.2 2-1.8 2.6 2.6" />
	{:else if name === 'globe'}
		<circle cx="8" cy="8" r="5.7" />
		<ellipse cx="8" cy="8" rx="2.4" ry="5.7" />
		<path d="M2.5 8h11" />
	{:else if name === 'pin'}
		<path d="M6 2.4h4M8 2.4v4.2M5 6.6h6l-.7 2.6H5.7z" />
		<path d="M8 9.2v4.4" />
	{:else if name === 'up'}
		<path d="M8 12.6V3.9M4.4 7.5 8 3.9l3.6 3.6" />
	{:else if name === 'clock'}
		<circle cx="8" cy="8" r="5.6" />
		<path d="M8 4.6V8l2.4 1.5" />
	{:else if name === 'link'}
		<path d="M6.6 9.4 9.4 6.6" />
		<path d="M7.1 4.9 8.5 3.5a2.6 2.6 0 0 1 3.7 3.7l-1.4 1.4" />
		<path d="M8.9 11.1 7.5 12.5a2.6 2.6 0 0 1-3.7-3.7l1.4-1.4" />
	{:else if name === 'tag'}
		<path d="M8.4 2.4H13a.6.6 0 0 1 .6.6v4.6l-6 6a.9.9 0 0 1-1.3 0L2.4 9.7a.9.9 0 0 1 0-1.3z" />
		<circle cx="10.8" cy="5.2" r="0.9" />
	{/if}
</svg>
