<script lang="ts">
	import { onMount } from 'svelte';
	import { goto } from '$app/navigation';
	// DEV-205 모듈1: Welcome 문자열 i18n.
	import { locale, t } from '$lib/stores/locale';
	import { recentsApi, type Recent } from '$lib/api/recents';
	import { detectEnvironment } from '$lib/api/transport';
	// DEV-138: welcome 에서도 ⚙ 퀵메뉴 (Nav 와 동일 컴포넌트).
	import SettingsQuickMenu from '$lib/components/SettingsQuickMenu.svelte';
	// DEV-154: 호환 안 되는 길드(더 새 schema) 전용 안내 + 업데이트 확인.
	import ConfirmDialog from '$lib/components/ConfirmDialog.svelte';
	// DEV-259: 결과 표시는 전역 알림 호스트(ToastHost, +layout.svelte 에
	// mount)가 담당 — 여기선 체크 트리거만.
	import { checkForUpdate } from '$lib/api/updater';
	// DEV-113: 원격 서버 연결 — "어떤 길드를 열지" 선택이라 길드 열기와 같은
	// Welcome 화면에서 처리(설정 페이지에서 연결하는 건 자리가 어색하다는 피드백).
	import {
		setRemoteServerUrl,
		pingRemoteServer,
		markRemoteSessionActive
	} from '$lib/stores/remoteServer';
	import { markGuildContextInactive } from '$lib/stores/guildSession';
	// DEV-113 후속(사용자 피드백: "원격 길드도 등록하면 일반 길드처럼 접속할
	// 수 있어야하는데 지금은 이해가 안 된다"): 원격 연결을 local recents 와
	// 동등한 "최근 목록" 항목으로 — 한 번 연결하면 기록에 남아 다음부터는
	// 클릭만으로 재연결.
	import {
		listRemoteGuilds,
		registerRemoteGuild,
		removeRemoteGuild,
		clearRemoteGuilds,
		type RemoteGuild
	} from '$lib/stores/remoteGuilds';
	let quickMenuOpen = $state(false);
	function runUpdateCheck() {
		incompatibleMsg = null;
		void checkForUpdate();
	}

	// 더 새 schema 길드 열기 시도 시 메시지 (commands.rs 의 INCOMPATIBLE_GUILD_TAG).
	const INCOMPAT_TAG = 'INCOMPATIBLE_GUILD::';
	let incompatibleMsg = $state<string | null>(null);
	// 길드 열기/초기화 에러 공통 처리 — IncompatibleGuild 면 전용 모달, 그 외는
	// 일반 메시지 반환(호출 측이 openErr/initErr 등에 대입).
	function handleOpenError(e: unknown): string | null {
		const msg = e instanceof Error ? e.message : String(e);
		if (msg.startsWith(INCOMPAT_TAG)) {
			incompatibleMsg = msg.slice(INCOMPAT_TAG.length).trim();
			return null;
		}
		return msg;
	}

	let recents: Recent[] = $state([]);
	// DEV-113 후속: 원격 길드 "최근 목록" — local recents 와 같은 자리에 합쳐서 표시.
	let remoteGuildList: RemoteGuild[] = $state([]);
	// DEV-206(사용자 보고): 이전에 연결했던 원격 길드가 항상 활성 상태로 보여
	// 서버가 죽어있어도 클릭해야만 실패를 알 수 있었다. local recents 의
	// missing(경로 없음 → 회색 비활성화)과 대칭으로, url → 확인 결과(아직
	// 없으면 "확인 중" = 비활성화, true = 활성화, false = 비활성화 + 경고).
	let remoteReachable = $state<Record<string, boolean>>({});
	let loading = $state(true);
	let err: string | null = $state(null);
	let confirmOpen = $state(false); // 브라우저 confirm 대신 커스텀 모달.
	let opening: string | null = $state(null); // 진행 중인 path (UI 비활성화). remote 는 동기라 불필요.
	let openErr: string | null = $state(null);

	// DEV-113 후속: local(path 기준) 과 remote(url 기준) 를 하나의 목록으로 —
	// "원격 길드도 등록하면 일반 길드처럼 접속할 수 있어야" 피드백 반영.
	// kind 로 열기/제거 핸들러와 표시(path vs url)를 분기.
	type UnifiedEntry =
		| { kind: 'local'; path: string; name: string; last_opened: string; missing: boolean }
		| { kind: 'remote'; url: string; name: string; last_opened: string };

	let unified: UnifiedEntry[] = $derived(
		(
			[
				...recents.map((r) => ({ kind: 'local' as const, ...r })),
				...remoteGuildList.map((g) => ({ kind: 'remote' as const, ...g }))
			] satisfies UnifiedEntry[]
		).sort((a, b) => (a.last_opened < b.last_opened ? 1 : -1))
	);

	// DEV-052 후속 (2회차): 길드 미초기화 디렉토리에서 시작했을 때의 prompt.
	let uninitPath: string | null = $state(null);
	let initRunning = $state(false);
	let initErr: string | null = $state(null);
	// DEV-052 후속 (4회차): 길드 이름 input. 기본값은 디렉토리명.
	let initName = $state('');
	// DEV-053: "폴더 열기" 진행 상태.
	let pickRunning = $state(false);
	let pickErr: string | null = $state(null);

	const env = detectEnvironment();

	onMount(async () => {
		// DEV-207 후속: Welcome 에 도달했다는 건 "아직 길드를 안 골랐다"는
		// 뜻 — 직전에 어떤 길드를 봤었든(로컬/원격) 설정 페이지의 "지금 열려
		// 있음" 판단은 여기서 항상 리셋. 길드를 다시 열면(routes/+page.svelte
		// 의 board onMount) 다시 active 로 마크됨.
		markGuildContextInactive();
		// recents 먼저 로드.
		try {
			recents = await recentsApi.list();
		} catch (e) {
			err = e instanceof Error ? e.message : String(e);
		} finally {
			loading = false;
		}
		remoteGuildList = listRemoteGuilds();
		// DEV-206: 목록에 뜬 원격 길드 전부를 백그라운드에서 병렬로 ping —
		// 응답 오는 항목부터 차례로 활성화(전체 대기 없이 즉시 반영).
		for (const g of remoteGuildList) {
			pingRemoteServer(g.url)
				.then((ok) => {
					remoteReachable = { ...remoteReachable, [g.url]: ok };
				})
				.catch(() => {
					remoteReachable = { ...remoteReachable, [g.url]: false };
				});
		}
		// launch_mode 가 'uninit' 이면 prompt 활성화.
		if (env === 'tauri') {
			try {
				const { invoke } = await import('@tauri-apps/api/core');
				const info = await invoke<{ mode: string; uninit_path: string | null }>('launch_mode');
				if (info.mode === 'uninit' && info.uninit_path) {
					uninitPath = info.uninit_path;
					// 기본 길드 이름 = path 의 마지막 component (디렉토리명).
					// Windows / POSIX 모두에서 동작하도록 둘 다 split.
					const parts = info.uninit_path.split(/[\\/]/).filter(Boolean);
					initName = parts[parts.length - 1] ?? 'guild';
				}
			} catch {
				// invoke 실패 시 무시.
			}
		}
	});

	async function openRecent(path: string) {
		// DEV-052 후속 (2회차): 현재 창에서 store swap (새 창 spawn 안 함).
		// Tauri 환경 외에서 호출되면 에러 표시.
		if (opening) return;
		opening = path;
		openErr = null;
		try {
			if (env !== 'tauri') {
				throw new Error(t('welcome.tauriOnly', $locale));
			}
			// DEV-113 후속: local 길드를 열 때는 이전에 연결해둔 원격 서버
			// override 를 반드시 끈다 — 안 그러면 Rust 의 Store 는 이 local
			// 길드로 swap 되는데 transport(매 호출 시 remoteServerUrl 우선
			// 확인)는 여전히 옛 원격 URL 로 HTTP 호출해, "로컬을 열었는데
			// 화면엔 그대로 원격 데이터"가 보이는 혼란이 생긴다(사용자 보고:
			// 원격/로컬 전환이 이해가 안 된다).
			setRemoteServerUrl(null);
			const { invoke } = await import('@tauri-apps/api/core');
			await invoke('open_guild_in_current_window', { path });
			// 성공: 현재 process 의 Store 가 swap 됐음. 보드로 이동.
			goto('/');
		} catch (e) {
			openErr = handleOpenError(e);
		} finally {
			opening = null;
		}
	}

	async function initUninit() {
		if (!uninitPath || initRunning) return;
		const name = initName.trim();
		if (!name) {
			initErr = t('welcome.enterGuildName', $locale);
			return;
		}
		initRunning = true;
		initErr = null;
		try {
			setRemoteServerUrl(null); // DEV-113 후속 — openRecent 와 동일 이유.
			const { invoke } = await import('@tauri-apps/api/core');
			await invoke('init_and_open_guild', { path: uninitPath, name });
			// 성공: store swap 됨. 보드로.
			goto('/');
		} catch (e) {
			initErr = handleOpenError(e);
		} finally {
			initRunning = false;
		}
	}

	// DEV-053: 네이티브 폴더 다이얼로그로 길드 폴더 선택. 마커가 있으면 바로 열고,
	// 없으면 "이 위치를 길드로 초기화?" 인라인 prompt(uninitPath) 를 띄운다.
	async function pickFolder() {
		if (pickRunning) return;
		if (env !== 'tauri') {
			pickErr = t('welcome.tauriOnly', $locale);
			return;
		}
		pickRunning = true;
		pickErr = null;
		try {
			const { open } = await import('@tauri-apps/plugin-dialog');
			const selected = await open({
				directory: true,
				multiple: false,
				title: t('welcome.pickDialogTitle', $locale)
			});
			if (!selected) return; // 취소.
			const dir = typeof selected === 'string' ? selected : selected[0];
			if (!dir) return;

			const { invoke } = await import('@tauri-apps/api/core');
			const info = await invoke<{
				exists: boolean;
				is_dir: boolean;
				has_marker: boolean;
				resolved_path: string;
			}>('inspect_guild_path', { path: dir });

			if (!info.is_dir) {
				pickErr = `${t('welcome.notValidDir', $locale)}: ${info.resolved_path}`;
				return;
			}
			if (info.has_marker) {
				// 기존 길드 → 바로 현재 창에서 열기.
				setRemoteServerUrl(null); // DEV-113 후속 — openRecent 와 동일 이유.
				await invoke('open_guild_in_current_window', { path: info.resolved_path });
				goto('/');
			} else {
				// 마커 없음 → 인라인 초기화 prompt 활성화.
				uninitPath = info.resolved_path;
				const parts = info.resolved_path.split(/[\\/]/).filter(Boolean);
				initName = parts[parts.length - 1] ?? 'guild';
			}
		} catch (e) {
			pickErr = handleOpenError(e);
		} finally {
			pickRunning = false;
		}
	}

	// DEV-052 후속 (5회차): 단일 항목 제거 — 모든 항목에 × 버튼.
	// 확인 모달 거쳐서 실수 방지. DEV-113 후속: local/remote 공용 — kind 로 분기.
	let confirmRemove: UnifiedEntry | null = $state(null);

	function askRemove(entry: UnifiedEntry) {
		confirmRemove = entry;
	}

	function cancelRemove() {
		confirmRemove = null;
	}

	async function doRemove() {
		const target = confirmRemove;
		if (!target) return;
		confirmRemove = null;
		try {
			if (target.kind === 'local') {
				await recentsApi.remove(target.path);
				recents = recents.filter((r) => r.path !== target.path);
			} else {
				removeRemoteGuild(target.url);
				remoteGuildList = remoteGuildList.filter((g) => g.url !== target.url);
			}
		} catch (e) {
			openErr = handleOpenError(e);
		}
	}

	function declineUninit() {
		// 사용자가 "초기화 안 함" 선택 → uninitPath 만 비워 prompt 닫음 (welcome 으로 머묾).
		uninitPath = null;
	}

	function askClear() {
		confirmOpen = true;
	}

	async function doClear() {
		confirmOpen = false;
		await recentsApi.clear();
		recents = [];
		// DEV-113 후속: "전체 비우기" 가 이제 통합 목록을 비우는 동작이라 원격
		// 기록도 함께.
		clearRemoteGuilds();
		remoteGuildList = [];
	}

	function cancelClear() {
		confirmOpen = false;
	}

	function fmtDate(iso: string): string {
		const d = new Date(iso);
		if (Number.isNaN(d.getTime())) return iso;
		const pad = (n: number) => String(n).padStart(2, '0');
		return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())} ${pad(d.getHours())}:${pad(d.getMinutes())}`;
	}

	// DEV-113 후속: 원격 서버 연결 — URL 입력 + 연결 확인 + 연결(목록에 등록 +
	// 활성화 + 보드로 이동). 한 번 연결하면 unified 목록에 남아 다음부터는
	// 그냥 클릭으로 재연결("원격 길드도 등록하면 일반 길드처럼 접속할 수
	// 있어야" 피드백).
	let remoteInput = $state('');
	let remoteCheckState = $state<'idle' | 'checking' | 'ok' | 'fail'>('idle');
	let remoteCheckMsg = $state<string | null>(null);

	async function checkRemote() {
		const url = remoteInput.trim();
		if (!url) return;
		remoteCheckState = 'checking';
		remoteCheckMsg = null;
		try {
			const ok = await pingRemoteServer(url);
			remoteCheckState = ok ? 'ok' : 'fail';
			if (!ok) remoteCheckMsg = t('welcome.badResponse', $locale);
		} catch (e) {
			remoteCheckState = 'fail';
			remoteCheckMsg = e instanceof Error ? e.message : String(e);
		}
	}

	// 목록에 있는 기존 원격 길드 클릭 — 재입력 없이 바로 재연결. 새 URL 연결
	// (connectRemote)과 핵심 로직 공유 — core::recents::add 가 로컬 열기
	// 성공 시 자동으로 LRU 갱신하는 것과 동등하게, 연결 성공 시 registerRemoteGuild
	// 로 갱신.
	function openRemoteEntry(url: string) {
		// BUG-095: 이번 세션에서 사용자가 직접 연결했음을 표시 — board 의
		// bounce guard 가 콜드 스타트(이전 세션의 localStorage 잔존 값)와
		// 구분하는 기준.
		markRemoteSessionActive();
		setRemoteServerUrl(url);
		registerRemoteGuild(url);
		remoteGuildList = listRemoteGuilds();
		goto('/');
	}

	// 새 URL 연결 — 확인(ping) 없이도 시도 가능(신뢰된 서버 주소를 이미 아는 경우).
	function connectRemote() {
		const url = remoteInput.trim();
		if (!url) return;
		openRemoteEntry(url);
		remoteInput = '';
		remoteCheckState = 'idle';
		remoteCheckMsg = null;
	}

	// local 길드를 열면 항상 setRemoteServerUrl(null) 로 원격 override 를
	// 끈다 — openRecent / initUninit / pickFolder 참조.
</script>

<svelte:head>
	<title>Welcome — openguild</title>
</svelte:head>

<main class="welcome">
	<header>
		<div class="title-row">
			<div>
				<h1>openguild</h1>
				<p class="sub">{t('welcome.sub', $locale)}</p>
			</div>
			<!-- DEV-052 fix → DEV-138: welcome 에서도 ⚙ 가 퀵메뉴 (Nav 와 동일).
				 Nav 가 가려져 있으므로 페이지 자체에 톱니바퀴. -->
			<div class="settings-wrap">
				<button
					class="settings-link"
					class:active={quickMenuOpen}
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

	{#if env === 'tauri'}
		<!-- DEV-053: 파일 탐색기로 임의 위치의 길드 열기. DEV-204 후속(admin
		     "revert 해라"): .guild 마커 파일 직접 선택 옵션은 폐기 — 원래의
		     단일 폴더 다이얼로그로 되돌림. -->
		<section class="picker">
			<button class="btn-pick" onclick={pickFolder} disabled={pickRunning}>
				{pickRunning ? t('welcome.opening', $locale) : t('welcome.pickFolder', $locale)}
			</button>
			<span class="picker-hint">
				{t('welcome.pickHint', $locale)}
			</span>
			{#if pickErr}
				<p class="err">{pickErr}</p>
			{/if}
		</section>

		<!-- DEV-113 후속: 원격 서버 연결 — "어떤 길드를 열지"의 또 다른 선택이라
		     길드 열기와 같은 화면에서. 연결하면 아래 "최근 길드" 목록에 등록되어
		     일반 길드처럼 클릭 한 번으로 다시 열 수 있다. local 과 대칭으로 "현재
		     연결 상태" 표시는 없음(local 도 "현재 열려있는 길드" 표시가 없음) —
		     사용자 피드백: 별도 "로컬로 전환" 버튼이 무슨 역할인지 혼란스러움.
		     로컬로 돌아가려면 아래 목록에서 local 항목을 클릭하면 됨(그 경로들이
		     이미 setRemoteServerUrl(null) 처리). 설정 페이지엔 현재 연결 상태만
		     읽기 전용 표시. -->
		<section class="picker remote-picker">
			<div class="remote-input-row">
				<input
					type="text"
					placeholder={t('welcome.remotePlaceholder', $locale)}
					bind:value={remoteInput}
					aria-label={t('welcome.remoteAria', $locale)}
				/>
				<button class="btn-pick alt" onclick={checkRemote} disabled={!remoteInput.trim()}>
					{remoteCheckState === 'checking' ? t('welcome.checking', $locale) : t('welcome.checkConn', $locale)}
				</button>
				<button class="btn-pick" onclick={connectRemote} disabled={!remoteInput.trim()}>
					{t('welcome.connect', $locale)}
				</button>
			</div>
			{#if remoteCheckState === 'ok'}
				<p class="remote-check ok">{t('welcome.connOk', $locale)}</p>
			{:else if remoteCheckState === 'fail'}
				<p class="remote-check err">{t('welcome.connFail', $locale)}{remoteCheckMsg ? `: ${remoteCheckMsg}` : ''}</p>
			{/if}
			<span class="picker-hint">
				{t('welcome.remoteHint1', $locale)}<strong>{t('welcome.remoteHintStrong', $locale)}</strong>{t('welcome.remoteHint2', $locale)}
			</span>
		</section>
	{/if}

	{#if uninitPath}
		<!-- DEV-052 후속: 길드 마커 없는 디렉토리에서 시작 → 초기화 prompt. -->
		<section class="uninit">
			<h2>{t('welcome.uninitTitle', $locale)}</h2>
			<p class="uninit-path">{uninitPath}</p>
			<p class="uninit-desc">
				{t('welcome.uninitDesc1', $locale)}<code>{t('welcome.markerExample', $locale)}</code>{t('welcome.uninitDesc2', $locale)}<code>.guild/</code>{t('welcome.uninitDesc3', $locale)}
			</p>
			<label class="uninit-name">
				<span>{t('welcome.guildName', $locale)}</span>
				<input type="text" bind:value={initName} placeholder="guild" disabled={initRunning} />
			</label>
			{#if initErr}
				<p class="err">{initErr}</p>
			{/if}
			<div class="uninit-actions">
				<button class="btn-yes" onclick={initUninit} disabled={initRunning}>
					{initRunning ? t('welcome.initializing', $locale) : t('welcome.initAndOpen', $locale)}
				</button>
				<button class="btn-no" onclick={declineUninit} disabled={initRunning}>{t('common.no', $locale)}</button>
			</div>
		</section>
	{/if}

	{#if loading}
		<p class="loading">{t('welcome.loading', $locale)}</p>
	{:else if err}
		<p class="err">{err}</p>
	{:else if env !== 'tauri'}
		<p class="info">
			{t('welcome.browserInfo1', $locale)}<br />
			{t('welcome.browserInfo2', $locale)}
		</p>
	{:else if unified.length === 0}
		<p class="empty">
			{t('welcome.empty1', $locale)}<br />
			<code>openguild init</code>{t('welcome.empty2', $locale)}<code>openguild-gui &lt;path&gt;</code>{t('welcome.empty3', $locale)}
		</p>
	{:else}
		<!-- DEV-113 후속: local + remote 를 하나의 목록으로(최근 연 순). -->
		<ul class="recent-list">
			{#each unified as entry (entry.kind === 'local' ? entry.path : entry.url)}
				{@const remoteOk = entry.kind === 'remote' && remoteReachable[entry.url] === true}
				{@const remoteChecked = entry.kind === 'remote' && entry.url in remoteReachable}
				<li class="recent-row" class:missing={entry.kind === 'local' ? entry.missing : !remoteOk}>
					{#if entry.kind === 'local'}
						<button
							class="recent-btn"
							type="button"
							onclick={() => openRecent(entry.path)}
							disabled={opening !== null || entry.missing}
							title={entry.missing
								? t('welcome.pathMissing', $locale)
								: t('welcome.openInWindow', $locale)}
						>
							<div class="row">
								<span class="name">{entry.name}</span>
								<span class="last">{fmtDate(entry.last_opened)}</span>
							</div>
							<div class="path">{entry.path}</div>
							{#if entry.missing}
								<div class="missing-label">{t('welcome.pathNotFound', $locale)}</div>
							{/if}
							{#if opening === entry.path}
								<div class="opening">{t('welcome.guildOpening', $locale)}</div>
							{/if}
						</button>
					{:else}
						<button
							class="recent-btn"
							type="button"
							onclick={() => openRemoteEntry(entry.url)}
							disabled={opening !== null || !remoteOk}
							title={!remoteChecked
								? t('welcome.checkingConn', $locale)
								: remoteOk
									? t('welcome.connectRemote', $locale)
									: t('welcome.serverUnreachable', $locale)}
						>
							<div class="row">
								<span class="name">🌐 {entry.name}</span>
								<span class="last">{fmtDate(entry.last_opened)}</span>
							</div>
							<div class="path">{entry.url}</div>
							{#if !remoteChecked}
								<div class="checking-label">{t('welcome.checkingConn', $locale)}</div>
							{:else if !remoteOk}
								<div class="missing-label">{t('welcome.serverUnreachableWarn', $locale)}</div>
							{/if}
						</button>
					{/if}
					<!-- DEV-052 후속 (5회차): 모든 항목에 × — 단일 삭제 + 확인 모달. -->
					<button
						class="recent-remove"
						type="button"
						onclick={() => askRemove(entry)}
						title={t('welcome.removeFromList', $locale)}
						aria-label={t('welcome.removeFromList', $locale)}
					>
						×
					</button>
				</li>
			{/each}
		</ul>
		{#if openErr}
			<p class="err">{openErr}</p>
		{/if}
		<button class="clear" onclick={askClear}>{t('welcome.clearAll', $locale)}</button>
	{/if}

	<footer class="hint">
		<p>{t('welcome.footerHint', $locale)}</p>
	</footer>
</main>

{#if confirmOpen}
	<!-- DEV-052 후속: 브라우저 confirm() 대신 인앱 스타일 모달. -->
	<div class="ov" role="presentation">
		<div class="modal" role="dialog" aria-modal="true" tabindex="-1">
			<h3 class="modal-title">{t('welcome.clearTitle', $locale)}</h3>
			<p class="modal-msg">{t('welcome.clearMsg', $locale)}</p>
			<div class="modal-actions">
				<button class="btn-yes" onclick={doClear}>{t('welcome.clearConfirm', $locale)}</button>
				<button class="btn-no" onclick={cancelClear}>{t('common.cancel', $locale)}</button>
			</div>
		</div>
	</div>
{/if}

{#if confirmRemove}
	<!-- DEV-052 후속 (5회차): 단일 항목 제거 확인. -->
	<div class="ov" role="presentation">
		<div class="modal" role="dialog" aria-modal="true" tabindex="-1">
			<h3 class="modal-title">{t('welcome.removeTitle', $locale)}</h3>
			<p class="modal-msg">
				<strong>{confirmRemove.name}</strong>{t('welcome.removeSuffix', $locale)}
			</p>
			<p class="modal-path">
				{confirmRemove.kind === 'local' ? confirmRemove.path : confirmRemove.url}
			</p>
			<p class="modal-msg modal-note">
				{confirmRemove.kind === 'local'
					? t('welcome.removeNoteLocal', $locale)
					: t('welcome.removeNoteRemote', $locale)}
			</p>
			<div class="modal-actions">
				<button class="btn-yes" onclick={doRemove}>{t('welcome.remove', $locale)}</button>
				<button class="btn-no" onclick={cancelRemove}>{t('common.cancel', $locale)}</button>
			</div>
		</div>
	</div>
{/if}

<!-- DEV-154: 더 새 schema 길드 — 전용 안내 + 업데이트 확인 (DEV-063). -->
<ConfirmDialog
	open={incompatibleMsg !== null}
	title={t('welcome.incompatTitle', $locale)}
	message={incompatibleMsg ?? ''}
	confirmLabel={t('welcome.updateCheck', $locale)}
	oncancel={() => (incompatibleMsg = null)}
	onconfirm={runUpdateCheck}
/>

<style>
	.welcome {
		max-width: var(--content-max-width, 720px);
		margin: 0 auto;
		padding: 2rem 1.5rem;
		color: var(--text);
	}
	header h1 {
		margin: 0;
		font-size: 2rem;
		color: var(--accent);
	}
	header .sub {
		margin: 0.25rem 0 1.5rem;
		color: var(--text-muted);
	}
	/* DEV-052 fix: welcome 의 설정 진입 — 우상단 톱니바퀴. */
	header .title-row {
		display: flex;
		align-items: flex-start;
		justify-content: space-between;
		gap: 1rem;
	}
	/* DEV-138: 퀵메뉴 anchor. */
	.settings-wrap {
		position: relative;
	}
	.settings-link {
		display: inline-flex;
		align-items: center;
		justify-content: center;
		width: 2.25rem;
		height: 2.25rem;
		border-radius: 8px;
		background: transparent;
		border: none;
		cursor: pointer;
		text-decoration: none;
		color: var(--text-muted);
		background: var(--bg-elevated);
		border: 1px solid var(--border);
		font-size: 1.1rem;
		transition:
			background 0.1s,
			color 0.1s,
			border-color 0.1s;
	}
	.settings-link:hover {
		color: var(--text);
		background: var(--bg-subtle);
		border-color: var(--text-faint);
	}
	.recent-list {
		list-style: none;
		padding: 0;
		margin: 0;
		display: flex;
		flex-direction: column;
		gap: 0.5rem;
	}
	.recent-list .recent-row {
		display: flex;
		gap: 0.5rem;
		align-items: stretch;
	}
	.recent-list .recent-row.missing .recent-btn {
		opacity: 0.55;
		cursor: not-allowed;
	}
	.recent-remove {
		flex: 0 0 auto;
		padding: 0 0.85rem;
		background: transparent;
		border: 1px solid var(--border);
		border-radius: 6px;
		color: var(--text-muted);
		font-size: 1.2rem;
		line-height: 1;
		cursor: pointer;
		transition:
			border-color 0.12s,
			color 0.12s,
			background 0.12s;
	}
	.recent-remove:hover {
		border-color: var(--danger);
		color: var(--danger);
		background: rgba(233, 79, 79, 0.08);
	}
	.recent-btn {
		flex: 1 1 auto;
	}
	.missing-label {
		color: var(--warning);
		font-size: 0.8rem;
	}
	/* DEV-206: 원격 길드 ping 확인 중 — 아직 경고는 아니므로 muted. */
	.checking-label {
		color: var(--text-muted);
		font-size: 0.8rem;
	}
	.recent-btn {
		width: 100%;
		text-align: left;
		padding: 0.75rem 1rem;
		background: var(--bg-elevated);
		border: 1px solid var(--border);
		border-radius: 6px;
		color: inherit;
		font: inherit;
		cursor: pointer;
		display: grid;
		grid-template-columns: 1fr;
		gap: 0.25rem;
		transition:
			border-color 0.12s,
			background 0.12s;
	}
	.recent-list .recent-row:not(.missing) .recent-btn:hover:not(:disabled) {
		border-color: var(--accent);
		background: var(--bg-subtle);
	}
	.recent-btn:disabled {
		opacity: 0.6;
		cursor: default;
	}
	.recent-btn .row {
		display: flex;
		justify-content: space-between;
		align-items: baseline;
		gap: 1rem;
	}
	.recent-btn .name {
		font-weight: 600;
		font-size: 1.05rem;
	}
	.recent-btn .last {
		color: var(--text-muted);
		font-size: 0.85rem;
		font-family: 'SFMono-Regular', Consolas, monospace;
	}
	.recent-btn .path {
		color: var(--text-muted);
		font-size: 0.85rem;
		font-family: 'SFMono-Regular', Consolas, monospace;
		word-break: break-all;
	}
	.recent-btn .opening {
		color: var(--accent);
		font-size: 0.8rem;
	}
	.clear {
		margin-top: 1rem;
		padding: 0.4rem 0.9rem;
		background: transparent;
		color: var(--danger);
		border: 1px solid var(--border);
		border-radius: 4px;
		font-size: 0.85rem;
		cursor: pointer;
	}
	.clear:hover {
		border-color: var(--danger);
	}
	.loading,
	.empty,
	.info,
	.err {
		padding: 1rem;
		background: var(--bg-elevated);
		border-radius: 6px;
		color: var(--text-muted);
	}
	.err {
		color: var(--danger);
		margin-top: 1rem;
	}
	code {
		background: var(--bg);
		padding: 0.1rem 0.4rem;
		border-radius: 3px;
		font-family: 'SFMono-Regular', Consolas, monospace;
		font-size: 0.9em;
	}
	.hint {
		margin-top: 2rem;
		padding-top: 1rem;
		border-top: 1px solid var(--border);
		color: var(--text-muted);
		font-size: 0.85rem;
	}

	/* --- DEV-053: 폴더 열기 picker --- */
	.picker {
		display: flex;
		align-items: center;
		flex-wrap: wrap;
		gap: 0.85rem;
		margin: 0 0 1.25rem;
		padding: 0.85rem 1rem;
		background: var(--bg-elevated);
		border: 1px solid var(--border);
		border-radius: 8px;
	}
	.btn-pick {
		padding: 0.5rem 1rem;
		background: var(--accent-strong);
		border: 1px solid var(--accent);
		border-radius: 6px;
		color: var(--btn-primary-text);
		font-size: 0.9rem;
		font-weight: 500;
		cursor: pointer;
		transition: background 0.12s;
		flex: 0 0 auto;
	}
	.btn-pick:hover:not(:disabled) {
		background: var(--accent);
	}
	.btn-pick:disabled {
		opacity: 0.6;
		cursor: default;
	}
	.picker-hint {
		flex: 1 1 auto;
		min-width: 12.5rem; /* BUG-064 */
		color: var(--text-muted);
		font-size: 0.825rem;
	}
	.picker .err {
		flex: 1 0 100%;
		margin: 0;
		padding: 0.5rem 0.75rem;
		font-size: 0.85rem;
	}
	/* DEV-113: 원격 서버 연결 섹션 — 길드 폴더 열기 picker 와 같은 톤, 살짝 구분. */
	.remote-picker {
		margin-top: -0.4rem;
	}
	.remote-input-row {
		display: flex;
		flex-wrap: wrap;
		gap: 0.5rem;
		flex: 1 0 100%;
	}
	.remote-input-row input {
		flex: 1 1 auto;
		min-width: 12.5rem;
		padding: 0.45rem 0.6rem;
		border: 1px solid var(--border);
		border-radius: 6px;
		background: var(--bg);
		color: var(--text);
		font-size: 0.85rem;
	}
	.btn-pick.alt {
		background: transparent;
		color: var(--text);
		border-color: var(--border);
	}
	.btn-pick.alt:hover:not(:disabled) {
		background: var(--bg-subtle);
	}
	.remote-check {
		flex: 1 0 100%;
		margin: 0;
		font-size: 0.8rem;
	}
	.remote-check.ok {
		color: var(--success);
	}
	.remote-check.err {
		color: var(--danger);
	}

	/* --- uninit prompt (DEV-052 후속 2회차) --- */
	.uninit {
		margin-bottom: 1.5rem;
		padding: 1rem 1.25rem;
		background: var(--bg-subtle);
		border: 1px solid var(--accent);
		border-radius: 8px;
	}
	.uninit h2 {
		margin: 0 0 0.5rem;
		font-size: 1.05rem;
		color: var(--accent);
	}
	.uninit-path {
		margin: 0 0 0.5rem;
		padding: 0.4rem 0.6rem;
		background: var(--bg);
		border-radius: 4px;
		font-family: 'SFMono-Regular', Consolas, monospace;
		font-size: 0.85rem;
		color: var(--text);
		word-break: break-all;
	}
	.uninit-desc {
		margin: 0 0 0.85rem;
		color: var(--text);
		font-size: 0.875rem;
	}
	.uninit-name {
		display: flex;
		align-items: center;
		gap: 0.6rem;
		margin: 0 0 0.85rem;
		font-size: 0.875rem;
		color: var(--text);
	}
	.uninit-name > span {
		flex: 0 0 auto;
		color: var(--text-muted);
	}
	.uninit-name input {
		flex: 1 1 auto;
		padding: 0.4rem 0.6rem;
		background: var(--bg);
		border: 1px solid var(--border);
		border-radius: 4px;
		color: var(--text);
		font: inherit;
		font-family: 'SFMono-Regular', Consolas, monospace;
	}
	.uninit-name input:focus {
		outline: none;
		border-color: var(--accent);
	}
	.uninit-actions {
		display: flex;
		gap: 0.5rem;
		justify-content: flex-end;
	}

	/* --- 커스텀 confirm 모달 --- */
	.ov {
		position: fixed;
		inset: 0;
		background: rgba(0, 0, 0, 0.6);
		z-index: 100;
		display: flex;
		align-items: center;
		justify-content: center;
		padding: 1rem;
	}
	.modal {
		background: var(--bg-elevated);
		border: 1px solid var(--border);
		border-radius: 10px;
		width: 100%;
		max-width: calc(26.25rem * var(--popup-scale, 1)); /* BUG-064 */
		padding: 1.2rem 1.4rem;
		box-shadow: 0 12px 36px rgba(0, 0, 0, 0.6);
		color: var(--text);
	}
	.modal-title {
		margin: 0 0 0.5rem;
		font-size: 1rem;
		font-weight: 600;
		color: var(--text-strong);
	}
	.modal-msg {
		margin: 0 0 1rem;
		font-size: 0.875rem;
		color: var(--text);
	}
	.modal-msg strong {
		color: var(--text-strong);
	}
	.modal-path {
		margin: -0.5rem 0 0.85rem;
		padding: 0.4rem 0.6rem;
		background: var(--bg);
		border-radius: 4px;
		font-family: 'SFMono-Regular', Consolas, monospace;
		font-size: 0.8rem;
		color: var(--text-muted);
		word-break: break-all;
	}
	.modal-note {
		font-size: 0.8rem;
		color: var(--text-muted);
	}
	.modal-actions {
		display: flex;
		gap: 0.5rem;
		justify-content: flex-end;
	}
	.btn-yes {
		padding: 0.4rem 1.1rem;
		background: rgba(233, 79, 79, 0.15);
		border: 1px solid var(--danger);
		border-radius: 6px;
		color: var(--danger);
		font-size: 0.875rem;
		cursor: pointer;
	}
	.btn-yes:hover {
		background: rgba(233, 79, 79, 0.25);
	}
	.btn-no {
		padding: 0.4rem 1rem;
		background: transparent;
		border: 1px solid var(--border);
		border-radius: 6px;
		color: var(--text-muted);
		font-size: 0.875rem;
		cursor: pointer;
	}
	.btn-no:hover {
		background: var(--bg-subtle);
	}
</style>
