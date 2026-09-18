<!--
  DEV-416: 입력창이 화면 밖으로 밀리면 **화면 아래에 붙여** 둔다.

  본문을 읽으면서 댓글을 쓰면, 글자를 칠 때마다 브라우저가 커서를 보이려고 스크롤을 입력창
  쪽으로 끌어내렸다. 그래서 본문을 보면서 쓸 수가 없었다.

  고치는 방식이 중요하다. **입력창 요소를 그대로 두고 자리만 바꾼다** — DOM 에서 옮기거나 다른
  요소로 바꿔 그리면 포커스·캐럿·조합 중인 한글·되돌리기 기록이 다 날아간다. 화면 아래에 붙어
  있으면 그 요소는 이미 화면 안이라, 브라우저가 스크롤할 이유 자체가 없어진다.

  ## 언제 붙나

      붙는다 = 이 자리가 화면 밖 && (포커스가 있거나 쓰던 글이 있다) && 닫기를 안 눌렀다

  포커스를 잃어도 **쓰던 글이 있으면 그대로 둔다**(admin 결정) — 글을 쓰다 다른 곳을 보는 일이
  흔하다. 대신 오른쪽 위 ×로 언제든 원래 자리로 돌린다(글은 남는다). 스크롤을 올려 원래 자리가
  화면에 들어오면 저절로 돌아온다.

  ## 움직임

  붙거나 떨어질 때 원래 자리와 붙는 자리 사이를 FLIP(옛 위치 → 새 위치 차이를 transform 으로
  되돌린 뒤 한 프레임 뒤에 푼다)으로 잇는다. `position` 은 전환이 안 되기 때문에, 그냥 바꾸면
  상자가 툭 튄다.
-->
<script lang="ts">
	import type { Snippet } from 'svelte';
	import { locale, t } from '$lib/stores/locale';

	let {
		/** 쓰던 글이 있나 — 포커스를 잃어도 붙여 둘지 정한다. */
		hasContent = false,
		/** 붙었을 때 상자 위에 붙일 이름(누구에게 쓰는 중인지 등). */
		label = '',
		children
	}: { hasContent?: boolean; label?: string; children: Snippet } = $props();

	let slot: HTMLDivElement | undefined = $state(undefined);
	let box: HTMLDivElement | undefined = $state(undefined);
	let inView = $state(true);
	let focused = $state(false);
	/** ×를 눌렀다 — 원래 자리가 다시 보일 때까지 안 붙는다. */
	let dismissed = $state(false);

	const docked = $derived(!inView && !dismissed && (focused || hasContent));

	// 붙은 상자는 본문 칸과 같은 폭·같은 가로 위치를 쓴다 — 창 전체로 늘리면 글 읽는 폭과
	// 어긋나 보인다.
	let rect = $state({ left: 0, width: 0 });
	function measure() {
		if (!slot) return;
		const r = slot.getBoundingClientRect();
		rect = { left: r.left, width: r.width };
	}

	$effect(() => {
		if (!slot) return;
		measure();
		// 이 기능은 **없어도 되는 편의**다 — 관찰기가 없는 환경(옛 WebView, 시험용 DOM)에서는
		// 그냥 제자리에 둔다. 여기서 던지면 댓글 화면 전체가 안 그려진다.
		if (typeof IntersectionObserver === 'undefined') return;
		const io = new IntersectionObserver(
			(entries) => {
				const e = entries[0];
				// 자리가 바뀌기 **직전**의 위치 — 아래 FLIP 이 여기서부터 이어 붙인다.
				rememberTop();
				inView = e.isIntersecting;
				// 원래 자리가 다시 보이면 ×로 닫아 둔 상태를 푼다 — 다음에 또 밀려나면 붙는다.
				if (e.isIntersecting) dismissed = false;
			},
			// 살짝 걸쳐 있는 것은 "보인다" 로 친다 — 경계에서 붙었다 떨어졌다 하지 않게.
			{ threshold: 0, rootMargin: '-48px 0px -48px 0px' }
		);
		io.observe(slot);
		const onResize = () => measure();
		window.addEventListener('resize', onResize);
		return () => {
			io.disconnect();
			window.removeEventListener('resize', onResize);
		};
	});

	// FLIP — 자리가 바뀌는 순간의 옛 위치를 들고 있다가, 새 위치에서 그만큼 되돌린 뒤 푼다.
	let prevTop: number | null = null;
	let wasDocked = false;
	$effect(() => {
		const now = docked;
		if (!box) return;
		if (now === wasDocked) return;
		const from = prevTop;
		wasDocked = now;
		measure();
		if (from == null) return;
		requestAnimationFrame(() => {
			if (!box) return;
			const to = box.getBoundingClientRect().top;
			const dy = from - to;
			if (Math.abs(dy) < 1) return;
			box.style.transition = 'none';
			box.style.transform = `translateY(${dy}px)`;
			requestAnimationFrame(() => {
				if (!box) return;
				box.style.transition = '';
				box.style.transform = '';
			});
		});
	});

	/** 자리가 바뀌기 **직전의** 위치 — 포커스/스크롤 이벤트마다 최신으로 들고 있는다. */
	function rememberTop() {
		if (box) prevTop = box.getBoundingClientRect().top;
	}

	function onFocusIn() {
		rememberTop();
		focused = true;
	}
	function onFocusOut() {
		rememberTop();
		focused = false;
	}
	function close() {
		rememberTop();
		dismissed = true;
	}
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div
	class="dock-slot"
	class:reserved={docked}
	bind:this={slot}
	onfocusin={onFocusIn}
	onfocusout={onFocusOut}
