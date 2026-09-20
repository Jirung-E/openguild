<!--
  DEV-416: 입력창이 화면 밖으로 밀리면 **화면 아래에 붙여** 둔다.

  본문을 읽으면서 댓글을 쓰면, 글자를 칠 때마다 브라우저가 커서를 보이려고 스크롤을 입력창
  쪽으로 끌어내렸다. 그래서 본문을 보면서 쓸 수가 없었다.

  고치는 방식이 중요하다. **입력창 요소를 그대로 두고 자리만 바꾼다** — DOM 에서 옮기거나 다른
  요소로 바꿔 그리면 포커스·캐럿·조합 중인 한글·되돌리기 기록이 다 날아간다. 화면 아래에 붙어
  있으면 그 요소는 이미 화면 안이라, 브라우저가 스크롤할 이유 자체가 없어진다.

  ## 언제 붙나

      붙는다 = 이 자리가 **조금이라도** 가려짐 && (포커스가 있거나 쓴 글이 있다) && 닫기를 안 눌렀다

  처음에는 "완전히 화면 밖" 일 때만 붙였는데, 반쯤 걸친 상태가 제일 불편하다(admin) — 입력칸은
  보이는데 커서 줄이 안 보여서 결국 스크롤이 튄다. 그래서 **한 귀퉁이라도 잘리면** 붙인다.

  포커스를 잃어도 **쓰던 글이 있으면 그대로 둔다**(admin) — 글을 쓰다 다른 곳을 보는 일이 흔하다.
  대신 오른쪽 위 ×로 언제든 원래 자리로 돌린다(글은 남는다).

  ## 붙었을 때의 모양

  오른쪽 위에 동그란 버튼 셋 — **보내기 · 컴팩트 · 닫기**(admin). 보내기를 위로 올리면 상자
  아래쪽의 빈 자리가 사라진다. 예전에는 버튼 하나 때문에 그 줄이 통째로 남아 화면을 먹었다.

  **팝업 자체에는 스크롤이 없다.** 길어지는 것은 입력칸이고, 스크롤도 그 안에서만 생겨야 한다 —
  바깥이 스크롤되면 버튼이 밀려 올라가 안 보인다.
