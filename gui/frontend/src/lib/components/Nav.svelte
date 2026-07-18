<script lang="ts">
	import { page } from '$app/stores';
	import { adminApi } from '$lib/api/admin';
	import { bumpReindex } from '$lib/stores/reindex';
	// DEV-205 모듈1: Nav 문자열 i18n.
	import { locale, t } from '$lib/stores/locale';
	// DEV-138: 설정 퀵메뉴 — ⚙ 클릭 시 dropdown (테마/UI크기/폭 + 전체 설정).
	// DEV-125 의 standalone 테마 순환 버튼은 퀵메뉴로 흡수.
	import SettingsQuickMenu from './SettingsQuickMenu.svelte';
	let quickMenuOpen = $state(false);

	// BUG-146: 예전엔 커스텀 타이틀바가 없는 환경(브라우저 dev / 당시엔
	// macOS/Linux 로 오판)에서 Nav 에 "openguild" 로고 + 길드명을 fallback
	// 으로 그렸으나, 이제 로고/길드명은 타이틀바(TitleBar)로 일원화. 앱(모든
	// OS)이든 웹이든 Nav 에는 아예 그리지 않으므로 관련 상태/조회/판별 전부
	// 제거했다.

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
	<!-- BUG-146: 로고/길드명은 타이틀바로 일원화 — Nav 에는 그리지 않음. -->
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
