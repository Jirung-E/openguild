<!--
  DEV-259: 앱 알림 통합 호스트 (layout 에 1회 마운트).
  `notifications` 스토어(도착순)를 우하단 한 컬럼으로 렌더 — 새 알림이 맨
  아래(코너)에, 기존은 위로 밀림. 종류(kind)별로 렌더:
   - toast : 텍스트(자동 소멸). showToast() 로 추가.
   - update: 업데이터 상태 카드.
   - schema: DB schema-ahead 경고 카드.
  DEV-478: 토스트는 알림함과 같은 카드(`NoticeCard`)로 그린다. 종을 누르면(`inboxOpen`) 이 스택이 그대로 알림함이
  된다 — 지금 떠 있는 카드는 제자리에 두고, 그 사이 사라졌던 알림이 시각 순으로 끼어 들어온다. 닫으면 그것들만 빠진다.
  업데이트·스키마 알림의 소스 watcher(주기 체크 / schema 조회 → 스토어
  upsert)도 이 호스트가 내장한다(이전의 UpdateBanner/SchemaAheadBanner
  껍데기 컴포넌트를 흡수·제거).
-->
<script lang="ts">
	// DEV-454: 글자 아이콘 대신 SVG.
	import Icon from './Icon.svelte';
	import { onMount } from 'svelte';
	import { get } from 'svelte/store';
	import { goto } from '$app/navigation';
	import {
		notifications,
		upsertNotif,
		dismissNotif,
		dismissSchema,
		schemaSig,
		isSchemaDismissed,
		computeVisible,
		MAX_VISIBLE_NOTIFS,
		showToast
	} from '$lib/stores/toast';
	import {
		updateState,
		checkForUpdate,
		downloadAndRelaunch,
		dismissUpdate
	} from '$lib/api/updater';
	// DEV-305: 자동 확인 on/off.
	import { isAutoUpdateCheckEnabled } from '$lib/stores/updateSettings';
	import { detectEnvironment } from '$lib/api/transport';
	import OverlayScrollbar from './OverlayScrollbar.svelte';
	import NoticeCard from './NoticeCard.svelte';
	import { locale, t } from '$lib/stores/locale';
	import { tick } from 'svelte';
	import { cubicOut } from 'svelte/easing';
	import {
		clearAllNotices,
		closeInbox,
		dismissItem,
		inboxItems,
		inboxOpen
	} from '$lib/stores/inbox';
	import { stackCards, type NoticeCardData } from '$lib/utils/noticeCard';
	import type { ToastNotif } from '$lib/stores/toast';

	const RELEASE_URL = 'https://github.com/Jirung-E/openguild/releases/latest';
	const isTauri = detectEnvironment() === 'tauri';

	let notesEl: HTMLPreElement | undefined = $state(undefined);

	// 스키마 경고의 "최신 버전 받기" — 시스템 브라우저로. (button 이라 layout 의
	// anchor intercept 를 안 타서 명시 호출.)
	async function openRelease() {
		try {
			const { openUrl } = await import('@tauri-apps/plugin-opener');
			await openUrl(RELEASE_URL);
		} catch {
			try {
				window.open(RELEASE_URL, '_blank');
			} catch {
				/* ignore */
			}
		}
	}

	// ── DEV-259: 알림 소스 watcher 를 이 호스트로 흡수 ──────────────────
	// 이전엔 UpdateBanner/SchemaAheadBanner 가 아무것도 렌더 안 하면서 로직만
	// 돌리는 껍데기 컴포넌트였음 → 여기로 합쳐 두 컴포넌트를 제거.

	// DEV-083: 주기적 업데이트 재확인 간격(6시간 타협점).
	const PERIODIC_CHECK_MS = 6 * 60 * 60 * 1000;
	// BUG-096: uptodate/error 는 정보성이라 일정 시간 후 자동으로 idle 로.
	const AUTO_DISMISS_MS = 5000;

	// BUG-170: 디버그 훅(`window.__ogNotify`) 노출 조건 = **디버그 빌드**.
	//
	// 이전엔 `import.meta.env.DEV` 로만 게이트했는데 이 값은 프런트 **번들
	// 모드**(vite dev server)라 Rust 빌드 프로파일과 무관하다 — 디버그로
	// 패키징한 앱도 프런트는 production 번들이라 훅이 사라졌다(사용자 보고:
	// "난 디버그 기준으로 말한 건데"). 빌드 프로파일은 Rust 만 아니까
	// `is_debug_build` 로 물어본다. 릴리스 빌드면 false → 훅 없음.
	let debugHooks = $state(import.meta.env.DEV);

	/** DEV-266 의 콘솔 트리거 헬퍼 — 디버그 빌드에서만 window 에 붙인다. */
	function exposeDebugHooks() {
		if (typeof window === 'undefined') return;
		(window as unknown as Record<string, unknown>).__ogNotify = {
			toast: showToast,
			schema: () =>
				upsertNotif({
					id: 'schema',
					kind: 'schema',
					binaryVersion: '0.0.0-dev',
					aheadVersions: [99],
					latestKnown: 1
				}),
			update: (status = 'available') =>
				updateState.set(
					status === 'available'
						? { status: 'available', version: '9.9.9-dev', notes: 'dev trigger' }
						: ({ status } as never)
				)
		};
	}

	// DEV-063/DEV-259: 업데이터 상태 → 'update' 알림 presence(제자리 갱신).
	// DEV-266: dev 모드는 브라우저에서도 허용 — __ogNotify.update() 트리거용.
	// BUG-170: 디버그 빌드에서도 동일하게 허용(트리거 검증).
	$effect(() => {
		if (!isTauri && !debugHooks) return;
		if ($updateState.status === 'idle') {
			dismissNotif('update');
		} else {
			upsertNotif({ id: 'update', kind: 'update' });
		}
	});
	// BUG-096: uptodate/error 자동 닫힘(→ idle → 위 effect 가 알림 제거).
	$effect(() => {
		const s = $updateState.status;
		if (s !== 'uptodate' && s !== 'error') return;
		const handle = setTimeout(() => dismissUpdate(), AUTO_DISMISS_MS);
		return () => clearTimeout(handle);
	});

	// BUG-041/BUG-139: DB schema 가 binary 보다 앞서면 'schema' 알림.
	async function checkSchema() {
		if (!isTauri) return;
		try {
			const { invoke } = await import('@tauri-apps/api/core');
			const status = (await invoke('get_db_schema_status')) as {
				is_ahead: boolean;
				ahead_versions: number[];
				binary_version: string;
				latest_known_migration: number | null;
			};
			if (!status.is_ahead) {
				dismissNotif('schema');
				return;
			}
			const ahead = status.ahead_versions ?? [];
			const binaryVersion = status.binary_version ?? '';
			if (isSchemaDismissed(schemaSig(binaryVersion, ahead))) return;
			upsertNotif({
				id: 'schema',
				kind: 'schema',
				binaryVersion,
				aheadVersions: ahead,
				latestKnown: status.latest_known_migration
			});
		} catch {
			// 명령 미존재(구 backend) — 조용히 무시.
		}
	}

	// ── DEV-266: 표시 상한 + "+N개 더" 축약 ─────────────────────────────
	// 동시 다발 알림이 스택을 무한정 키우지 않게 computeVisible 로 접는다.
	// 칩 클릭 시 임시 전체 펼침 — 개수가 상한 아래로 내려가면 자동 복귀.
	let expanded = $state(false);
	const view = $derived(computeVisible($notifications, expanded));
	$effect(() => {
		if ($notifications.length <= MAX_VISIBLE_NOTIFS) expanded = false;
	});

	// ── DEV-478: 알림 카드 · 알림함 ─────────────────────────────────────
	const shownToasts = $derived(
		($inboxOpen ? $notifications : view.visible).filter((n) => n.kind === 'toast') as ToastNotif[]
	);
	const persistent = $derived(view.visible.filter((n) => n.kind !== 'toast'));
	// 알림함 줄 → 그 줄이 떴던 토스트의 key. 카드가 같은 알림이면 같은 key 를 쓰게(`stackCards`).
	const seenKeys = new Map<string, string>();
	const cards = $derived(stackCards(shownToasts, $inboxItems, $inboxOpen, seenKeys));

	function openCard(card: NoticeCardData) {
		if (card.href) {
			if (card.toast) dismissNotif(card.toast.id);
			closeInbox();
			void goto(card.href);
			return;
		}
		// 갈 곳이 없는 토스트는 눌러서 닫는다(알림함에는 남는다). 알림함의 지나간 줄은 누를 일이 없다.
		if (card.toast && !$inboxOpen) dismissNotif(card.toast.id);
	}

	function dismissCard(card: NoticeCardData) {
		if (card.toast) dismissNotif(card.toast.id);
		if (card.item) void dismissItem(card.item);
	}

	async function clearAll() {
		for (const n of $notifications) if (n.kind === 'toast') dismissNotif(n.id);
		await clearAllNotices();
		closeInbox();
	}

	/** CSS 의 움직임 시간(`--dur-mid`) — '동작 줄이기' 면 0 이다. */
	function cssMs(name: string): number {
		if (typeof window === 'undefined') return 0;
		const v = getComputedStyle(document.documentElement).getPropertyValue(name).trim();
		const n = parseFloat(v);
		if (!Number.isFinite(n)) return 0;
		return v.endsWith('ms') ? n : n * 1000;
	}

	/** 올 때 · 갈 때 — 오른쪽에서 미끄러져 들어오며 높이가 열리고, 나갈 때는 그 반대. */
	//
	// 두 단계로 나눈다 — 앞 절반은 높이를 그대로 두고 옆으로 미끄러지기만(그림자가 온전하다), 뒤 절반은 다 빠진 뒤
	// 높이만 접는다. 한꺼번에 하면 접는 동안 `overflow: hidden` 이 그림자를 매 프레임 다르게 잘라 끊겨 보였다.
	// 들어올 때는 그 반대(자리가 먼저 열리고 그 자리로 미끄러져 들어온다).
	function cardMotion(node: HTMLElement) {
		const h = node.offsetHeight;
		return {
			duration: cssMs('--dur-mid') * 1.5,
			// 장면마다 같은 속성을 다 적는다 — 어느 장면에 빠진 속성은 브라우저가 이상하게 이어 붙인다(높이가 눌려 보였다).
			css: (t: number) => {
				if (t >= 0.5) {
					const s = cubicOut((t - 0.5) * 2);
					return `opacity: ${s}; transform: translateX(${(1 - s) * 110}%); max-height: ${h}px; overflow: visible;`;
				}
				const s = cubicOut(t * 2);
				return `opacity: 0; transform: translateX(110%); max-height: ${s * h}px; overflow: hidden;`;
			}
		};
	}

	// ── 알림함의 스크롤 칸 ──
	let scrollEl: HTMLDivElement | undefined = $state(undefined);
	let fadeTop = $state(false);
	let fadeBottom = $state(false);
	/** 열 때 맨 아래(가장 최근)에 붙어 따라간다 — 사용자가 스크롤하면 놓는다. */
	let following = true;

	function updateFade() {
		const el = scrollEl;
		if (!el || !$inboxOpen) {
			fadeTop = fadeBottom = false;
			return;
		}
		fadeTop = el.scrollTop > 1;
		fadeBottom = el.scrollTop + el.clientHeight < el.scrollHeight - 1;
	}

	// 카드가 바뀌면 흐림 표시를 다시 잰다.
	$effect(() => {
		void cards;
		void tick().then(updateFade);
	});

	// 알림함 — 열면 맨 아래(가장 최근)가 보이게, 밖을 누르거나 Esc 면 닫는다.
	$effect(() => {
		if (!$inboxOpen) {
			updateFade();
			return;
		}
		// 끼어 드는 카드들이 자라는 동안 매 프레임 맨 아래에 붙인다 — 한두 번 뛰어내리면 흔들려 보였다.
		following = true;
		const until = performance.now() + cssMs('--dur-mid') * 1.5 + 80;
		let frame = 0;
		const stick = () => {
			if (scrollEl && following) scrollEl.scrollTop = scrollEl.scrollHeight;
			updateFade();
			if (performance.now() < until) frame = requestAnimationFrame(stick);
		};
		frame = requestAnimationFrame(stick);
		// BUG-322 와 같은 이유 — 타이틀바 클릭은 오버레이가 못 받으니 capture 로 받는다.
		const onDown = (e: MouseEvent) => {
			const el = e.target;
			if (el instanceof Element && (el.closest('.notif-stack') || el.closest('.inbox-wrap')))
				return;
			closeInbox();
		};
		const onKey = (e: KeyboardEvent) => {
			if (e.key === 'Escape') closeInbox();
		};
		window.addEventListener('mousedown', onDown, { capture: true });
		window.addEventListener('keydown', onKey);
		return () => {
			cancelAnimationFrame(frame);
			window.removeEventListener('mousedown', onDown, { capture: true });
			window.removeEventListener('keydown', onKey);
		};
	});

	onMount(() => {
		checkSchema();
		// DEV-266: dev 전용 알림 트리거 — schema-ahead 는 '구버전 binary + 신버전
		// DB', 업데이트 카드는 실제 릴리즈가 있어야만 떠서 동시 발생 조합을
		// 실기로 재현하기 어렵다. 개발 모드에서만 콘솔 헬퍼를 노출해 임의
		// 조합을 수동 재현할 수 있게 한다. 예:
		//   __ogNotify.toast('hello', 'error'); __ogNotify.schema(); __ogNotify.update('available');
		// BUG-170: dev 서버뿐 아니라 **디버그 빌드로 패키징한 앱**에서도 노출.
		// (import.meta.env.DEV 는 번들 모드라 패키징하면 항상 false — 그래서
		// 디버그 빌드에서도 훅이 없었다. Rust 에 빌드 프로파일을 물어본다.)
		void (async () => {
			if (!debugHooks) {
				try {
					const { invoke } = await import('@tauri-apps/api/core');
					debugHooks = await invoke<boolean>('is_debug_build');
				} catch {
					/* 브라우저 모드 / 구 backend — 훅 없음 */
				}
			}
			if (debugHooks) exposeDebugHooks();
		})();
		if (!isTauri) return; // 브라우저 모드 — 업데이트 개념 없음.
		// DEV-305: 자동 확인이 꺼져 있으면 시동/주기 확인을 건너뛴다.
		// 매 호출 시점에 다시 읽는다 — 설정을 끄면 앱 재시작 없이 즉시 멈춘다.
		// (설정 화면의 '지금 확인'은 이 설정과 무관하게 계속 동작한다.)
		if (!isAutoUpdateCheckEnabled()) return;
		checkForUpdate({ silent: true });
		const handle = setInterval(() => {
			if (!isAutoUpdateCheckEnabled()) return;
			const s = get(updateState).status;
			if (s === 'available' || s === 'downloading' || s === 'ready') return;
			checkForUpdate({ silent: true });
		}, PERIODIC_CHECK_MS);
		return () => clearInterval(handle);
	});