-->
<script lang="ts">
	import type { Snippet } from 'svelte';
	import { locale, t } from '$lib/stores/locale';

	let {
		/** 쓰던 글이 있나 — 포커스를 잃어도 붙여 둘지 정한다. */
		hasContent = false,
		/** 붙었을 때 상자 위에 붙일 이름(누구에게 쓰는 중인지 등). */
		label = '',
		/** 붙었을 때 머리줄의 동그란 보내기 버튼. 없으면 안 그린다. */
		onsubmit,
		submitTitle = '',
		submitDisabled = false,
		children
	}: {
		hasContent?: boolean;
		label?: string;
		onsubmit?: () => void;
		submitTitle?: string;
		submitDisabled?: boolean;
		children: Snippet;
	} = $props();

	let slot: HTMLDivElement | undefined = $state(undefined);
	let box: HTMLDivElement | undefined = $state(undefined);
	let fullyVisible = $state(true);
	let focused = $state(false);
	/** ×를 눌렀다 — 원래 자리가 다시 보일 때까지 안 붙는다. */
	let dismissed = $state(false);
	/** 좁게 보기 — 입력칸만 남기고 높이를 줄인다. */
	let compact = $state(false);

	const docked = $derived(!fullyVisible && !dismissed && (focused || hasContent));

	// 붙은 상자는 본문 칸과 같은 폭·같은 가로 위치를 쓴다 — 창 전체로 늘리면 글 읽는 폭과
	// 어긋나 보인다.
	let rect = $state({ left: 0, width: 0 });
	function measure() {
		if (!slot) return;
		const r = slot.getBoundingClientRect();
		rect = { left: r.left, width: r.width };
	}

	/** 자리가 바뀌기 **직전**의 위치 — 아래 FLIP 이 여기서부터 이어 붙인다. */
	let prevTop: number | null = null;
	function rememberTop() {
		if (box) prevTop = box.getBoundingClientRect().top;
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
				rememberTop();
				// **한 귀퉁이라도 잘리면** 붙는다(admin) — 반쯤 걸친 상태가 제일 불편하다.
				fullyVisible = e.intersectionRatio >= 0.99;
				// 원래 자리가 다시 온전히 보이면 ×로 닫아 둔 상태를 푼다.
				if (fullyVisible) dismissed = false;
			},
			// 1 하나만 두면 "완전히 보임 → 아님" 을 놓칠 수 있다(경계에서 콜백이 안 온다).
			{ threshold: [0, 0.99, 1] }
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
	// `position` 은 전환이 안 되기 때문에, 그냥 바꾸면 상자가 툭 튄다.
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
		class:compact={docked && compact}
		bind:this={box}
		style:left={docked ? `${rect.left}px` : undefined}
		style:width={docked ? `${rect.width}px` : undefined}
	>
		{#if docked}
			<div class="dock-head">
				<span class="dock-label">{label || t('compose.docked', $locale)}</span>
				<!-- admin: 오른쪽 위에 동그란 버튼 셋 — 보내기 · 컴팩트 · 닫기. -->
				<span class="dock-btns">
					{#if onsubmit}
						<button
							class="dock-btn"
							type="button"
							onclick={onsubmit}
							disabled={submitDisabled}
							title={submitTitle || t('compose.send', $locale)}
							aria-label={submitTitle || t('compose.send', $locale)}>↵</button
						>
					{/if}
					<button
						class="dock-btn"
						type="button"
						onclick={() => (compact = !compact)}
						aria-pressed={compact}
						title={compact ? t('compose.expand', $locale) : t('compose.compact', $locale)}
						aria-label={compact ? t('compose.expand', $locale) : t('compose.compact', $locale)}
						>{compact ? '⌃' : '⌄'}</button
					>
					<button
						class="dock-btn"
						type="button"
						onclick={close}
						title={t('compose.undock', $locale)}
						aria-label={t('compose.undock', $locale)}>×</button
					>
				</span>
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
		padding: 0.5rem 0.6rem 0.6rem;
	}
	/* admin: 보내기 버튼이 머리줄로 올라갔으니 아래 버튼 줄은 자리만 먹는다 — 접는다. */
	.dock-box.docked :global(.actions) {
		display: none;
	}
	/* **팝업은 안 스크롤된다.** 길어지는 것은 입력칸이고, 스크롤도 그 안에서만 생긴다 —
	   바깥이 스크롤되면 머리줄의 버튼이 밀려 올라가 안 보인다. */
	.dock-box.docked :global(textarea),
	.dock-box.docked :global(.cm-scroller) {
		max-height: 40vh; /* 미지원 브라우저 폴백 — 먼저 */
		max-height: 40dvh;
		overflow: auto;
	}
	/* 좁게 보기 = **입력칸만**(admin: "댓글 입력창만 뜨는 것"). 작성자 칸과 편집기 토글은
	   접는다 — 붙어 있는 동안 화면을 먹는 것이 그 줄이다. 입력칸 높이도 더 줄인다. */
	.dock-box.compact :global(.new-row),
	.dock-box.compact :global(.reply-author) {
		display: none;
	}
	.dock-box.compact :global(textarea),
	.dock-box.compact :global(.cm-scroller) {
		max-height: 4.5rem;
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
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.dock-btns {
		flex: none;
		display: inline-flex;
		align-items: center;
		gap: 0.25rem;
	}
	.dock-btn {
		display: inline-flex;
		align-items: center;
		justify-content: center;
		width: 1.5rem;
		height: 1.5rem;
		border: var(--bw) solid var(--border);
		border-radius: var(--r-pill);
		background: var(--bg);
		color: var(--text-muted);
		font-size: 0.85rem;
		line-height: 1;
		cursor: pointer;
	}
	.dock-btn:hover:not(:disabled) {
		background: var(--nav-hover-bg);
		color: var(--text);
	}
	.dock-btn:disabled {
		opacity: 0.45;
		cursor: default;
	}
</style>
