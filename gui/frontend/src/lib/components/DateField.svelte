<!--
  DEV-205: 언어 반응 날짜 입력.

  배경: 크로미움/WebView2 의 네이티브 <input type="date"> 는 표기 형식도,
  showPicker() 로 띄우는 달력 팝업도 navigator.language(OS 로케일)만 따르고
  lang 속성/앱 언어를 무시한다 — 앱을 영어로 바꿔도 달력이 한글로 뜨는 문제
  (사용자 보고, DEV-205 3차). 네이티브 팝업의 언어는 웹에서 제어 불가이므로
  달력 팝업 자체를 커스텀으로 구현한다.

  - 값 표시는 앱 전역과 동일한 ISO(YYYY-MM-DD) 텍스트 직접 입력(언어 무관).
  - 📅 버튼 → 커스텀 달력 팝업(월 헤더/요일 이름을 Intl.DateTimeFormat 에
    "앱 locale 을 명시 인자로" 넘겨 렌더 — OS 로케일과 무관하게 언어 토글을
    따라간다).
  - mode="month" 면 YYYY-MM 값 + 12개월 그리드 팝업(작업기록 월 뷰 용).
-->
<script lang="ts">
	import { locale, t } from '$lib/stores/locale';

	let {
		value = $bindable(''),
		disabled = false,
		ariaLabel = '',
		mode = 'date',
		onpick
	}: {
		value?: string;
		disabled?: boolean;
		ariaLabel?: string;
		/** 'date' = YYYY-MM-DD(일 그리드), 'month' = YYYY-MM(월 그리드). */
		mode?: 'date' | 'month';
		/** 값 확정 시(달력 선택 또는 텍스트 입력 blur/Enter) 호출 — onchange 대용. */
		onpick?: (v: string) => void;
	} = $props();

	let rootEl = $state<HTMLElement | null>(null);
	let open = $state(false);
	// 팝업이 보여주는 연/월 (month 는 0-based).
	let viewYear = $state(new Date().getFullYear());
	let viewMonth = $state(new Date().getMonth());

	const intlLoc = $derived($locale === 'en' ? 'en-US' : 'ko-KR');

	// 요일 헤더 — Intl 에 앱 locale 을 명시해 OS 로케일과 무관하게 렌더.
	// 2023-01-01 이 일요일이라 그 주로 고정 생성(주 시작 = 일요일).
	const weekdays = $derived.by(() => {
		const fmt = new Intl.DateTimeFormat(intlLoc, { weekday: 'short' });
		return Array.from({ length: 7 }, (_, i) => fmt.format(new Date(2023, 0, 1 + i)));
	});
	const headerLabel = $derived.by(() => {
		if (mode === 'month') return String(viewYear);
		return new Intl.DateTimeFormat(intlLoc, { year: 'numeric', month: 'long' }).format(
			new Date(viewYear, viewMonth, 1)
		);
	});
	const monthNames = $derived.by(() => {
		const fmt = new Intl.DateTimeFormat(intlLoc, { month: 'short' });
		return Array.from({ length: 12 }, (_, i) => fmt.format(new Date(2023, i, 1)));
	});

	// 일 그리드 — 앞쪽 공백(null) + 1..말일.
	const dayGrid = $derived.by(() => {
		const first = new Date(viewYear, viewMonth, 1).getDay();
		const last = new Date(viewYear, viewMonth + 1, 0).getDate();
		const cells: (number | null)[] = Array(first).fill(null);
		for (let d = 1; d <= last; d++) cells.push(d);
		return cells;
	});

	const pad = (n: number) => String(n).padStart(2, '0');
	const todayIso = () => {
		const now = new Date();
		return `${now.getFullYear()}-${pad(now.getMonth() + 1)}-${pad(now.getDate())}`;
	};

	function togglePicker() {
		if (disabled) return;
		if (open) {
			open = false;
			return;
		}
		// 현재 값(또는 오늘) 기준으로 팝업 뷰 초기화.
		const m = /^(\d{4})-(\d{2})(?:-(\d{2}))?/.exec(value.trim());
		if (m) {
			viewYear = Number(m[1]);
			viewMonth = Number(m[2]) - 1;
		} else {
			const now = new Date();
			viewYear = now.getFullYear();
			viewMonth = now.getMonth();
		}
		open = true;
	}

	function stepMonth(delta: number) {
		const d = new Date(viewYear, viewMonth + delta, 1);
		viewYear = d.getFullYear();
		viewMonth = d.getMonth();
	}

	function commit(v: string) {
		value = v;
		open = false;
		onpick?.(v);
	}
	function pickDay(day: number) {
		commit(`${viewYear}-${pad(viewMonth + 1)}-${pad(day)}`);
	}
	function pickMonth(mIdx: number) {
		commit(`${viewYear}-${pad(mIdx + 1)}`);
	}
	function pickToday() {
		const iso = todayIso();
		commit(mode === 'month' ? iso.slice(0, 7) : iso);
	}

	// 바깥 클릭 시 닫기 — SearchPalette(DEV-255)에서 확립한 패턴 그대로:
	// mousedown(여는 클릭과 이벤트 종류 분리) + capture(타이틀바 drag-region 의
	// stopImmediatePropagation 영향 안 받음).
	function onWindowMouseDown(e: MouseEvent) {
		if (!open) return;
		if (rootEl && !rootEl.contains(e.target as Node)) open = false;
	}
	function onKeydown(e: KeyboardEvent) {
		if (e.key === 'Escape' && open) {
			e.stopPropagation();
			open = false;
		}
	}

	$effect(() => {
		if (!open) return;
		window.addEventListener('mousedown', onWindowMouseDown, { capture: true });
		return () => window.removeEventListener('mousedown', onWindowMouseDown, { capture: true });
	});
