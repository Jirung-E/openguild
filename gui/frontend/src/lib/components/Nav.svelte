<script lang="ts">
	import { page } from '$app/stores';
	import { adminApi } from '$lib/api/admin';
	import { metaApi } from '$lib/api/meta';
	import { bumpReindex } from '$lib/stores/reindex';
	// DEV-205 모듈1: Nav 문자열 i18n.
	import { locale, t } from '$lib/stores/locale';
	// DEV-138: 설정 퀵메뉴 — ⚙ 클릭 시 dropdown (테마/UI크기/폭 + 전체 설정).
	// DEV-125 의 standalone 테마 순환 버튼은 퀵메뉴로 흡수.
	import SettingsQuickMenu from './SettingsQuickMenu.svelte';
	let quickMenuOpen = $state(false);

	// DEV-141 / DEV-113 후속(사용자 보고): 현재 길드 이름 — 어느 길드인지
	// 한눈에. 이전엔 Tauri invoke 만 써서 (a) 브라우저/server 모드는 항상
	// 미표시, (b) Tauri + 원격 연결 시 Rust 로컬 placeholder 이름
	// ("openguild-welcome-placeholder")이 잘못 보였다. `metaApi.getGuildDisplayInfo()`
	// 가 모드별로 올바른 source(Tauri-local invoke vs HTTP)를 골라 항상 실제
	// 길드 이름을 가져오고, 원격 연결이면 `isRemote` 로 배지도 표시.
	// BUG-136 후속(admin #2): 다른 길드를 열었다가 Welcome 으로 돌아오면 Nav 에
	// 이전 길드 이름이 남았음 — onMount 1회 조회는 라우팅 변화를 못 따라감.
	// DEV-207 의 guildContextActive(보드/Welcome 마운트가 갱신, 이번에 반응형
	// 스토어화)를 구독해 활성 전환마다 재조회 / 비활성이면 숨김.
	import { guildContextActive } from '$lib/stores/guildSession';
	import { detectEnvironment } from '$lib/api/transport';
	let guildName = $state('');
	let isRemoteGuild = $state(false);

	// DEV-253: 커스텀 타이틀바(Windows Tauri)가 있으면 로고/길드 이름을 타이틀바로
	// 옮겼으므로 Nav 에선 숨긴다. 타이틀바가 없는 환경(브라우저 dev / macOS)에선
	// Welcome 진입점과 길드 이름이 사라지지 않도록 기존 로고를 그대로 유지.
	const hasTitleBar =
		detectEnvironment() === 'tauri' &&
		typeof navigator !== 'undefined' &&
		navigator.userAgent.includes('Windows');
	$effect(() => {
		if (!$guildContextActive) {
			guildName = '';
			isRemoteGuild = false;
			return;
		}
		metaApi
			.getGuildDisplayInfo()
			.then((info) => {
				guildName = info.name;
				isRemoteGuild = info.remote;
			})
			.catch(() => {
				/* 길드 모드 아님 / 조회 실패 — 표시 안 함 */
				guildName = '';
			});
	});

	// DEV-011: Home 탭. URL `/` 가 ?view 없으면 home 기본.
	type View = 'home' | 'board' | 'list';

	let currentView: View = $derived(($page.url.searchParams.get('view') as View | null) ?? 'home');

	let onAdminPath = $derived($page.url.pathname.startsWith('/admin'));
	let onRootPath = $derived($page.url.pathname === '/');
	let onSettingsPath = $derived($page.url.pathname.startsWith('/settings'));
	// DEV-016: 길드 규칙 페이지.
	let onRulesPath = $derived($page.url.pathname.startsWith('/rules'));
	// DEV-217: 도서관 페이지.
	let onLibraryPath = $derived($page.url.pathname.startsWith('/library'));

	// DEV-095: Reindex 버튼 — 사용자 의견 "Admin 페이지 아닌 일반 사용자도
	// 접근 가능". 외부 편집 / `openguild quest new` CLI / git pull 등으로
	// `.guild/quests/*.md` 가 바뀌었을 때 cache 정합 회복.
	type ReindexState =
		| { status: 'idle' }
		| { status: 'running' }
		| { status: 'done'; ts: number }
		| { status: 'error'; message: string };
	let reindexState = $state<ReindexState>({ status: 'idle' });

	async function runReindex() {
		reindexState = { status: 'running' };
		try {
			await adminApi.reindex();
			// DEV-095 fix: invalidateAll() 은 +page.ts 의 load() 만 트리거 — 우리
			// 페이지들은 onMount 직접 fetch 라 안 먹음. store-bump 패턴으로 페이지
			// 가 reactive subscribe.
			bumpReindex();
			reindexState = { status: 'done', ts: Date.now() };
			// 3 초 후 idle 로 — 사용자 noise 최소화.
			setTimeout(() => {
				if (reindexState.status === 'done') {
					reindexState = { status: 'idle' };
				}
			}, 3000);
		} catch (e) {
			reindexState = {
				status: 'error',
				message: e instanceof Error ? e.message : t('nav.reindex.error', $locale)
			};
		}
	}