</script>

<div class="notif-stack" class:open={$inboxOpen} role="status" aria-live="polite">
	<!-- DEV-478: 알림함 머리 — 열 때 펼쳐지고, 카드와 달리 스크롤되지 않는다. -->
	{#if $inboxOpen}
		<div class="ib-head" transition:cardMotion>
			<span class="ib-title">{t('inbox.title', $locale)}</span>
			<button class="btn btn-sm" onclick={() => void clearAll()} disabled={cards.length === 0}
				>{t('inbox.clearAll', $locale)}</button
			>
		</div>
	{/if}
	<!-- 카드들. 알림함이면 이 안에서 스크롤하고, 가려진 쪽 끝을 흐리게 한다. -->
	<div
		class="ib-scroll"
		class:fade-top={fadeTop}
		class:fade-bottom={fadeBottom}
		bind:this={scrollEl}
		onscroll={updateFade}
		onwheel={() => (following = false)}
	>
		{#if $inboxOpen && cards.length === 0}
			<div class="slot" transition:cardMotion>
				<p class="card ib-empty">{t('inbox.empty', $locale)}</p>
			</div>
		{/if}
		<!-- DEV-266: 상한 초과분 축약 칩 — 클릭 시 전체 펼침. 스택 맨 위(코너
		     반대쪽)에 둬 최신/지속 알림을 가리지 않는다. -->
		{#if view.hidden > 0 && !$inboxOpen}
			<button class="card more-chip" onclick={() => (expanded = true)}>
				{t('notif.more', $locale).replace('{n}', String(view.hidden))}
			</button>
		{/if}
		{#each cards as card (card.key)}
			<div class="slot" transition:cardMotion>
				<NoticeCard {card} onopen={openCard} ondismiss={dismissCard} />
			</div>
		{/each}
	</div>
	{#if $inboxOpen && scrollEl}
		<OverlayScrollbar target={scrollEl} />
	{/if}
	{#each persistent as n (n.id)}
		{#if n.kind === 'update'}
			<div class="card upd" class:err={$updateState.status === 'error'} role="status">
				<button class="card-x" title={t('update.close', $locale)} onclick={() => dismissUpdate()}
					><Icon name="close" size="1em" /></button
				>
				{#if $updateState.status === 'checking'}
					<p class="t-title">{t('update.checking', $locale)}</p>
				{:else if $updateState.status === 'uptodate'}
					<p class="t-title ok">{t('update.uptodate', $locale)}</p>
				{:else if $updateState.status === 'available'}
					<p class="t-title">
						{t('update.availablePre', $locale)}<strong>{$updateState.version}</strong>{t(
							'update.availablePost',
							$locale
						)}
					</p>
					{#if $updateState.notes}
						<details>
							<summary>{t('update.releaseNotes', $locale)}</summary>
							<pre bind:this={notesEl}>{$updateState.notes}</pre>
							{#if notesEl}
								<OverlayScrollbar target={notesEl} />
							{/if}
						</details>
					{/if}
					<button class="btn btn-primary" onclick={() => downloadAndRelaunch()}>
						{t('update.installBtn', $locale)}
					</button>
				{:else if $updateState.status === 'downloading'}
					<p class="t-title">
						{t('update.downloading', $locale)}
						{$updateState.pct !== null ? `${$updateState.pct}%` : ''}
					</p>
				{:else if $updateState.status === 'ready'}
					<p class="t-title ok">{t('update.ready', $locale)}</p>
				{:else if $updateState.status === 'error'}
					<p class="t-title err">{t('update.error', $locale)}</p>
					<p class="t-msg">{$updateState.message}</p>
				{/if}
			</div>
		{:else if n.kind === 'schema'}
			<div class="card schema" role="alert">
				<button
					class="card-x"
					onclick={() => dismissSchema(schemaSig(n.binaryVersion, n.aheadVersions))}
					aria-label={t('update.close', $locale)}
					title={t('update.close', $locale)}><Icon name="close" size="1em" /></button
				>
				<div class="msg">
					<strong><Icon name="warn" size="1em" /> 이 길드 DB 가 현재 GUI 버전보다 새롭습니다</strong
					>
					<span class="detail">
						현재 GUI: <code>v{n.binaryVersion}</code>
						{#if n.latestKnown != null}
							(migration {n.latestKnown} 까지 인식)
						{/if}
						· 이 길드 DB: migration <strong>{Math.max(...n.aheadVersions)}</strong>
						{#if n.aheadVersions.length > 1}
							(+{n.aheadVersions.length - 1} more)
						{/if}
						적용됨. 일부 데이터가 제대로 표시되지 않거나 reindex 가 실패할 수 있습니다.
					</span>
				</div>
				<button class="btn btn-warning btn-update" onclick={openRelease}>
					최신 버전 받기 <Icon name="external" size="1em" />
				</button>
			</div>
		{/if}
	{/each}
</div>

<style>
	/* DEV-259: 우하단 단일 스택. bottom 고정 + column → 새 항목이 코너에,
	   기존은 위로 밀림. */
	.notif-stack {
		/* DEV-478: 카드 모서리에 걸친 × · 점과 그림자 번짐(--shadow-md)이 잘리지 않게 안쪽 여백을 두고, 그만큼
		   바깥으로 낸다. 여백은 비어 있어 눌림을 받지 않는다. */
		--edge-x: 1.5rem; /* token-ok: 그림자 번짐이 들어갈 자리 — 화면 오른쪽 여백과 같아 스택이 오른쪽 끝에 붙는다 */
		--edge-y: 1.75rem; /* token-ok: 그림자 번짐(아래로 더 퍼진다)이 들어갈 자리 */
		position: fixed;
		right: calc(1.5rem - var(--edge-x));
		bottom: calc(1.5rem - var(--edge-y));
		z-index: 2000;
		display: flex;
		flex-direction: column;
		align-items: stretch;
		box-sizing: content-box;
		padding: 0 var(--edge-x) var(--edge-y);
		width: calc(26.25rem * var(--popup-scale, 1)); /* BUG-064 */
		max-width: calc(100vw - 3rem);
		pointer-events: none;
	}
	/* 알림함 — 배경은 없고(카드마다의 그림자로 가른다). 위 끝은 타이틀바 · 상단 바 아래, 머리는 고정하고
	   카드 칸만 스크롤한다. */
	.notif-stack.open {
		top: calc(var(--titlebar-h, 0px) + var(--nav-h, 3.25rem));
		justify-content: flex-end;
	}
	.ib-scroll {
		display: flex;
		flex-direction: column;
		pointer-events: none;
	}
	.notif-stack.open .ib-scroll {
		flex: 0 1 auto;
		min-height: 0;
		/* 그림자 · 모서리 표시가 스크롤 칸에 잘리지 않게 여백을 안에 두고 그만큼 바깥으로 */
		margin: 0 calc(-1 * var(--edge-x)) calc(-1 * var(--edge-y));
		padding: 0 var(--edge-x) var(--edge-y);
		overflow-y: auto;
		overscroll-behavior: contain;
		scrollbar-width: none;
		pointer-events: auto;
	}
	.notif-stack.open .ib-scroll::-webkit-scrollbar {
		display: none;
	}
	/* 스크롤로 가려진 카드가 있는 쪽 끝을 흐리게. 마스크는 알파만 보므로 불투명한 아무 색(currentColor)이면 된다. */
	.ib-scroll.fade-top {
		mask-image: linear-gradient(to bottom, transparent, currentColor var(--sp-32));
	}
	.ib-scroll.fade-bottom {
		mask-image: linear-gradient(to top, transparent, currentColor var(--sp-32));
	}
	.ib-scroll.fade-top.fade-bottom {
		mask-image: linear-gradient(
			to bottom,
			transparent,
			currentColor var(--sp-32),
			currentColor calc(100% - var(--sp-32)),
			transparent
		);
	}
	/* 간격은 위 여백으로 — 카드 자리(.slot)와 업데이트 · 스키마 카드가 같은 간격으로 쌓인다. */
	.notif-stack > .card,
	.ib-scroll > .card {
		margin-top: var(--sp-12);
	}
	.card {
		position: relative;
		text-align: left;
		border-radius: var(--r-lg);
		border: var(--bw) solid var(--border);
		background: var(--bg-elevated);
		box-shadow: var(--shadow-md);
		color: var(--text);
		font-size: var(--fs-md);
		pointer-events: auto;
		animation: notif-in var(--dur-mid) ease-out;
	}
	@keyframes notif-in {
		from {
			opacity: 0;
			transform: translateY(8px);
		}
		to {
			opacity: 1;
			transform: translateY(0);
		}
	}
	.card-x {
		position: absolute;
		top: 0.4rem;
		right: 0.5rem;
		background: none;
		border: none;
		color: var(--text-faint);
		font-size: 1.1rem; /* token-ok: × 글리프 크기 */
		line-height: var(--lh-tight);
		cursor: pointer;
		padding: 0 var(--sp-4);
	}
	.card-x:hover {
		color: var(--text);
	}

	/* ── DEV-478: 알림 카드 자리 · 알림함 ── */
	/* 카드 사이 간격은 자리의 위 여백으로 — 들고 날 때 높이와 함께 접혀 튀지 않는다. */
	.slot {
		padding-top: var(--sp-12);
		pointer-events: none;
	}
	.slot > :global(*) {
		pointer-events: auto;
	}
	.ib-head {
		display: flex;
		flex: none;
		align-items: center;
		justify-content: space-between;
		gap: var(--sp-8);
		padding: var(--sp-12) var(--sp-4) 0;
		pointer-events: auto;
	}
	.ib-title {
		font-size: var(--fs-sm);
		font-weight: var(--fw-semibold);
		color: var(--text);
		text-shadow: 0 0 var(--sp-4) var(--bg);
	}
	.ib-empty {
		margin: 0;
		padding: var(--sp-12) var(--sp-16);
		font-size: var(--fs-sm);
		color: var(--text-muted);
		text-align: center;
	}
	/* DEV-266: "+N개 더" 축약 칩. */
	.more-chip {
		align-self: flex-end;
		padding: var(--sp-4) var(--sp-12);
		font-size: var(--fs-xs);
		color: var(--text-muted);
		cursor: pointer;
	}
	.more-chip:hover {
		color: var(--text);
	}

	/* ── update ── */
	.upd {
		padding: var(--sp-12) var(--sp-32) var(--sp-12) var(--sp-16);
		border-left: 3px solid var(--success-strong);
	}
	.upd.err {
		border-left-color: var(--danger);
	}
	.upd .t-title {
		margin: 0;
		font-weight: var(--fw-semibold);
	}
	.upd .t-title.ok {
		color: var(--success);
	}
	.upd .t-title.err {
		color: var(--danger);
	}
	.upd .t-msg {
		margin: var(--sp-6) 0 0;
		color: var(--text-muted);
		font-size: var(--fs-sm);
		word-break: break-word;
	}
	.upd details {
		margin: var(--sp-8) 0;
		color: var(--text-muted);
	}
	.upd pre {
		white-space: pre-wrap;
		background: var(--bg);
		border: var(--bw) solid var(--bg-subtle);
		border-radius: var(--r-md);
		padding: var(--sp-8) var(--sp-12);
		max-height: 8rem;
		overflow-y: auto;
		margin: var(--sp-6) 0 0;
		scrollbar-width: none;
	}
	.upd pre::-webkit-scrollbar {
		display: none;
	}
	.upd .btn-primary {
		margin-top: var(--sp-8);
	}

	/* ── schema ── */
	.schema {
		display: flex;
		flex-direction: column;
		gap: var(--sp-8);
		padding: var(--sp-12) var(--sp-32) var(--sp-12) var(--sp-16);
		border-left: 3px solid var(--orange);
	}
	.schema .msg {
		display: flex;
		flex-direction: column;
		gap: var(--sp-4);
	}
	.schema .msg strong {
		font-weight: var(--fw-semibold);
		color: var(--orange);
	}
	.schema .detail {
		font-size: var(--fs-sm);
		color: var(--text-muted);
	}
	.schema .detail code {
		background: color-mix(in srgb, var(--text) 10%, transparent);
		padding: 0 var(--sp-4);
		border-radius: var(--r-xs);
		font-size: 0.85em;
		color: var(--text);
	}
	.schema .btn-update {
		align-self: flex-start;
	}
</style>