</script>

<span class="datefield" class:disabled bind:this={rootEl} onkeydown={onKeydown} role="presentation">
	<input
		class="df-text"
		class:month={mode === 'month'}
		type="text"
		inputmode="numeric"
		placeholder={mode === 'month' ? 'YYYY-MM' : 'YYYY-MM-DD'}
		pattern={mode === 'month' ? '\\d{4}-\\d{2}' : '\\d{4}-\\d{2}-\\d{2}'}
		bind:value
		{disabled}
		onchange={() => onpick?.(value)}
		aria-label={ariaLabel || t('common.pickDate', $locale)}
	/>
	<button
		type="button"
		class="df-btn"
		onclick={togglePicker}
		{disabled}
		aria-label={t('common.pickDate', $locale)}
		aria-expanded={open}
		title={t('common.pickDate', $locale)}
	>
		<!-- BUG-254: 속성 px 는 배율을 안 따라간다 — CSS 로 덮는다(.df-ico). -->
		<svg class="df-ico" width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.3" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
			<rect x="2" y="3" width="12" height="11" rx="1.5" />
			<path d="M2 6.5h12M5 2v2.5M11 2v2.5" />
		</svg>
	</button>

	{#if open}
		<div class="df-pop" role="dialog" aria-label={t('common.pickDate', $locale)}>
			<div class="df-nav">
				<button type="button" class="df-nav-btn" onclick={() => (mode === 'month' ? (viewYear -= 1) : stepMonth(-1))} aria-label="◀">◀</button>
				<span class="df-head">{headerLabel}</span>
				<button type="button" class="df-nav-btn" onclick={() => (mode === 'month' ? (viewYear += 1) : stepMonth(1))} aria-label="▶">▶</button>
			</div>
			{#if mode === 'date'}
				<div class="df-grid days">
					{#each weekdays as w, i (i)}
						<span class="df-wd" class:sun={i === 0} class:sat={i === 6}>{w}</span>
					{/each}
					{#each dayGrid as day, i (i)}
						{#if day === null}
							<span></span>
						{:else}
							{@const iso = `${viewYear}-${pad(viewMonth + 1)}-${pad(day)}`}
							<button
								type="button"
								class="df-cell"
								class:selected={value.trim() === iso}
								class:today={todayIso() === iso}
								onclick={() => pickDay(day)}
							>
								{day}
							</button>
						{/if}
					{/each}
				</div>
			{:else}
				<div class="df-grid months">
					{#each monthNames as name, i (i)}
						{@const iso = `${viewYear}-${pad(i + 1)}`}
						<button
							type="button"
							class="df-cell"
							class:selected={value.trim() === iso}
							class:today={todayIso().slice(0, 7) === iso}
							onclick={() => pickMonth(i)}
						>
							{name}
						</button>
					{/each}
				</div>
			{/if}
			<div class="df-foot">
				<button type="button" class="df-today" onclick={pickToday}>{t('common.today', $locale)}</button>
			</div>
		</div>
	{/if}
</span>

<style>
	/* BUG-254: SVG 의 width/height **속성**은 px 라 UI 배율을 안 따른다.
	   속성은 폴백으로 두고 CSS 로 덮는다. */
	.df-ico {
		width: 0.875rem;
		height: 0.875rem;
	}

	.datefield {
		display: inline-flex;
		align-items: center;
		gap: 2px;
		position: relative;
	}
	.df-text {
		width: 6.6rem;
		padding: 0.2rem 0.4rem;
		font-size: 0.8rem;
		font-family: 'JetBrains Mono', ui-monospace, monospace;
		background: var(--bg);
		border: var(--bw) solid var(--border);
		border-radius: var(--r-sm);
		color: var(--text);
	}
	.df-text.month {
		width: 5rem;
	}
	.df-text:focus {
		outline: none;
		border-color: var(--accent);
	}
	.df-btn {
		display: inline-flex;
		align-items: center;
		justify-content: center;
		width: 1.5rem;
		height: 1.5rem;
		padding: 0;
		color: var(--text-muted);
		background: var(--bg-subtle);
		border: var(--bw) solid var(--border);
		border-radius: var(--r-sm);
		cursor: pointer;
	}
	.df-btn:hover:not(:disabled) {
		background: var(--border);
		color: var(--text);
	}
	.datefield.disabled,
	.df-btn:disabled {
		opacity: 0.5;
		cursor: not-allowed;
	}
	/* ── 커스텀 달력 팝업 ── */
	.df-pop {
		position: absolute;
		top: calc(100% + 4px);
		left: 0;
		z-index: 1300;
		padding: 0.5rem;
		background: var(--bg-elevated);
		border: var(--bw) solid var(--border);
		border-radius: var(--r-lg);
		box-shadow: 0 8px 26px rgba(0, 0, 0, 0.45);
		user-select: none;
	}
	.df-nav {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 0.4rem;
		margin-bottom: 0.4rem;
	}
	.df-head {
		font-size: 0.8rem;
		font-weight: 600;
		color: var(--text-strong);
		white-space: nowrap;
	}
	.df-nav-btn {
		width: 1.5rem;
		height: 1.4rem;
		border: none;
		border-radius: var(--r-sm);
		background: transparent;
		color: var(--text-muted);
		font-size: 0.65rem;
		cursor: pointer;
	}
	.df-nav-btn:hover {
		background: var(--nav-hover-bg);
		color: var(--text);
	}
	.df-grid.days {
		display: grid;
		grid-template-columns: repeat(7, 1.7rem);
		gap: 1px;
	}
	.df-grid.months {
		display: grid;
		grid-template-columns: repeat(4, 3rem);
		gap: 2px;
	}
	.df-wd {
		font-size: 0.62rem;
		color: var(--text-faint);
		text-align: center;
		padding-bottom: 2px;
	}
	.df-wd.sun {
		color: var(--danger);
	}
	.df-wd.sat {
		color: var(--accent);
	}
	.df-cell {
		height: 1.6rem;
		border: none;
		border-radius: var(--r-sm);
		background: transparent;
		color: var(--text);
		font-size: 0.72rem;
		cursor: pointer;
	}
	.df-cell:hover {
		background: var(--nav-hover-bg);
	}
	.df-cell.today {
		box-shadow: inset 0 0 0 1px var(--accent);
	}
	.df-cell.selected {
		background: var(--btn-primary-bg);
		color: var(--btn-primary-text);
	}
	.df-foot {
		display: flex;
		justify-content: flex-end;
		margin-top: 0.35rem;
	}
	.df-today {
		border: none;
		border-radius: var(--r-sm);
		background: transparent;
		color: var(--accent);
		font-size: 0.7rem;
		padding: 0.15rem 0.4rem;
		cursor: pointer;
	}
	.df-today:hover {
		background: var(--nav-hover-bg);
	}
</style>
