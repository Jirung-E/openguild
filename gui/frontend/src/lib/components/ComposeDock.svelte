<!--
  DEV-416: 입력창이 화면 밖으로 밀리면 **화면 아래에 붙여** 둔다.

  본문을 읽으면서 댓글을 쓰면, 글자를 칠 때마다 브라우저가 커서를 보이려고 스크롤을 입력창
  쪽으로 끌어내렸다. 그래서 본문을 보면서 쓸 수가 없었다.

  고치는 방식이 중요하다. **입력창 요소를 그대로 두고 자리만 바꾼다** — DOM 에서 옮기거나 다른
  요소로 바꿔 그리면 포커스·캐럿·조합 중인 한글·되돌리기 기록이 다 날아간다. 화면 아래에 붙어
  있으면 그 요소는 이미 화면 안이라, 브라우저가 스크롤할 이유 자체가 없어진다.

  ## 언제 붙나

      붙는다 = **입력칸**이 조금이라도 가려짐 && (포커스가 있거나 쓴 글이 있다) && 닫기를 안 눌렀다

  처음에는 "완전히 화면 밖" 일 때만 붙였는데, 반쯤 걸친 상태가 제일 불편하다(admin) — 입력칸은
  보이는데 커서 줄이 안 보여서 결국 스크롤이 튄다. 그래서 **한 귀퉁이라도 잘리면** 붙인다.

  재는 것은 **입력칸 하나**다(admin). 예전에는 이 상자 전체를 쟀는데, 그러면 작성자 칸이나
  아래 [댓글 추가] 버튼이 화면을 벗어나기만 해도 팝업이 떴다 — 정작 글 쓰는 칸은 멀쩡히
  보이는데도.

  포커스를 잃어도 **쓰던 글이 있으면 그대로 둔다**(admin) — 글을 쓰다 다른 곳을 보는 일이 흔하다.
  대신 오른쪽 위 ×로 언제든 원래 자리로 돌린다(글은 남는다).

  ## 붙었을 때의 모양

  **기본이 좁게 보기다**(admin). 좁게 보기는 **배경이 없다** — 입력칸과 동그란 버튼 셋만
  뜬다. 카드 배경·테두리·'작성 중' 글자는 그 모양을 해친다. 넓게 펴면 작성자 칸까지 보이는
  카드가 된다.

  버튼 셋은 **보내기 · 좁게/넓게 · 닫기**(admin). 보내기를 위로 올리면 상자 아래쪽의 빈 자리가
  사라진다. 예전에는 버튼 하나 때문에 그 줄이 통째로 남아 화면을 먹었다.

  좁게/넓게는 **다시 켜도 그대로다**(admin) — 한 번 정해 두면 계속 그 모양으로 뜬다.

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
	/** BUG-315: 입력칸이 돌아갈 자리를 가리키는 표식 — 붙어 있는 동안에도 여기 남는다. */
	let probe: HTMLDivElement | undefined = $state(undefined);
	/**
	 * 붙기 **직전** 상자의 높이. 붙으면 상자가 흐름에서 빠지므로 그만큼 자리를 비워 둬야
	 * 아래 내용이 안 올라온다.
	 *
	 * BUG: 예전에는 고정값(8rem)이었다. 실제 높이와 다르면 붙는 순간 레이아웃이 움직이고,
	 * 그 움직임이 "이 자리가 보이나" 를 뒤집어 **붙었다 떨어졌다를 반복**했다(admin: 특정
	 * 높이에서 떨림). 실제 높이를 그대로 잡아 두면 붙어도 레이아웃이 그대로라 안 흔들린다.
	 */
	let reservedH = $state(0);
	let box: HTMLDivElement | undefined = $state(undefined);
	let fullyVisible = $state(true);
	let focused = $state(false);
	/** ×를 눌렀다 — 원래 자리가 다시 보일 때까지 안 붙는다. */
	let dismissed = $state(false);
	/**
	 * 좁게 보기 — 입력칸과 버튼만 남긴다.
	 *
	 * **기본이 켜짐**이고, 끄고 켠 것은 다시 켜도 남는다(admin). 앱 전체에 하나뿐인 취향이라
	 * 길드별로 나누지 않는다.
	 */
	const COMPACT_KEY = 'openguild.composeCompact';
	function loadCompact(): boolean {
		try {
			// 적어 둔 적이 없으면 켜짐 — 꺼 둔 사람만 'false' 가 적혀 있다.
			return localStorage.getItem(COMPACT_KEY) !== 'false';
		} catch {
			return true;
		}
	}
	let compact = $state(loadCompact());
	function toggleCompact() {
		compact = !compact;
		try {
			localStorage.setItem(COMPACT_KEY, String(compact));
		} catch {
			/* 무시 — 못 적어도 이번 판에서는 동작한다. */
		}
	}

	/**
	 * 붙는 판단에는 **되먹임 고리**가 있다 — 붙으면 상자가 흐름에서 빠지고, 그러면 이 자리가
	 * 다시 보여서 떨어지고, 떨어지면 또 가려진다. admin 이 본 "특정 높이에서 떨림" 이 그것이다.
	 *
	 * 고리를 끊는 것은 위의 **정확한 높이 예약**이다(레이아웃이 안 변하면 판단도 안 뒤집힌다).
	 * 여기 머무름 시간은 그 위의 안전망이다 — 그래도 남는 흔들림이 있으면 눈에 안 띄게
	 * 눌러 준다. 사람이 스크롤해서 바꾸는 속도(수백 ms)보다는 짧다.
	 */
	const SETTLE_MS = 250;
	let lastFlip = 0;
	let sticky = $state(false);
	const want = $derived(!fullyVisible && !dismissed && (focused || hasContent));
	$effect(() => {
		const w = want;
		if (w === sticky) return;
		const wait = Math.max(0, SETTLE_MS - (Date.now() - lastFlip));
		if (wait === 0) {
			lastFlip = Date.now();
			sticky = w;
			return;
		}
		const t = setTimeout(() => {
			lastFlip = Date.now();
			sticky = want; // 기다리는 사이 또 바뀌었을 수 있다 — 그때의 값을 쓴다.
		}, wait);
		return () => clearTimeout(t);
	});
	const docked = $derived(sticky);

	// 붙은 상자는 본문 칸과 같은 폭·같은 가로 위치를 쓴다 — 창 전체로 늘리면 글 읽는 폭과
	// 어긋나 보인다.
	let rect = $state({ left: 0, width: 0 });
	/**
	 * BUG-315: 입력칸이 **제자리에 있을 때** 어디였나 — 슬롯 위에서 잰 값.
	 *
	 * 관찰 대상을 입력칸 자체로 두면 안 된다. 붙는 순간 상자가 화면 아래로 옮겨 가고, 그
	 * 안에 든 입력칸도 같이 옮겨 가 **보이게** 된다. 그러면 "이제 보이니 떨어져라" 가 되고,
	 * 떨어지면 다시 안 보이고 — 끝없이 떴다 내려간다(admin). BUG-312 에서 대상을 입력칸으로
	 * 좁히며 이 고리를 만들었다.
	 *
	 * 그래서 **제자리에 남는 표식**을 슬롯 안에 두고 그것을 본다. 슬롯은 붙어 있는 동안에도
	 * 흐름에 남아 자리를 지키므로(예약 높이), 표식은 "입력칸이 돌아갈 자리" 를 그대로 가리킨다.
	 * 재는 뜻은 입력칸 그대로이고, 움직이지 않으니 고리도 없다.
	 */
	let probeBox = $state({ top: 0, height: 0 });
	function measure() {
		if (!slot) return;
		const r = slot.getBoundingClientRect();
		rect = { left: r.left, width: r.width };
		// 붙어 있는 동안에는 재지 않는다 — 그때 입력칸은 제자리에 없다.
		if (docked) return;
		const el = inputEl();
		if (!el) return;
		const ir = el.getBoundingClientRect();
		probeBox = { top: ir.top - r.top, height: ir.height };
	}

	/** 자리가 바뀌기 **직전**의 위치 — 아래 FLIP 이 여기서부터 이어 붙인다. */
	let prevTop: number | null = null;
	function rememberTop() {
		if (!box) return;
		const r = box.getBoundingClientRect();
		prevTop = r.top;
		// 붙어 있는 동안에는 재지 않는다 — 그때 높이는 띄운 상자의 높이다.
		if (!docked) reservedH = r.height;
	}

	/**
	 * 재는 대상 — **글 쓰는 칸 하나**다(admin).
	 *
	 * 예전에는 이 상자 전체를 쟀다. 그러면 작성자 칸이나 아래 [댓글 추가] 버튼이 화면을
	 * 벗어나기만 해도 팝업이 떴다 — 정작 입력칸은 멀쩡히 보이는데도.
	 *
	 * 편집기 토글(M↓)을 누르면 `textarea` 와 CodeMirror 가 서로 갈린다. 그래서 요소를
	 * 붙잡아 두지 않고 그때그때 찾는다.
	 */
	function inputEl(): Element | null {
		return slot?.querySelector('textarea, .cm-editor') ?? null;
	}

	$effect(() => {
		if (!slot) return;
		measure();
		// 이 기능은 **없어도 되는 편의**다 — 관찰기가 없는 환경(옛 WebView, 시험용 DOM)에서는
		// 그냥 제자리에 둔다. 여기서 던지면 댓글 화면 전체가 안 그려진다.
		if (typeof IntersectionObserver === 'undefined') return;
		const io = new IntersectionObserver(
			(entries) => {
				const e = entries[entries.length - 1];
				rememberTop();
				// **한 귀퉁이라도 잘리면** 붙는다(admin) — 반쯤 걸친 상태가 제일 불편하다.
				fullyVisible = e.intersectionRatio >= 0.99;
				// 원래 자리가 다시 온전히 보이면 ×로 닫아 둔 상태를 푼다.
				if (fullyVisible) dismissed = false;
			},
			// 1 하나만 두면 "완전히 보임 → 아님" 을 놓칠 수 있다(경계에서 콜백이 안 온다).
			{ threshold: [0, 0.99, 1] }
		);
		// BUG-315: 보는 것은 **표식**이다 — 입력칸 자체가 아니다(위 `probeBox` 참고).
		// 표식이 아직 안 그려졌으면(시험용 DOM 등) 입력칸이라도 본다.
		let watched: Element | null = null;
		const retarget = () => {
			const el = probe ?? inputEl();
			if (el === watched) return;
			if (watched) io.unobserve(watched);
			watched = el;
			if (watched) io.observe(watched);
			// 볼 것이 아예 없으면 붙을 이유도 없다.
			else fullyVisible = true;
		};
		retarget();
		// 편집기 토글(M↓)로 입력칸이 갈리면 크기가 달라진다 — 표식을 다시 맞춘다.
		const mo =
			typeof MutationObserver === 'undefined'
				? null
				: new MutationObserver(() => {
						retarget();
						measure();
					});
		mo?.observe(slot, { childList: true, subtree: true });
		const onResize = () => measure();
		window.addEventListener('resize', onResize);
		// 글을 치면 입력칸이 자란다 — DOM 이 안 바뀌므로 위 관찰기로는 못 본다. 표식이
		// 실제 크기를 따라가게 슬롯 크기를 지켜본다.
		const ro =
			typeof ResizeObserver === 'undefined' ? null : new ResizeObserver(() => measure());
		ro?.observe(slot);
		return () => {
			io.disconnect();
			mo?.disconnect();
			ro?.disconnect();
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
	function onFocusOut(e: FocusEvent) {
		// admin: [좁게 보기] 를 누르면 팝업이 그냥 사라졌다. 버튼을 누르는 순간 입력칸이
		// 포커스를 잃고, 쓴 글이 없으면 붙어 있을 이유가 사라지기 때문이다.
		// **상자 안으로 가는 포커스는 잃은 것이 아니다.**
		const to = e.relatedTarget;
		if (to instanceof Node && box?.contains(to)) return;
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
	style:min-height={docked ? `${reservedH}px` : undefined}
	bind:this={slot}
	onfocusin={onFocusIn}
	onfocusout={onFocusOut}
>
	<!-- BUG-315: 붙어 있는 동안에도 **제자리에 남는** 표식. 보이지도 눌리지도 않는다 —
	     "입력칸이 돌아갈 자리가 보이나" 를 재는 데만 쓴다. -->
	<div
		class="dock-probe"
		aria-hidden="true"
		bind:this={probe}
		style:top={`${probeBox.top}px`}
		style:height={`${probeBox.height}px`}
	></div>
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
				<!-- 좁게 보기에서는 이름표가 없다(admin) — 입력칸과 버튼만 남긴다. -->
				{#if !compact}
					<span class="dock-label">{label || t('compose.docked', $locale)}</span>
				{/if}
				<!-- admin: 오른쪽 위에 동그란 버튼 셋 — 보내기 · 좁게/넓게 · 닫기. -->
				<!-- 누를 때 입력칸의 포커스를 뺏지 않는다 — 캐럿과 조합 중인 글자를 지킨다. -->
				<!-- svelte-ignore a11y_no_static_element_interactions -->
				<span class="dock-btns" onmousedown={(e) => e.preventDefault()}>
					{#if onsubmit}
						<button
							class="dock-btn send"
							type="button"
							onclick={onsubmit}
							disabled={submitDisabled}
							title={submitTitle || t('compose.send', $locale)}
							aria-label={submitTitle || t('compose.send', $locale)}
						>
							<!-- BUG-312: 글자(↵ × ⌃)로 그리면 글꼴마다 기준선이 달라 동그라미 안에서
							     제각각 치우친다(admin). 그림으로 그리면 가운데가 가운데다. -->
							<svg
								viewBox="0 0 16 16"
								fill="none"
								stroke="currentColor"
								stroke-width="1.6"
								stroke-linecap="round"
								stroke-linejoin="round"
								aria-hidden="true"
							>
								<path d="M8 12.5V4" />
								<path d="M4.4 7.6 8 4l3.6 3.6" />
							</svg>
						</button>
					{/if}
					<button
						class="dock-btn"
						type="button"
						onclick={toggleCompact}
						aria-pressed={compact}
						title={compact ? t('compose.expand', $locale) : t('compose.compact', $locale)}
						aria-label={compact ? t('compose.expand', $locale) : t('compose.compact', $locale)}
					>
						<!-- 좁게 보기면 '펴기'(∨ 두 겹이 벌어지는 모양), 넓게면 '접기'(∧). -->
						<svg
							viewBox="0 0 16 16"
							fill="none"
							stroke="currentColor"
							stroke-width="1.6"
							stroke-linecap="round"
							stroke-linejoin="round"
							aria-hidden="true"
						>
							{#if compact}
								<path d="M4.5 6.2 8 2.7l3.5 3.5" />
								<path d="M4.5 9.8 8 13.3l3.5-3.5" />
							{:else}
								<path d="M4.5 3.2 8 6.7l3.5-3.5" />
								<path d="M4.5 12.8 8 9.3l3.5 3.5" />
							{/if}
						</svg>
					</button>
					<button
						class="dock-btn"
						type="button"
						onclick={close}
						title={t('compose.undock', $locale)}
						aria-label={t('compose.undock', $locale)}
					>
						<svg
							viewBox="0 0 16 16"
							fill="none"
							stroke="currentColor"
							stroke-width="1.6"
							stroke-linecap="round"
							stroke-linejoin="round"
							aria-hidden="true"
						>
							<path d="M4.6 4.6 11.4 11.4" />
							<path d="M11.4 4.6 4.6 11.4" />
						</svg>
					</button>
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
		/* 표식이 이 안에서 자리를 잡는다. */
		position: relative;
	}
	.dock-probe {
		position: absolute;
		left: 0;
		right: 0;
		pointer-events: none;
		visibility: hidden;
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
	/* BUG-312: 좁게 보기에는 **배경이 없다**(admin). 입력칸과 동그란 버튼만 떠야 하는데,
	   카드의 배경·테두리·그림자·여백이 그 둘을 한 번 더 감쌌다. 입력칸은 제 테두리를 가지고
	   있으므로 카드를 걷어도 제 모양이 남는다. */
	.dock-box.docked.compact {
		background: none;
		border-color: transparent;
		box-shadow: none;
		padding: 0;
	}
	.dock-head {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 0.5rem;
		margin-bottom: 0.35rem;
	}
	/* 이름표가 없으니 버튼이 왼쪽으로 붙는다 — 오른쪽 위로 돌려놓는다. */
	.dock-box.compact .dock-head {
		justify-content: flex-end;
		margin-bottom: 0.25rem;
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
		/* BUG-312: 글자 크기에 딸린 여백이 남지 않게 — 안에 든 것은 그림뿐이다. */
		padding: 0;
		width: 1.5rem;
		height: 1.5rem;
		border: var(--bw) solid var(--border);
		border-radius: var(--r-pill);
		background: var(--bg);
		color: var(--text-muted);
		line-height: 1;
		cursor: pointer;
	}
	/* `display: block` 이 아니면 svg 가 글자처럼 놓여 기준선만큼 아래로 내려간다 —
	   동그라미 안에서 살짝 치우쳐 보이던 것이 이것이다(admin). */
	.dock-btn svg {
		display: block;
		width: 0.875rem;
		height: 0.875rem;
	}
	/* admin: 보내기는 원본 [댓글 추가] 와 **같은 초록**이어야 한다 — 같은 일을 하는 버튼이다. */
	.dock-btn.send {
		background: var(--btn-primary-bg);
		border-color: var(--btn-primary-border);
		color: var(--btn-primary-text);
	}
	.dock-btn.send:hover:not(:disabled) {
		background: var(--btn-primary-bg-hover);
		color: var(--btn-primary-text);
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