>
	<div
		class="dock-box"
		class:docked
		bind:this={box}
		style:left={docked ? `${rect.left}px` : undefined}
		style:width={docked ? `${rect.width}px` : undefined}
	>
		{#if docked}
			<div class="dock-head">
				<span class="dock-label">{label || t('compose.docked', $locale)}</span>
				<button
					class="dock-close"
					onclick={close}
					title={t('compose.undock', $locale)}
					aria-label={t('compose.undock', $locale)}>×</button
				>
			</div>
		{/if}
		{@render children()}
	</div>
</div>

<style>
	.dock-slot {
		/* 붙어 있는 동안 원래 자리가 접히면 그 위의 글이 밀린다 — 높이를 잡아 둔다. */
		min-height: 0;
	}
	.dock-slot.reserved {
		min-height: 8rem;
	}
	.dock-box {
		transition: transform 0.18s ease-out;
	}
	.dock-box.docked {
		position: fixed;
		bottom: 0.75rem;
		z-index: 1100; /* 검색 팔레트(1200)보다 아래, 본문 위. */
		background: var(--bg-elevated);
		border: var(--bw) solid var(--border);
		border-radius: var(--r-lg);
		box-shadow: 0 8px 28px var(--shadow);
		padding: 0.6rem 0.75rem;
		/* 긴 편집기(메모)가 화면을 통째로 덮지 않게. 안쪽 편집기는 자기 스크롤을 쓴다. */
		max-height: 60vh; /* 미지원 브라우저 폴백 — 먼저 */
		max-height: 60dvh;
		overflow: auto;
	}
	.dock-head {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 0.5rem;
		margin-bottom: 0.35rem;
	}
	.dock-label {
		font-size: 0.8rem;
		color: var(--text-muted);
	}
	.dock-close {
		border: none;
		background: none;
		color: var(--text-muted);
		font-size: 1.1rem;
		line-height: 1;
		padding: 0.15rem 0.4rem;
		border-radius: var(--r-sm);
		cursor: pointer;
	}
	.dock-close:hover {
		background: var(--bg-elevated);
		color: var(--text);
	}
</style>
