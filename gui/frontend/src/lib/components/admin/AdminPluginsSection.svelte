<!--
  DEV-379: 플러그인 — 이 길드의 플러그인 목록, 동의(허용·철회·일괄), 설정값.

  DEV-393: 설정 페이지에서 관리 페이지로 옮겼다. 여기 나오는 것은 전부 **지금 연 길드의 것**
  이다 — 목록은 그 길드의 `.guild/plugins/`, 동의와 설정값도 길드별로 따로 남는다. 앱 전체
  설정 화면에 두니 "아직 길드를 열지 않았습니다" 같은 상태를 따로 다뤄야 했다.

  DEV-400: 길드 밖 폴더(소스)의 플러그인도 여기서 더하고 뺀다. **더하는 것과 허용은 다른 일**
  이라, 더한 직후 그 항목으로 데려가 스크립트를 펼쳐 둔다 — 무엇에 동의하는지 본 자리에서
  허용한다. DEV-394: [다시 읽기] 는 디스크의 정의로 다시 꽂는다(자동 감지는 없다).
-->
<script lang="ts">
	import { onMount, tick } from 'svelte';
	// DEV-379/380: 조회는 어디서든(브라우저 포함), 허용/철회는 로컬 길드를 연 데스크톱에서만.
	import {
		pluginApi,
		pluginsManageable,
		type PluginStatus,
		type PluginInput,
		type PluginSource,
		type PluginView
	} from '$lib/api/plugins';
	import { locale, t } from '$lib/stores/locale';
	import { showToast } from '$lib/stores/toast';
	import ConfirmDialog from '$lib/components/ConfirmDialog.svelte';

	// ─── DEV-379: 플러그인 ───
	//
	// 묻는 **시점**을 길드 열 때가 아니라 여기로 잡았다. 열자마자 대화상자로
	// 막으면 플러그인을 안 쓰는 사람까지 방해한다. 대신 안 돌고 있는 것이
	// 목록에서 바로 보인다.
	let pluginStatus = $state<PluginStatus | null>(null);
	let pluginError = $state<string | null>(null);
	// DEV-383: 스칼라 하나로 잠그면 두 번째 클릭이 첫 번째의 잠금을 풀어 버린다.
	let pluginBusy = $state<string[]>([]);
	let openScript = $state<string | null>(null);
	// BUG-288: 확인이 필요한 일괄 조작 — 둘 다 **보지 않은 코드를 돌리게** 한다.
	let confirmBulk = $state<'allowAll' | 'autoOn' | null>(null);
	let bulkBusy = $state(false);
	// 관리 가능 여부는 **서버가 답한 값**으로 정한다. 프런트의 환경 감지만
	// 믿으면 원격 길드를 보면서 로컬 동의를 고치게 된다(BUG-255 계열).
	const canManage = $derived(pluginsManageable() && (pluginStatus?.manageable ?? false));
	// 답을 받기 **전에** 판단하면 안 된다 — 로컬 데스크톱에서도 잠깐
	// "조회만 가능합니다" 가 스쳐 지나가고, 실패하면 그 상태로 굳는다.
	const pluginsLoaded = $derived(pluginStatus !== null);

	async function refreshPlugins() {
		try {
			pluginStatus = await pluginApi.status();
			pluginError = null;
		} catch (e) {
			// DEV-383: 직전 목록을 지우지 않는다. 지우면 목록이 통째로 사라지고
			// "다른 기계에서 하세요" 로 바뀌어, 데스크톱 앱 안에서 데스크톱 앱을
			// 쓰라는 화면이 된다.
			pluginError = e instanceof Error ? e.message : String(e);
		}
	}

	// DEV-393: 관리 페이지의 탭이 열릴 때 이 컴포넌트가 붙는다 — 붙을 때 한 번 읽는다.
	onMount(refreshAll);

	// DEV-384: 동작 뒤 목록이 다시 그려지면 눌렀던 버튼이 사라져 포커스가
	// `<body>` 로 떨어진다. 그러면 다음 플러그인까지 앱 맨 위에서부터 Tab 해야
	// 한다. 이름으로 버튼을 기억해 두었다가 되돌린다.
	const pluginBtns: Record<string, HTMLButtonElement | undefined> = {};

	// ─── REQ-021: 플러그인 설정값 ───
	//
	// 편집 중인 값은 화면에만 두고, 저장을 눌러야 파일에 간다. 타이핑마다
	// 저장하면 토큰을 반쯤 친 상태가 파일에 남고, 그 사이에 이벤트가 나가면
	// 깨진 값으로 요청이 간다.
	let inputDrafts = $state<Record<string, string | number | boolean>>({});
	let inputBusy = $state<string[]>([]);

	function draftKey(plugin: string, key: string) {
		return `${plugin}\u0000${key}`;
	}

	/** 편집 중인 값이 있으면 그것, 없으면 서버가 준 현재 값. */
	function draftOf(plugin: string, i: PluginInput): string | number | boolean {
		const k = draftKey(plugin, i.key);
		if (k in inputDrafts) return inputDrafts[k];
		if (i.secret) return ''; // 비밀은 값이 안 온다 — 새로 칠 때만 쓴다.
		if (i.value === null || i.value === undefined) return i.type === 'checkbox' ? false : '';
		return i.value as string | number | boolean;
	}

	function setDraft(plugin: string, key: string, v: string | number | boolean) {
		inputDrafts = { ...inputDrafts, [draftKey(plugin, key)]: v };
	}

	function isDirty(plugin: string, i: PluginInput) {
		return draftKey(plugin, i.key) in inputDrafts;
	}

	async function saveInput(plugin: string, i: PluginInput) {
		const k = draftKey(plugin, i.key);
		inputBusy = [...inputBusy, k];
		try {
			let v = inputDrafts[k];
			if (i.type === 'number' && typeof v === 'string') v = Number(v);
			await pluginApi.setValue(plugin, i.key, v as string | number | boolean);
			const { [k]: _drop, ...rest } = inputDrafts;
			inputDrafts = rest;
			await refreshPlugins();
			showToast(t('plugins.valueSaved', $locale), 'success');
		} catch (e) {
			showToast(e instanceof Error ? e.message : String(e), 'error');
		} finally {
			inputBusy = inputBusy.filter((x) => x !== k);
		}
	}

	/** 저장된 값을 지운다 — 기본값이나 환경변수로 되돌아간다. */
	async function clearInput(plugin: string, i: PluginInput) {
		const k = draftKey(plugin, i.key);
		inputBusy = [...inputBusy, k];
		try {
			await pluginApi.setValue(plugin, i.key, null);
			const { [k]: _drop, ...rest } = inputDrafts;
			inputDrafts = rest;
			await refreshPlugins();
		} catch (e) {
			showToast(e instanceof Error ? e.message : String(e), 'error');
		} finally {
			inputBusy = inputBusy.filter((x) => x !== k);
		}
	}

	async function togglePlugin(name: string, grant: boolean) {
		pluginBusy = [...pluginBusy, name];
		try {
			await (grant ? pluginApi.allow(name) : pluginApi.revoke(name));
			await refreshPlugins();
			showToast(
				grant
					? t('plugins.allowed', $locale)
					: t('plugins.revoked', $locale),
				'success'
			);
			// 다시 그려진 뒤에 잡아야 새 버튼이 잡힌다.
			await tick();
			pluginBtns[name]?.focus();
		} catch (e) {
			showToast(e instanceof Error ? e.message : String(e), 'error');
		} finally {
			pluginBusy = pluginBusy.filter((n) => n !== name);
		}
	}

	// ─── DEV-400: 길드 밖 플러그인(소스) ───
	let sources = $state<PluginSource[]>([]);
	let sourceBusy = $state(false);
	/** 방금 더한 플러그인 — 눈에 띄게 하고 스크립트를 펼쳐 둔다. */
	let fresh = $state<string | null>(null);
	let confirmSource = $state<string | null>(null);
	let reloading = $state(false);

	async function refreshSources() {
		// 소스는 이 기계의 기록이다 — 관리할 수 없는 화면(웹·원격 길드)에서는 묻지도 않는다.
		if (!pluginsManageable()) return;
		try {
			sources = await pluginApi.sources();
		} catch (e) {
			pluginError = e instanceof Error ? e.message : String(e);
		}
	}

	async function refreshAll() {
		await refreshPlugins();
		await refreshSources();
	}

	/** 그 항목으로 데려가 스크립트를 펼친다 — 더한 다음 할 일은 "읽고 허용" 이다. */
	async function reveal(name: string) {
		fresh = name;
		openScript = name;
		await tick();
		document.getElementById(`plugin-${name}`)?.scrollIntoView?.({ block: 'center' });
		pluginBtns[name]?.focus();
	}

	async function addPluginFolder() {
		let dir: string | string[] | null;
		try {
			const { open } = await import('@tauri-apps/plugin-dialog');
			dir = await open({ directory: true, title: t('plugins.addPick', $locale) });
		} catch (e) {
			showToast(e instanceof Error ? e.message : String(e), 'error');
			return;
		}
		if (!dir || Array.isArray(dir)) return;
		sourceBusy = true;
		try {
			const out = await pluginApi.addFolder(dir);
			await refreshAll();
			if (out.kind === 'used') {
				showToast(t('plugins.addedUsed', $locale).replace('{name}', out.name), 'success');
				await reveal(out.name);
			} else {
				// 여럿이면 고르게 한다 — 통째로 켜지 않는다.
				showToast(
					t('plugins.addedRegistered', $locale)
						.replace('{source}', out.source)
						.replace('{n}', String(out.plugins)),
					'success'
				);
				await tick();
				document.getElementById(`plugin-source-${out.source}`)?.scrollIntoView?.({ block: 'center' });
			}
		} catch (e) {
			showToast(e instanceof Error ? e.message : String(e), 'error');
		} finally {
			sourceBusy = false;
		}
	}

	async function usePlugin(source: string, folder: string, name: string) {
		sourceBusy = true;
		try {
			await pluginApi.use(source, folder);
			await refreshAll();
			await reveal(name);
		} catch (e) {
			showToast(e instanceof Error ? e.message : String(e), 'error');
		} finally {
			sourceBusy = false;
		}
	}

	async function stopUsing(source: string, folder: string) {
		sourceBusy = true;
		try {
			await pluginApi.stopUsing(source, folder);
			await refreshAll();
			showToast(t('plugins.stopped', $locale), 'success');
		} catch (e) {
			showToast(e instanceof Error ? e.message : String(e), 'error');
		} finally {
			sourceBusy = false;
		}
	}

	/** 목록의 항목이 소스의 어느 폴더인지 — 뺄 때 이름이 아니라 짝으로 짚는다. */
	function folderOf(p: PluginView): string | null {
		if (!p.source) return null;
		return (
			sources
				.find((s) => s.name === p.source)
				?.plugins.find((x) => x.used && x.name === p.name)?.folder ?? null
		);
	}

	async function removeSource(name: string) {
		confirmSource = null;
		sourceBusy = true;
		try {
			await pluginApi.removeSource(name);
			await refreshAll();
		} catch (e) {
			showToast(e instanceof Error ? e.message : String(e), 'error');
		} finally {
			sourceBusy = false;
		}
	}

	/** DEV-394: 디스크의 정의로 다시 꽂는다. 바뀐 정의는 동의가 풀려 멈춘다. */
	async function reload() {
		reloading = true;
		try {
			pluginStatus = await pluginApi.reload();
			pluginError = null;
			await refreshSources();
			showToast(t('plugins.reloaded', $locale), 'success');
		} catch (e) {
			showToast(e instanceof Error ? e.message : String(e), 'error');
		} finally {
			reloading = false;
		}
	}

	/** BUG-288: 전체 허용·전체 해제·자동 허용 — 개별 조작과 따로 논다. */
	async function bulk(action: 'allowAll' | 'revokeAll' | 'autoOn' | 'autoOff') {
		confirmBulk = null;
		bulkBusy = true;
		try {
			if (action === 'allowAll') await pluginApi.allowAll();
			else if (action === 'revokeAll') await pluginApi.revokeAll();
			else await pluginApi.setAutoAllow(action === 'autoOn');
			await refreshPlugins();
		} catch (e) {
			showToast(e instanceof Error ? e.message : String(e), 'error');
		} finally {
			bulkBusy = false;
		}
	}