</script>

<header>
	<!-- DEV-052 후속 (4회차): 로고 클릭 → Welcome (다른 길드로 전환 / recent 관리).
	     DEV-253: 커스텀 타이틀바가 있는 환경에선 로고/길드 이름을 타이틀바로 옮겨
	     여기선 숨김. 타이틀바 없는 환경에서만 fallback 으로 노출. -->
	{#if !hasTitleBar}
		<a href="/welcome" class="logo">
			openguild
			<!-- DEV-141: 현재 길드 이름 — 로고 옆 작은 배지로 어느 길드인지 표시. -->
			{#if guildName}
				<span class="guild-name" title="{t('nav.currentGuild', $locale)}: {guildName}">{guildName}</span>
				<!-- DEV-113 후속: 원격 서버에 연결된 상태면 명시 배지. -->
				{#if isRemoteGuild}
					<span class="remote-badge" title={t('nav.remoteConnected', $locale)}>🌐 {t('nav.remote', $locale)}</span>
				{/if}
			{/if}
		</a>
	{/if}

	<nav>
		<a href="/" class:active={onRootPath && currentView === 'home'}>{t('nav.home', $locale)}</a>
		<a href="/?view=board" class:active={onRootPath && currentView === 'board'}>{t('nav.board', $locale)}</a>
		<a href="/?view=list" class:active={onRootPath && currentView === 'list'}>{t('nav.list', $locale)}</a>
		<a href="/admin" class:active={onAdminPath}>{t('nav.admin', $locale)}</a>
		<!-- DEV-016: 길드 규칙 — 팀 컨벤션 / 그라운드 룰. -->
		<a href="/rules" class:active={onRulesPath}>{t('nav.rules', $locale)}</a>
		<!-- DEV-217: 도서관 — 프로젝트 참고문서/노트 (BOOK 번호). -->
		<a href="/library" class:active={onLibraryPath}>{t('nav.library', $locale)}</a>
	</nav>

	<div class="nav-right">
		<!-- DEV-095: 외부 편집 후 cache 정합 회복 — 일반 사용자도 한 클릭으로. -->
		<button
			class="btn-reindex"
			class:running={reindexState.status === 'running'}
			class:done={reindexState.status === 'done'}
			class:error={reindexState.status === 'error'}
			onclick={runReindex}
			disabled={reindexState.status === 'running'}
			title={reindexState.status === 'error'
				? `${t('nav.reindex.failed', $locale)}: ${reindexState.message}`
				: reindexState.status === 'done'
					? t('nav.reindex.done', $locale)
					: t('nav.reindex.hint', $locale)}
			aria-label="Reindex"
		>
			{#if reindexState.status === 'running'}
				⟳
			{:else if reindexState.status === 'done'}
				✓
			{:else if reindexState.status === 'error'}
				⚠
			{:else}
				⟲
			{/if}
		</button>
		<!-- DEV-084 → DEV-138: ⚙ 가 바로 페이지 이동 대신 퀵메뉴 dropdown.
		     자주 쓰는 표시 설정은 메뉴에서 즉시, 전체 설정은 링크로. -->
		<div class="settings-wrap">
			<button
				class="btn-settings"
				class:active={onSettingsPath || quickMenuOpen}
				onclick={() => (quickMenuOpen = !quickMenuOpen)}
				title={t('nav.settings', $locale)}
				aria-label={t('nav.settings', $locale)}
				aria-expanded={quickMenuOpen}>⚙</button
			>
			{#if quickMenuOpen}
				<SettingsQuickMenu onclose={() => (quickMenuOpen = false)} />
			{/if}
		</div>
	</div>
</header>

<style>
	/* DEV-074: hardcoded color → var() 마이그레이션. */
	/* DEV-101 fix5: height 52px → 3.25rem — UI scale 반영. 안 그러면 버튼 (rem) 만
	   확대돼 nav 가 자식보다 작아짐. */
	header {
		display: flex;
		align-items: center;
		gap: 2rem;
		padding: 0 1.5rem;
		height: var(--nav-h, 3.25rem);
		background: var(--nav-bg);
		border-bottom: 1px solid var(--nav-border);
		position: sticky;
		/* 커스텀 타이틀바(Windows Tauri) 아래에 붙도록 — 없으면 0px. */
		top: var(--titlebar-h, 0px);
		z-index: 100;
	}

	.logo {
		display: inline-flex;
		align-items: baseline;
		gap: 0.5rem;
		font-size: 1.1rem;
		font-weight: 700;
		color: var(--text);
		text-decoration: none;
		letter-spacing: 0.02em;
	}

	/* DEV-141: 현재 길드 이름 배지 — 로고보다 작고 muted, accent 보더로 구분. */
	.guild-name {
		font-size: 0.75rem;
		font-weight: 600;
		letter-spacing: 0;
		color: var(--accent);
		background: color-mix(in srgb, var(--accent) 12%, transparent);
		border: 1px solid color-mix(in srgb, var(--accent) 35%, transparent);
		border-radius: 5px;
		padding: 0.1rem 0.4rem;
		max-width: 12rem;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	/* DEV-113 후속: 원격 연결 배지 — guild-name 과 톤 구분(warning 계열,
		 인증 없는 네트워크 노출 상태라는 의미). */
	.remote-badge {
		font-size: 0.7rem;
		font-weight: 600;
		letter-spacing: 0;
		color: var(--warning);
		background: color-mix(in srgb, var(--warning) 14%, transparent);
		border: 1px solid color-mix(in srgb, var(--warning) 40%, transparent);
		border-radius: 5px;
		padding: 0.1rem 0.4rem;
		white-space: nowrap;
	}

	nav {
		display: flex;
		gap: 0.25rem;
		flex: 1;
	}

	nav a {
		padding: 0.35rem 0.85rem;
		border-radius: 6px;
		font-size: 0.875rem;
		color: var(--text-muted);
		text-decoration: none;
		transition:
			background 0.15s,
			color 0.15s;
	}

	nav a:hover {
		background: var(--nav-hover-bg);
		color: var(--text);
	}

	nav a.active {
		background: var(--nav-hover-bg);
		color: var(--text);
	}

	.nav-right {
		display: flex;
		align-items: center;
		gap: 0.5rem;
		margin-left: auto;
	}

	/* DEV-138: 퀵메뉴 anchor — dropdown 의 position:absolute 기준점. */
	.settings-wrap {
		position: relative;
	}

	/* DEV-084: 설정 진입 — 톱니바퀴 아이콘. DEV-138 부터 button (퀵메뉴 토글). */
	.btn-settings {
		display: inline-flex;
		align-items: center;
		justify-content: center;
		width: 2rem;
		height: 2rem;
		border-radius: 6px;
		font-size: 1.1rem;
		line-height: 1;
		color: var(--text-muted);
		background: transparent;
		border: none;
		cursor: pointer;
		text-decoration: none;
		transition:
			background 0.15s,
			color 0.15s,
			transform 0.2s;
	}
	.btn-settings:hover {
		background: var(--nav-hover-bg);
		color: var(--text);
		transform: rotate(45deg);
	}
	.btn-settings.active {
		background: var(--nav-hover-bg);
		color: var(--text);
	}

	/* DEV-095: Reindex 버튼 — 설정 옆. */
	.btn-reindex {
		display: inline-flex;
		align-items: center;
		justify-content: center;
		width: 2rem;
		height: 2rem;
		border-radius: 6px;
		font-size: 1.05rem;
		line-height: 1;
		color: var(--text-muted);
		background: transparent;
		border: none;
		cursor: pointer;
		transition:
			background 0.15s,
			color 0.15s,
			transform 0.4s;
	}
	.btn-reindex:hover:not(:disabled) {
		background: var(--nav-hover-bg);
		color: var(--text);
	}
	.btn-reindex:disabled {
		cursor: wait;
	}
	.btn-reindex.running {
		color: var(--accent);
		animation: spin 1.2s linear infinite;
	}
	.btn-reindex.done {
		color: var(--success);
	}
	.btn-reindex.error {
		color: var(--danger);
	}
	@keyframes spin {
		from {
			transform: rotate(0deg);
		}
		to {
			transform: rotate(360deg);
		}
	}
</style>
