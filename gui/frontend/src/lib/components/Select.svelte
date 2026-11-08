<!--
  DEV-440 후속: 고르기(select) 하나 — 상자는 공통 `.input`(작으면 `.input.sm`), 꺾쇠는 작은 삼각형 아이콘.

  꺾쇠를 select 의 배경 그림으로 그리면 글자색을 따라가지 못한다(배경 그림은 currentColor 를 모른다). 그래서
  select 를 감싸 꺾쇠 아이콘을 위에 얹는다 — 꺾쇠는 **글자와 같은 색**이라 글자가 진하면 꺾쇠도 진하다
  (admin: 회색 꺾쇠는 "안 펼쳐질 것 같이 생겼다"). 모양은 보드 도구줄에서 쓰던 삼각형(`caret-down-sm`).

  - `class` 는 **감싼 상자**에 붙는다. 색 · 폭은 상자에 주면 select 와 꺾쇠가 함께 따른다.
    배경처럼 select 자체에 줄 것은 부르는 쪽에서 `:global(.내-클래스 select)` 로.
  - 나머지 속성(title · aria-label · id · disabled · onchange …)은 select 로 간다.
-->
<script lang="ts">
	import type { Snippet } from 'svelte';
	import type { HTMLSelectAttributes } from 'svelte/elements';
	import Icon from './Icon.svelte';

	type Props = Omit<HTMLSelectAttributes, 'class' | 'value' | 'children'> & {
		value?: unknown;
		/** 작은 줄(도구줄 · 목록 머리) — `--ctl-h-sm` 높이. */
		small?: boolean;
		class?: string;
		children: Snippet;
	};

	let { value = $bindable(), small = false, class: cls = '', children, ...rest }: Props = $props();
</script>

<span class="select-box {cls}" class:sm={small}>
	<select class="input" class:sm={small} bind:value {...rest}>{@render children()}</select>
	<span class="select-caret" aria-hidden="true"><Icon name="caret-down-sm" size="1.15em" /></span>
</span>

<style>
	.select-box {
		position: relative;
		display: inline-flex;
		color: var(--text);
		font-size: var(--fs-sm);
	}
	.select-box > select {
		flex: 1;
		min-width: 0;
		color: inherit;
	}
	.select-caret {
		position: absolute;
		right: var(--sp-4);
		top: 50%;
		transform: translateY(-50%);
		display: flex;
		pointer-events: none;
	}
	.select-box:has(select:disabled) .select-caret {
		opacity: 0.5;
	}
</style>