</script>

		<h2>{t('plugins.heading', $locale)}</h2>
		<!-- DEV-383: 길드를 안 열었으면 그 사실만 말한다. 예전엔 "플러그인이
		     없습니다. .guild/plugins/… 에 정의하세요" 와 "조회만 가능합니다"
		     가 함께 떴는데, 그 상황에서는 **둘 다 틀린 말**이다. -->
		{#if pluginStatus?.no_guild}
			<p class="scale-hint">{t('plugins.noGuild', $locale)}</p>
		{:else}
			<!-- DEV-384: 신뢰 중에는 "허용 전에는 안 돕니다" 가 바로 밑의
			     "전부 허용돼 있습니다" 와 정면으로 어긋난다. 위치만 알린다. -->
			<p class="scale-hint">
				{pluginStatus?.auto_allow
					? t('plugins.introAuto', $locale)
					: t('plugins.intro', $locale)}
			</p>
			<!-- DEV-380: 조회는 어디서든. 관리 버튼만 로컬 데스크톱에서 나온다.
			     DEV-383: **답을 받은 뒤에만** 판단한다 — 받기 전에 띄우면
			     로컬 데스크톱에서도 잠깐 스쳐 지나가고, 실패하면 굳는다. -->
			{#if pluginsLoaded && !canManage}
				<p class="scale-hint">{t('plugins.readOnly', $locale)}</p>
			{/if}
		{/if}
		{#if true}
			<!-- DEV-384: 조회 오류와 전달 실패는 **비동기로** 나타난다. 라이브
			     리전이 없으면 스크린리더는 그게 생긴 줄도 모른다. -->
			{#if pluginError}
				<!-- DEV-383: 실패했을 때 다시 시도할 방법이 없으면 탭을 다시
				     누를 생각을 못 한 사람은 그대로 막힌다. -->
				<p class="plugin-error" role="alert">
					{pluginError}
					<button class="btn-plain" onclick={refreshPlugins}
						>{t('plugins.retry', $locale)}</button
					>
				</p>
			{/if}
			{#if canManage && pluginStatus && !pluginStatus.no_guild}
				<!-- BUG-288: 일괄 조작 셋을 **한 자리에.** 예전엔 "전부 허용" 은 목록 맨 아래,
				     해제는 위 안내문에 흩어져 있었고, 둘 다 모드를 켜고 끄는 것이라 켜 둔
				     동안 개별 철회가 막혔다. 이제 전체 허용·해제는 개별 상태를 한 번에 바꿀
				     뿐이고, 앞으로 올 것을 묻지 않는 것은 자동 허용이 따로 맡는다. -->
				<!-- DEV-400 / DEV-394: 더하기와 다시 읽기. 일괄 동의와는 다른 일이라 줄을 나눈다. -->
				<div class="plugin-toolbar">
					<button type="button" class="btn-go" disabled={sourceBusy} onclick={addPluginFolder}
						>{t('plugins.add', $locale)}</button
					>
					<button type="button" class="btn-plain" disabled={reloading} onclick={reload}
						>{t('plugins.reload', $locale)}</button
					>
					<span class="plugin-auto-hint">{t('plugins.reloadHint', $locale)}</span>
				</div>
				<div class="plugin-bulk">
					<button
						class="btn-plain"
						disabled={bulkBusy || pluginStatus.plugins.length === 0}
						onclick={() => (confirmBulk = 'allowAll')}
						>{t('plugins.allowAll', $locale)}</button
					>
					<button
						class="btn-plain"
						disabled={bulkBusy || pluginStatus.plugins.length === 0}
						onclick={() => bulk('revokeAll')}>{t('plugins.revokeAll', $locale)}</button
					>
					<label class="plugin-auto">
						<input
							type="checkbox"
							checked={pluginStatus.auto_allow}
							disabled={bulkBusy}
							onclick={(e) => {
								// 켜는 것은 확인을 거친다 — 체크 표시는 답을 받은 뒤 상태로 그린다.
								e.preventDefault();
								if (pluginStatus?.auto_allow) bulk('autoOff');
								else confirmBulk = 'autoOn';
							}}
						/>
						{t('plugins.autoAllow', $locale)}
					</label>
					<span class="plugin-auto-hint">{t('plugins.autoAllowHint', $locale)}</span>
				</div>
			{/if}
			{#if pluginStatus &&
				!pluginStatus.no_guild &&
				pluginStatus.plugins.length === 0 &&
				pluginStatus.errors.length === 0}
				<p class="scale-hint">{t('plugins.none', $locale)}</p>
			{/if}
			<ul class="plugin-list" aria-busy={pluginBusy.length > 0}>
				{#each pluginStatus?.plugins ?? [] as p (p.name)}
					<li class="plugin" class:pending={!p.granted} class:fresh={fresh === p.name} id={`plugin-${p.name}`}>
						<div class="plugin-head">
							<strong>{p.name}</strong>
						</div>
						<!-- REQ-020: 나머지 줄은 전부 기계가 읽는 값이라, 처음 보는
						     플러그인 앞에서 "이게 무슨 일을 하는가" 를 알려주는 것이
						     하나도 없었다. 정의가 안 적었으면 자리도 안 만든다 —
						     빈 줄은 없는 것보다 나쁘다. -->
						{#if p.description}
							<p class="plugin-desc">{p.description}</p>
						{/if}
						<div class="plugin-meta">
							<span>{t('plugins.scope', $locale)}: {p.scope.join(', ')}</span>
						</div>
						<!-- DEV-403: 한 플러그인이 여러 줄을 갖는다 — 언제 → 무엇을, 적힌 순서대로. -->
						<ol class="plugin-lines" aria-label={t('plugins.lines', $locale)}>
							{#each p.handlers as h, i (i)}
								<li>
									<span class="line-when"
										>{h.stage === 'pre'
											? t('plugins.stagePre', $locale)
											: t('plugins.stagePost', $locale)}
										{h.events.join(' ')}</span
									>
									<span aria-hidden="true">→</span>
									<code class="plugin-target"
										>{h.call
											? `${h.call}()`
											: (h.action ?? `${h.action_kind} ${h.action_target}`)}</code
									>
									{#if h.when.length > 0}
										<span class="line-when">({h.when.join(', ')})</span>
									{/if}
									{#if h.with.length > 0}
										<span class="line-label"
											>{t('plugins.reads', $locale)}: {h.with.join(', ')}</span
										>
									{/if}
									<span class="line-label">{h.label}</span>
								</li>
							{/each}
						</ol>
						<!-- 어디로 나가고 무엇을 띄우는지 — 스크립트는 이 이름으로만 부른다. -->
						{#if p.actions.length > 0}
							<ul class="plugin-dests" aria-label={t('plugins.dests', $locale)}>
								{#each p.actions as a (a.name)}
									<li>
										<code>{a.name}</code>
										<span class="line-label">{a.kind}</span>
										<code class="plugin-target">{a.target}</code>
									</li>
								{/each}
							</ul>
						{/if}
						<!-- DEV-400: 길드 것과 소스 것이 한 목록에 섞인다 — 어디서 왔는지 보여야 한다.
						     소스 것은 길드 밖이라 경로도 함께. -->
						<p class="plugin-datadir">
							{#if p.source}
								{t('plugins.fromSource', $locale).replace('{source}', p.source)}
								{#if p.dir}<code class="plugin-path">{p.dir}</code>{/if}
							{:else}
								{t('plugins.fromGuild', $locale)}
							{/if}
						</p>
						<!-- BUG-279: run 훅은 플러그인 폴더가 아니라 여기에 쓴다.
						     안 알려주면 훅이 만든 파일을 찾을 방법이 없다. -->
						{#if p.data_dir}
							<p class="plugin-datadir">
								{t('plugins.dataDir', $locale)}:
								<!-- 라벨과 경로가 한 줄에 이어 붙는데, 영어에서는 둘이 같은
								     문자 집합이라 "writes files to: /Users/…" 가 한 덩어리로
								     읽힌다(한국어 라벨일 때는 글자가 달라 저절로 갈렸다).
								     글꼴만으로는 부족해서 본문 마크다운의 인라인 코드와
								     **같은 모양**을 준다 — 앱 안에서 "코드/경로" 는 이미
								     그 모양이므로 새 표기를 만들지 않는다. -->
								<code class="plugin-path">{p.data_dir}</code>
							</p>
						{/if}
						<!-- REQ-021: admin 이 요청한 자리 — "해당 플러그인 설명 아래에
						     입력란". 위젯 종류는 정의가 선언한다: 토큰은 텍스트,
						     알림 여부는 체크박스, 주기는 선택상자. -->
						{#if p.inputs.length > 0}
							<div class="plugin-inputs">
								{#each p.inputs as i (i.key)}
									{@const busy = inputBusy.includes(draftKey(p.name, i.key))}
									<div class="plugin-input" class:missing={i.source === 'missing'}>
										{#if i.type === 'checkbox'}
											<label class="pi-check">
												<input
													type="checkbox"
													checked={draftOf(p.name, i) === true}
													disabled={busy || !canManage}
													onchange={(e) =>
														setDraft(p.name, i.key, e.currentTarget.checked)}
												/>
												<span>{i.label}</span>
											</label>
										{:else}
											<label class="pi-label" for={`pi-${p.name}-${i.key}`}>{i.label}</label>
											{#if i.type === 'select'}
												<select
													id={`pi-${p.name}-${i.key}`}
													class="pi-field"
													disabled={busy || !canManage}
													onchange={(e) => setDraft(p.name, i.key, e.currentTarget.value)}
												>
													{#each i.options as o (o.value)}
														<option value={o.value} selected={draftOf(p.name, i) === o.value}
															>{o.label}</option
														>
													{/each}
												</select>
											{:else}
												<input
													id={`pi-${p.name}-${i.key}`}
													class="pi-field"
													type={i.secret ? 'password' : i.type === 'number' ? 'number' : 'text'}
													autocomplete="off"
													placeholder={i.secret && i.has_value
														? t('plugins.valueSet', $locale)
														: ''}
													value={draftOf(p.name, i)}
													disabled={busy || !canManage}
													oninput={(e) => setDraft(p.name, i.key, e.currentTarget.value)}
												/>
											{/if}
										{/if}
										<div class="pi-actions">
											{#if isDirty(p.name, i)}
												<button
													type="button"
													class="btn-go"
													disabled={busy}
													onclick={() => saveInput(p.name, i)}
													aria-label={`${t('plugins.valueSave', $locale)} — ${i.label}`}
													>{t('plugins.valueSave', $locale)}</button
												>
											{:else if i.source === 'stored' && canManage}
												<button
													type="button"
													class="btn-plain"
													disabled={busy}
													onclick={() => clearInput(p.name, i)}
													aria-label={`${t('plugins.valueClear', $locale)} — ${i.label}`}
													>{t('plugins.valueClear', $locale)}</button
												>
											{/if}
										</div>
										<!-- 값이 어디서 왔는지 안 보여주면 "이미 있는데 왜 또 넣지" 가
										     된다. 없는 값은 이 플러그인이 못 도는 이유다. -->
										<p class="pi-help">
											{#if i.source === 'missing'}
												<strong>{t('plugins.valueMissing', $locale)}</strong>
											{:else if i.source === 'env'}
												{t('plugins.valueFromEnv', $locale)}
											{:else if i.source === 'default'}
												{t('plugins.valueFromDefault', $locale)}
											{/if}
											{#if i.help}<span>{i.help}</span>{/if}
										</p>
									</div>
								{/each}
								<p class="pi-note">{t('plugins.valuesPlaintext', $locale)}</p>
							</div>
						{/if}
						<div class="plugin-state">
							{#if !p.granted}
								{t('plugins.pending', $locale)}
							{:else if p.runs_here}
								{t('plugins.running', $locale)}
							{:else}
								{t('plugins.otherScope', $locale)}
							{/if}
						</div>
						{#if p.script_src}
							<!-- DEV-383: 이 화면의 존재 이유가 "무엇에 동의하는지 보여
							     주는 것" 인데, 펼침 상태가 스크린리더에 안 전달되면
							     그 코드는 사실상 안 보이는 것과 같다.

							     BUG-277: 이 아래 버튼 셋의 이름은 **aria-label 로만**
							     구분한다. 스크린리더로 목록을 훑으면 "허용, 허용, 허용"
							     으로만 들려 어느 것인지 알 수 없는데(DEV-383), 그걸
							     고치려고 보이는 텍스트에 이름을 붙였던 것이 잘못이었다
							     — 이름은 같은 항목 세 줄 위에 이미 있다. 고쳐야 했던
							     것은 접근 이름뿐이다. 접근 이름이 보이는 문구를 그대로
							     포함하므로 WCAG 2.5.3(Label in Name)도 지킨다. -->
							<button
								type="button"
								class="link-btn"
								aria-expanded={openScript === p.name}
								aria-controls={`plugin-script-${p.name}`}
								aria-label={`${
									openScript === p.name
										? t('plugins.hideScript', $locale)
										: t('plugins.showScript', $locale)
								} — ${p.name}`}
								onclick={() => (openScript = openScript === p.name ? null : p.name)}
								>{openScript === p.name
									? t('plugins.hideScript', $locale)
									: t('plugins.showScript', $locale)}</button
							>
							{#if openScript === p.name}
								<pre id={`plugin-script-${p.name}`} class="plugin-script">{p.script_src}</pre>
							{/if}
						{/if}
						{#if canManage}
							<div class="plugin-actions">
								<!-- BUG-288: 자동 허용 중에도 개별 철회가 먹는다 — 철회가 판정에서 앞선다. -->
								{#if p.granted}
									<button
										type="button"
										class="btn-plain"
										bind:this={pluginBtns[p.name]}
										disabled={pluginBusy.includes(p.name)}
										aria-label={`${t('plugins.revoke', $locale)} — ${p.name}`}
										onclick={() => togglePlugin(p.name, false)}
										>{t('plugins.revoke', $locale)}</button
									>
								{:else}
									<button
										type="button"
										class="btn-go"
										bind:this={pluginBtns[p.name]}
										disabled={pluginBusy.includes(p.name)}
										aria-label={`${t('plugins.allow', $locale)} — ${p.name}`}
										onclick={() => togglePlugin(p.name, true)}
										>{t('plugins.allow', $locale)}</button
									>
								{/if}
								{#if p.source}
									{@const folder = folderOf(p)}
									{#if folder !== null}
										<button
											type="button"
											class="btn-plain"
											disabled={sourceBusy}
											aria-label={`${t('plugins.stopUsing', $locale)} — ${p.name}`}
											onclick={() => p.source && stopUsing(p.source, folder)}
											>{t('plugins.stopUsing', $locale)}</button
										>
									{/if}
								{/if}
							</div>
						{/if}
					</li>
				{/each}
			</ul>
			<!-- DEV-381: 전달 실패는 대개 설정 문제라 설정 옆에 있는 편이 낫다.
			     토스트로 띄우면 시끄럽고, 안 보여주면 조용히 실패한다. -->
			{#if (pluginStatus?.problems ?? []).length > 0}
				<h3 class="plugin-broken-h">{t('plugins.problems', $locale)}</h3>
				<ul class="plugin-list" role="status" aria-live="polite">
					{#each pluginStatus?.problems ?? [] as p, i (i)}
						<li class="plugin broken"><span>{p}</span></li>
					{/each}
				</ul>
			{/if}
			{#if (pluginStatus?.errors ?? []).length > 0}
				<h3 class="plugin-broken-h">{t('plugins.broken', $locale)}</h3>
				<ul class="plugin-list">
					{#each pluginStatus?.errors ?? [] as [name, why] (name)}
						<li class="plugin broken"><strong>{name}</strong><span>{why}</span></li>
					{/each}
				</ul>
			{/if}
			<!-- DEV-400: 소스 — 이 기계에 등록한 폴더들. 여럿 든 소스에서는 여기서 골라 쓴다.
			     사라진 폴더도 이유와 함께 남긴다(조용히 빠지면 "왜 안 돌지" 가 된다). -->
			{#if canManage && pluginStatus && !pluginStatus.no_guild}
				<h3 class="plugin-broken-h">{t('plugins.sources', $locale)}</h3>
				<p class="scale-hint">{t('plugins.sourcesHint', $locale)}</p>
				{#if sources.length === 0}
					<p class="scale-hint">{t('plugins.sourcesNone', $locale)}</p>
				{:else}
					<ul class="plugin-list">
						{#each sources as src (src.name)}
							<li class="plugin" class:broken-src={src.problem} id={`plugin-source-${src.name}`}>
								<div class="plugin-head">
									<strong>{src.name}</strong>
									<code class="plugin-path">{src.path}</code>
								</div>
								{#if src.problem}
									<p class="plugin-error">{src.problem}</p>
								{/if}
								{#if src.plugins.length > 0}
									<ul class="source-plugins">
										{#each src.plugins as sp (sp.folder)}
											<li>
												<span class="sp-name">{sp.name}</span>
												{#if sp.used}
													<span class="sp-used">{t('plugins.inUse', $locale)}</span>
													<button
														type="button"
														class="btn-plain"
														disabled={sourceBusy}
														aria-label={`${t('plugins.stopUsing', $locale)} — ${sp.name}`}
														onclick={() => stopUsing(src.name, sp.folder)}
														>{t('plugins.stopUsing', $locale)}</button
													>
												{:else}
													<button
														type="button"
														class="btn-go"
														disabled={sourceBusy}
														aria-label={`${t('plugins.use', $locale)} — ${sp.name}`}
														onclick={() => usePlugin(src.name, sp.folder, sp.name)}
														>{t('plugins.use', $locale)}</button
													>
												{/if}
											</li>
										{/each}
									</ul>
								{/if}
								<div class="plugin-actions">
									<button
										type="button"
										class="btn-plain"
										disabled={sourceBusy}
										aria-label={`${t('plugins.removeSource', $locale)} — ${src.name}`}
										onclick={() => (confirmSource = src.name)}
										>{t('plugins.removeSource', $locale)}</button
									>
								</div>
							</li>
						{/each}
					</ul>
				{/if}
			{/if}
		{/if}

<!-- DEV-379 / BUG-288: 둘 다 보지 않은 코드를 이 기계에서 돌리게 한다 — 확인 없이 받지 않는다. -->
<ConfirmDialog
	open={confirmBulk !== null}
	title={confirmBulk === 'autoOn'
		? t('plugins.autoAllow', $locale)
		: t('plugins.allowAll', $locale)}
	message={confirmBulk === 'autoOn'
		? t('plugins.autoAllowConfirm', $locale)
		: t('plugins.allowAllConfirm', $locale)}
	confirmLabel={confirmBulk === 'autoOn'
		? t('plugins.autoAllowOn', $locale)
		: t('plugins.allowAll', $locale)}
	danger
	onconfirm={() => confirmBulk && bulk(confirmBulk)}
	oncancel={() => (confirmBulk = null)}
/>

<!-- DEV-400: 소스 해제는 이 길드만의 일이 아니다 — 그 소스를 쓰던 모든 길드에서 빠진다. -->
<ConfirmDialog
	open={confirmSource !== null}
	title={t('plugins.removeSource', $locale)}
	message={t('plugins.removeSourceConfirm', $locale).replace('{source}', confirmSource ?? '')}
	confirmLabel={t('plugins.removeSource', $locale)}
	danger
	onconfirm={() => confirmSource && removeSource(confirmSource)}
	oncancel={() => (confirmSource = null)}
/>

<style>
	h2 {
		font-size: 1rem;
		color: var(--text);
		margin: 0 0 1rem;
	}
	.scale-hint {
		font-size: 0.75rem;
		color: var(--text-faint);
		margin: 0.5rem 0 0;
	}
	/* DEV-379: 플러그인 목록. 동의 전인 것이 눈에 띄어야 한다 — 조용히 안 도는
	   것이 제일 나쁘다. */
	.plugin-list {
		list-style: none;
		margin: 0.75rem 0;
		padding: 0;
		display: flex;
		flex-direction: column;
		gap: 0.5rem;
		/* DEV-383: 설정의 다른 절은 `.info-grid` 안이라 0.875rem 이다. 여기만
		   `.info-grid` 밖이라 1rem 으로 튀었다. */
		font-size: 0.875rem;
	}
	.plugin {
		border: var(--bw) solid var(--border);
		border-radius: var(--r-md);
		padding: 0.6rem 0.75rem;
		display: flex;
		flex-direction: column;
		gap: 0.3rem;
	}
	.plugin.pending {
		border-color: var(--warning);
	}
	.plugin.broken {
		border-color: var(--danger);
		flex-direction: row;
		/* DEV-383: 전달 실패 메시지에 URL 이 들어가면 줄이 카드를 넘어가 잘렸다 —
		   정작 이유가 URL **뒤에** 오는데 그게 안 보였다. */
		flex-wrap: wrap;
		gap: 0.75rem;
		align-items: baseline;
	}
	.plugin.broken span {
		min-width: 0;
		overflow-wrap: anywhere;
	}
	.plugin-head {
		display: flex;
		gap: 0.5rem;
		align-items: baseline;
		flex-wrap: wrap;
	}
	.plugin-target {
		/* DEV-383: 코드 글꼴 설정을 따른다 — 앱의 모든 mono 표면이 그렇다. */
		font-family: var(--font-mono);
		font-size: 0.8rem;
		color: var(--text-muted);
		word-break: break-all;
	}
	.plugin-desc {
		/* 사람이 읽는 유일한 줄이다 — 목적지·패턴보다 눈에 먼저 들어와야 하므로
		   muted 로 죽이지 않는다. */
		margin: 0;
		font-size: 0.85rem;
		color: var(--text);
		line-height: 1.45;
		/* 500자까지 허용하는데 그 안에 공백 없는 긴 토큰(주소 등)이 있으면 상자를
		   넘는다 — `.plugin-target` 이 같은 이유로 break-all 을 쓴다. 이쪽은 산문
		   이므로 어절을 먼저 지키는 anywhere 를 쓴다. */
		overflow-wrap: anywhere;
	}
	.plugin-path {
		/* MarkdownView 의 `.md :global(code)` 와 같은 값 — 본문에서 경로·코드를
		   보던 모양 그대로다. */
		font-family: var(--font-mono);
		font-size: 0.78rem;
		background: var(--bg-elevated);
		padding: 0.1rem 0.3rem;
		border-radius: var(--r-xs);
		color: var(--text);
		/* 경로에는 공백이 없어 줄바꿈 기회가 없다 — 좁은 화면에서 상자를 넘는다. */
		overflow-wrap: anywhere;
	}
	.plugin-datadir {
		/* `.plugin-meta` 를 재활용했더니 그쪽 `gap: 1rem` 이 **행 간격에도** 걸려,
		   긴 경로가 줄바꿈될 때 라벨과 경로 사이에 1rem 짜리 빈 줄이 생겼다
		   (admin: "엔터가 하나 있는 느낌"). flex 대신 그냥 글로 흘린다 —
		   경로는 라벨 바로 뒤에 이어 붙고 넘칠 때만 다음 줄로 간다. */
		margin: 0;
		font-size: 0.78rem;
		color: var(--text-muted);
		line-height: 1.5;
	}
	.plugin-inputs {
		display: flex;
		flex-direction: column;
		gap: 0.6rem;
		margin: 0.15rem 0 0.1rem;
		padding: 0.6rem 0.7rem;
		background: var(--bg-subtle);
		border-radius: var(--r-sm);
	}
	.plugin-input {
		display: grid;
		/* 첫 칸은 라벨. 최대를 두는 이유는 짧은 라벨들이 제각각 폭을 갖지
		   않게 하려는 것이고, 넘치는 라벨은 위에서 줄바꿈한다. */
		grid-template-columns: minmax(6rem, 11rem) minmax(0, 1fr) auto;
		gap: 0.4rem 0.6rem;
		align-items: center;
	}
	/* 체크박스는 라벨이 위젯에 붙어 있어 첫 칸을 안 쓴다. */
	.plugin-input:has(.pi-check) {
		grid-template-columns: 1fr auto;
	}
	.pi-check {
		display: flex;
		align-items: center;
		gap: 0.45rem;
		font-size: 0.85rem;
	}
	.pi-label {
		font-size: 0.82rem;
		color: var(--text-muted);
		/* 선언에 label 이 없으면 키가 그대로 라벨이 된다
		   (`DISCUSSION_WEBHOOK_TOKEN`). 공백이 없어서 줄바꿈 기회가 없고,
		   그리드 칸을 넘어 입력창 아래로 흘러 글자가 가려졌다. */
		overflow-wrap: anywhere;
		min-width: 0;
	}
	.pi-field {
		min-width: 0;
		font-size: 0.85rem;
		padding: 0.3rem 0.45rem;
		border: var(--bw) solid var(--border);
		border-radius: var(--r-sm);
		background: var(--bg);
		color: var(--text);
	}
	.pi-actions {
		display: flex;
		gap: 0.3rem;
		/* 버튼이 없을 때 칸이 무너지지 않게 — 저장 버튼이 나타났다 사라질 때마다
		   위젯이 좌우로 흔들리면 읽기 어렵다. */
		min-width: 3.5rem;
		justify-content: flex-end;
	}
	.pi-help {
		grid-column: 1 / -1;
		margin: 0;
		font-size: 0.76rem;
		color: var(--text-muted);
		line-height: 1.4;
		overflow-wrap: anywhere;
	}
	.plugin-input.missing .pi-help strong {
		color: var(--warning);
	}
	.pi-note {
		margin: 0;
		font-size: 0.74rem;
		color: var(--text-muted);
	}
	/* DEV-403: 줄 목록과 내보내는 곳. 기계가 읽는 값이라 작게, 줄바꿈은 자유롭게. */
	.plugin-lines,
	.plugin-dests {
		margin: 0;
		padding-left: 1.1rem;
		display: flex;
		flex-direction: column;
		gap: 0.15rem;
		font-size: 0.8rem;
	}
	.plugin-dests {
		list-style: none;
		padding-left: 0;
	}
	.plugin-lines li,
	.plugin-dests li {
		display: flex;
		flex-wrap: wrap;
		gap: 0.2rem 0.45rem;
		align-items: baseline;
		min-width: 0;
	}
	.line-when {
		color: var(--text);
		overflow-wrap: anywhere;
	}
	.line-label {
		color: var(--text-muted);
		font-size: 0.75rem;
	}
	.plugin-meta {
		display: flex;
		gap: 1rem;
		font-size: 0.78rem;
		color: var(--text-muted);
		flex-wrap: wrap;
	}
	.plugin-state {
		font-size: 0.8rem;
	}
	.plugin.pending .plugin-state {
		color: var(--warning);
	}
	.plugin-script {
		margin: 0.25rem 0 0;
		padding: 0.5rem;
		background: var(--bg-subtle);
		border-radius: var(--r-sm);
		font-family: var(--font-mono);
		font-size: 0.78rem;
		overflow-x: auto;
		/* DEV-383: 상한이 없으면 긴 스크립트가 허용 버튼을 화면 밖으로 밀어낸다
		   — 읽고 나서 동의하려면 스크립트 전체를 지나쳐 내려가야 했다.
		   rem 이라 UI 배율을 따라간다. */
		max-height: 14rem;
		overflow-y: auto;
		white-space: pre;
	}
	.plugin-actions {
		display: flex;
		gap: 0.4rem;
		margin-top: 0.15rem;
	}
	/* DEV-382: 앱의 버튼 관용구를 따른다. 처음엔 `ghost` / `primary` 라고 썼는데
	   이 저장소에 그런 클래스가 **없어서** 브라우저 기본 버튼이 그대로 나왔다 —
	   회색 덩어리 하나만 다른 UI 와 따로 놀았다. 같은 페이지의 `.btn-check-upd`
	   와 전역 `--btn-*` 토큰에 맞춘다. */
	.btn-plain,
	.btn-go {
		padding: 0.25rem 0.7rem;
		border-radius: var(--r-md);
		font-size: 0.78rem;
		cursor: pointer;
		transition:
			background 0.15s,
			color 0.15s,
			border-color 0.15s;
	}
	.btn-plain {
		background: transparent;
		border: var(--bw) solid var(--border);
		color: var(--text-muted);
	}
	.btn-plain:hover:not(:disabled) {
		background: var(--bg-subtle);
		color: var(--text);
	}
	.btn-go {
		background: var(--btn-primary-bg);
		border: var(--bw) solid var(--btn-primary-border);
		color: var(--btn-primary-text);
	}
	.btn-go:hover:not(:disabled) {
		background: var(--btn-primary-bg-hover);
		border-color: var(--btn-primary-border-hover);
	}
	.btn-plain:disabled,
	.btn-go:disabled {
		opacity: 0.5;
		cursor: not-allowed;
	}
	.plugin-error {
		color: var(--danger);
		font-size: 0.8rem;
		margin: 0.3rem 0 0;
		display: flex;
		gap: 0.5rem;
		align-items: center;
		flex-wrap: wrap;
	}
	.plugin.fresh {
		/* DEV-400: 방금 더한 것 — 여기서 읽고 허용하라는 표시. */
		box-shadow: 0 0 0 calc(var(--bw) * 2) var(--accent);
	}
	.plugin.broken-src {
		border-color: var(--danger);
	}
	.plugin-toolbar {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 0.4rem 0.75rem;
		margin: 0.75rem 0 0.25rem;
		font-size: 0.85rem;
	}
	.source-plugins {
		list-style: none;
		margin: 0.2rem 0 0;
		padding: 0;
		display: flex;
		flex-direction: column;
		gap: 0.3rem;
	}
	.source-plugins li {
		display: flex;
		align-items: center;
		flex-wrap: wrap;
		gap: 0.5rem;
	}
	.sp-name {
		font-weight: 600;
		overflow-wrap: anywhere;
	}
	.sp-used {
		font-size: 0.78rem;
		color: var(--success);
	}
	.plugin-bulk {
		/* BUG-288: 일괄 조작 셋을 한 줄에 — 좁으면 아래로 떨어진다. */
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 0.4rem 0.75rem;
		margin: 0.5rem 0;
		font-size: 0.85rem;
	}
	.plugin-auto {
		display: inline-flex;
		align-items: center;
		gap: 0.3rem;
		cursor: pointer;
	}
	.plugin-auto-hint {
		font-size: 0.8rem;
		color: var(--text-muted);
	}
	.plugin-broken-h {
		font-size: 0.9rem;
		margin: 1rem 0 0;
	}
	.link-btn {
		align-self: flex-start;
		background: none;
		border: none;
		padding: 0;
		font-size: 0.8rem;
		color: var(--accent);
		cursor: pointer;
		text-decoration: underline;
	}
</style>
