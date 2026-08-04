<!--
  DEV-156: 본문 아래 첨부파일 섹션 (Jira 식). quest/campaign 상세에서 본문 아래에
  표시. 이미지는 썸네일, 동영상은 플레이어, 그 외는 파일 칩. '+ 첨부' 로 추가,
  × 로 목록에서 제거(파일 자체는 self-heal 정책상 유지). 진리원은 sidecar —
  add/remove 커맨드가 갱신된 목록을 반환한다.
  BUG-081: 클릭 미리보기/열기(로컬 OS 기본앱 · 원격 새 탭) + 다운로드(전체/개별).
-->
<script lang="ts">
	import Icon from './Icon.svelte';
	import { guildFileUrl } from '$lib/utils/banner';
	import { pickAndUploadAttachments, type AttachQueueItem } from '$lib/utils/editor-attach';
	import { detectEnvironment } from '$lib/api/transport';
	import { getRemoteServerUrl } from '$lib/stores/remoteServer';
	import { api } from '$lib/api/client';
	// DEV-205: 첨부 섹션 i18n.
	import { locale, t } from '$lib/stores/locale';

	interface Attachment {
		path: string;
		name: string;
	}
	let {
		slug,
		scope = 'quest',
		attachments = $bindable([])
	}: {
		slug: string;
		scope?: 'quest' | 'campaign' | 'library';
		attachments?: Attachment[];
	} = $props();

	const list = $derived(attachments ?? []);
	// BUG-097(사용자 보고: "이미지 첨부한게 표시가 안된다"): Tauri + 원격
	// 연결 상태에서도 무조건 "로컬 파일 시스템" 경로(open_guild_file/
	// copy_guild_file invoke, 둘 다 Rust 의 로컬 Store 기준)를 타면 깨진다
	// — 원격 길드의 파일은 로컬 디스크에 없다. "진짜 로컬" 일 때만 invoke,
	// 그 외(브라우저 또는 Tauri+원격)는 URL 기반(새 탭/다운로드 링크)으로.
	const isTauri = detectEnvironment() === 'tauri' && !getRemoteServerUrl();

	let busy = $state(false);
	let error = $state<string | null>(null);
	// DEV-298 → DEV-322: 업로드 대기열 — null 이면 표시 안 함. 고른 파일 전체가
	// 여기 들어오고 각 줄이 자기 상태(대기/진행/완료/실패)를 들고 있다.
	let queue = $state<AttachQueueItem[] | null>(null);
	// DEV-323: 업로드가 도는 동안만 채워지는 취소 손잡이.
	let cancelUpload = $state<(() => void) | null>(null);
	let urls = $state<Record<string, string>>({});

	const IMG = /\.(png|jpe?g|gif|webp|bmp|svg)$/i;
	const VID = /\.(mp4|webm)$/i;
	const isImage = (p: string) => IMG.test(p);
	const isVideo = (p: string) => VID.test(p);

	$effect(() => {
		for (const a of list) {
			if ((isImage(a.path) || isVideo(a.path)) && !urls[a.path]) void resolve(a.path);
		}
	});
	async function resolve(path: string) {
		try {
			urls = { ...urls, [path]: await guildFileUrl(path) };
		} catch {
			/* 해석 실패 — 칩으로만 표시 */
		}
	}

	// DEV-152: quest/campaign 별 첨부 목록 endpoint. api.post/delete 가
	// transport.ts 를 거쳐 Tauri 면 invoke, 브라우저면 HTTP 로 자동 분기.
	// DEV-237: 도서관(library) 도 동일 sidecar 패턴으로 추가.
	const attachPath = $derived(
		scope === 'campaign'
			? `/api/campaigns/${slug}/attachments`
			: scope === 'library'
				? `/api/library/${slug}/attachments`
				: `/api/quests/by/${slug}/attachments`
	);

	// BUG-168: 대용량 첨부 실패 — 로컬 Tauri 에서는 bytes 를 IPC 로 보내지 않고
	// 경로만 넘긴다(pickAndUploadAttachments 가 환경별로 분기). 파일 하나가 끝날
	// 때마다 목록이 갱신되므로 여러 개를 고르면 진행 상황이 눈에 보인다.
	async function pickAndAdd() {
		busy = true;
		error = null;
		try {
			await pickAndUploadAttachments({
				onOne: async ({ rel, name }) => {
					attachments = await api.post<Attachment[]>(attachPath, { path: rel, name });
				},
				onError: (msg) => {
					error = msg;
				},
				// DEV-298: 대용량은 저장이 끝날 때까지 반응이 없어 멈춘 것처럼 보였다.
				// DEV-322: 이제 고른 파일 전체가 목록으로 온다.
				onQueue: (q) => {
					queue = q;
				},
				// DEV-323: 진행 중 + 대기 중 전부 취소.
				onCancelHandle: (fn) => {
					cancelUpload = fn;
				}
			});
		} finally {
			busy = false;
			// 실패가 있으면 pickAndUploadAttachments 가 목록을 남겨둔다 — 여기서
			// 지우면 어떤 파일이 실패했는지 사라지므로 건드리지 않는다.
		}
	}

	async function remove(path: string) {
		busy = true;
		error = null;
		try {
			attachments = await api.delete<Attachment[]>(
				`${attachPath}?path=${encodeURIComponent(path)}`
			);
		} catch (e) {
			error = e instanceof Error ? e.message : String(e);
		} finally {
			busy = false;
		}
	}

	// BUG-081: 클릭 = 미리보기/열기. 로컬은 OS 기본 앱, 원격은 새 탭(브라우저 미리보기).
	async function openFile(path: string) {
		error = null;
		try {
			if (isTauri) {
				const { invoke } = await import('@tauri-apps/api/core');
				await invoke('open_guild_file', { rel: path });
			} else {
				const url = urls[path] ?? (await guildFileUrl(path));
				window.open(url, '_blank', 'noopener');
			}
		} catch (e) {
			error = `${t('attach.openFailed', $locale)}: ${e instanceof Error ? e.message : String(e)}`;
		}
	}

	function browserDownload(url: string, name: string) {
		const a = document.createElement('a');
		a.href = url;
		a.download = name;
		document.body.appendChild(a);
		a.click();
		a.remove();
	}

	// BUG-081: 개별 다운로드. 로컬은 저장 위치 선택 후 복사, 원격은 <a download>.
	async function downloadOne(att: Attachment) {
		error = null;
		try {
			if (isTauri) {
				const { save } = await import('@tauri-apps/plugin-dialog');
				const dest = await save({ defaultPath: att.name });
				if (!dest) return;
				const { invoke } = await import('@tauri-apps/api/core');
				await invoke('copy_guild_file', { rel: att.path, dest });
			} else {
				browserDownload(await guildFileUrl(att.path), att.name);
			}
		} catch (e) {
			error = `${t('attach.downloadFailed', $locale)}: ${e instanceof Error ? e.message : String(e)}`;
		}
	}

	// BUG-081: 전체 다운로드. 로컬은 폴더 선택 후 일괄 복사, 원격은 순차 다운로드.
	async function downloadAll() {
		if (list.length === 0) return;
		error = null;
		if (isTauri) {
			try {
				const { open } = await import('@tauri-apps/plugin-dialog');
				const dir = await open({ directory: true, title: t('attach.saveDir', $locale) });
				if (!dir || typeof dir !== 'string') return;
				const { invoke } = await import('@tauri-apps/api/core');
				busy = true;
				for (const a of list) {
					await invoke('copy_guild_file', { rel: a.path, dest: `${dir}/${a.name}` });
				}
			} catch (e) {
				error = `${t('attach.downloadAllFailed', $locale)}: ${e instanceof Error ? e.message : String(e)}`;
			} finally {
				busy = false;
			}
		} else {
			for (const a of list) {
				browserDownload(await guildFileUrl(a.path), a.name);
				await new Promise((r) => setTimeout(r, 200));
			}
		}
	}
</script>

<section class="attachments">
	<div class="head">
		<h3>
			{t('attach.title', $locale)}
			{#if list.length > 0}<span class="count">({list.length})</span>{/if}
		</h3>
		<div class="head-actions">
			{#if list.length > 0}
				<button
					type="button"
					class="btn"
					onclick={downloadAll}
					disabled={busy}
					title={t('attach.downloadAll', $locale)}
				>
					{t('attach.downloadAllBtn', $locale)}
				</button>
			{/if}
			<button type="button" class="btn" onclick={pickAndAdd} disabled={busy}>
				{busy ? t('attach.processing', $locale) : t('attach.add', $locale)}
			</button>
		</div>
	</div>
	<!-- DEV-298 → DEV-322: 업로드 대기열. 예전엔 현재 파일 하나와 순번만 보여
	     무엇이 남았는지 알 수 없었다. 이제 고른 파일 전체를 상태와 함께 보여준다. -->
	{#if queue && queue.length > 0}
		{#if cancelUpload}
			<div class="upq-head">
				<span>{t('attach.uploading', $locale)}</span>
				<button type="button" class="upq-cancel" onclick={() => cancelUpload?.()}>
					{t('attach.cancelAll', $locale)}
				</button>
			</div>
		{/if}
		<ul class="upq" aria-live="polite">
			{#each queue as it (it.id)}
				{@const pct = it.percent}
				<li
					class="upq-item"
					class:failed={it.status === 'error'}
					class:cancelled={it.status === 'cancelled'}
				>
					<!-- BUG-190: 파일명과 진행 바는 **다른 줄**. 한 줄에 두면 이름이 긴
					     파일에서 바가 밀려 거의 안 보였다(admin 보고). -->
					<div class="up-line">
						<span class="up-name" title={it.name}>{it.name}</span>
						<span class="up-state">
							{it.status === 'pending'
								? t('attach.queued', $locale)
								: it.status === 'done'
									? '✓'
									: it.status === 'error'
										? '✕'
										: it.status === 'cancelled'
											? t('attach.cancelled', $locale)
											: ''}
						</span>
					</div>
					{#if it.status === 'uploading' || it.status === 'done'}
						<!-- DEV-321: %를 알면 결정형 바 + 숫자, 모르면 불확정 바. 브라우저
						     경로는 전송 전 base64 변환 구간이 있어 그동안 '준비 중'. -->
						<div
							class="up-line"
							role="progressbar"
							aria-valuemin={0}
							aria-valuemax={100}
							aria-valuenow={pct == null ? undefined : Math.round(pct)}
							aria-valuetext={pct == null ? t('attach.preparing', $locale) : `${Math.round(pct)}%`}
						>
							<span class="up-bar">
								<span
									class="up-fill"
									class:determinate={pct != null}
									style={pct != null ? `width:${pct}%` : undefined}
								></span>
							</span>
							<span class="up-pct">
								{pct == null
									? it.phase === 'preparing'
										? t('attach.preparing', $locale)
										: ''
									: `${Math.round(pct)}%`}
							</span>
						</div>
					{:else if it.status === 'error' && it.error}
						<div class="up-line"><span class="up-err">{it.error}</span></div>
					{/if}
				</li>
			{/each}
		</ul>
	{/if}
	{#if error}<p class="err">{error}</p>{/if}
	{#if list.length === 0}
		<p class="empty">{t('attach.empty', $locale)}</p>
	{:else}
		<ul class="grid">
			{#each list as a (a.path)}
				<li class="item">
					<button
						type="button"
						class="thumb"
						onclick={() => openFile(a.path)}
						title={t('attach.openPreview', $locale)}
					>
						{#if isImage(a.path) && urls[a.path]}
							<img src={urls[a.path]} alt={a.name} />
						{:else if isVideo(a.path) && urls[a.path]}
							<video src={urls[a.path]} muted></video>
						{:else}
							<span class="file-ico"><Icon name="doc" size={26} /></span>
						{/if}
					</button>
					<button
						type="button"
						class="rm"
						title={t('attach.remove', $locale)}
						aria-label={t('attach.removeAria', $locale)}
						onclick={() => remove(a.path)}>×</button
					>
					<button
						type="button"
						class="dl"
						title={t('attach.download', $locale)}
						aria-label={t('attach.download', $locale)}
						onclick={() => downloadOne(a)}>⤓</button
					>
					<span class="name" title={a.name}>{a.name}</span>
				</li>
			{/each}
		</ul>
	{/if}
</section>

<style>
	.attachments {
		/* 본문과는 좁게(본문 컨테이너가 아래 여백 담당), 구분선 위/아래는 넉넉히. */
		margin-top: 0.4rem;
		padding-bottom: 1.5rem;
		border-bottom: 1px solid var(--border);
		margin-bottom: 1.75rem;
	}
	.head {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 0.5rem;
	}
	.head h3 {
		margin: 0;
		font-size: 0.95rem;
		color: var(--text-strong);
	}
	/* DEV-298 → DEV-322: 업로드 대기열. 진행률을 알 수 없는 구간(경로 기반 복사
	   시작 전 / base64 변환)에서는 불확정 바 — 목적은 "돌고 있음"의 확인. */
	.upq {
		list-style: none;
		margin: 0.6rem 0 0;
		padding: 0;
		display: flex;
		flex-direction: column;
		gap: 0.45rem;
		font-size: 0.8rem;
		color: var(--text-muted);
		/* 많이 고르면 섹션이 통째로 길어지므로 자체 스크롤. */
		max-height: 11rem;
		overflow-y: auto;
	}
	.upq-item {
		/* BUG-190: 이름 줄 / 진행 줄 2단. */
		display: flex;
		flex-direction: column;
		gap: 0.25rem;
	}
	.upq-item.failed .up-name {
		color: var(--danger, #e5534b);
	}
	.upq-item.cancelled .up-name {
		text-decoration: line-through;
		opacity: 0.7;
	}
	.upq-head {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 0.5rem;
		margin-top: 0.6rem;
		font-size: 0.8rem;
		color: var(--text-muted);
	}
	.upq-cancel {
		flex: none;
		background: none;
		border: 1px solid var(--border);
		border-radius: 4px;
		padding: 0.1rem 0.45rem;
		font-size: 0.75rem;
		color: var(--text-muted);
		cursor: pointer;
	}
	.upq-cancel:hover {
		color: var(--text);
		border-color: var(--text-muted);
	}
	.up-state {
		flex: none;
		font-variant-numeric: tabular-nums;
	}
	.up-err {
		color: var(--danger, #e5534b);
		overflow-wrap: anywhere;
	}
	.up-line {
		display: flex;
		align-items: center;
		gap: 0.5rem;
	}
	.up-name {
		/* 이름 줄을 통째로 쓰되 넘치면 말줄임 — 바를 밀어내지 않는다. */
		flex: 1;
		min-width: 0;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.up-bar {
		flex: 1;
		min-width: 4rem;
		height: 3px;
		border-radius: 2px;
		background: var(--bg-subtle);
		overflow: hidden;
	}
	.up-fill {
		display: block;
		width: 35%;
		height: 100%;
		border-radius: 2px;
		background: var(--accent);
		animation: up-slide 1.1s ease-in-out infinite;
	}
	/* DEV-321: 실제 %를 아는 동안은 흐르는 애니메이션 대신 채워지는 바.
	   너비는 인라인 style 로 매 진행마다 갱신된다. */
	.up-fill.determinate {
		width: 0;
		animation: none;
		transition: width 0.15s linear;
	}
	.up-pct {
		flex: 0 0 auto;
		min-width: 2.5rem;
		text-align: right;
		font-variant-numeric: tabular-nums;
		color: var(--text-muted);
	}
	@keyframes up-slide {
		0% {
			transform: translateX(-100%);
		}
		100% {
			transform: translateX(300%);
		}
	}
	/* 모션 축소 선호 시 애니메이션 대신 정적 바. */
	@media (prefers-reduced-motion: reduce) {
		.up-fill {
			animation: none;
			width: 100%;
			opacity: 0.6;
		}
	}
	.count {
		color: var(--text-muted);
		font-weight: 400;
	}
	.head-actions {
		display: flex;
		gap: 0.4rem;
	}
	.btn {
		font-size: 0.8rem;
		padding: 0.25rem 0.7rem;
		border-radius: 6px;
		border: 1px solid var(--border);
		background: var(--bg-subtle);
		color: var(--text);
		cursor: pointer;
		white-space: nowrap;
	}
	.btn:hover:not(:disabled) {
		background: var(--bg-elevated);
	}
	.btn:disabled {
		opacity: 0.5;
		cursor: not-allowed;
	}
	.err {
		color: var(--danger);
		font-size: 0.825rem;
		margin: 0.4rem 0 0;
	}
	.empty {
		color: var(--text-muted);
		font-size: 0.825rem;
		margin: 0.4rem 0 0;
	}
	.grid {
		list-style: none;
		margin: 0.6rem 0 0;
		padding: 0;
		display: grid;
		grid-template-columns: repeat(auto-fill, minmax(7rem, 1fr));
		gap: 0.6rem;
	}
	.item {
		position: relative;
		display: flex;
		flex-direction: column;
		gap: 0.25rem;
		border: 1px solid var(--border);
		border-radius: 8px;
		padding: 0.4rem;
		background: var(--bg-elevated);
	}
	.thumb {
		display: flex;
		align-items: center;
		justify-content: center;
		height: 5rem;
		border: none;
		background: var(--bg-subtle);
		border-radius: 6px;
		cursor: pointer;
		overflow: hidden;
		padding: 0;
	}
	.thumb img,
	.thumb video {
		width: 100%;
		height: 100%;
		object-fit: cover;
	}
	.file-ico {
		font-size: 1.8rem;
	}
	.name {
		font-size: 0.75rem;
		color: var(--text);
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	/* BUG-081 후속: 다운로드 버튼을 × 아래 우상단 오버레이로 — 제목(name) 안 가림. */
	.dl {
		position: absolute;
		top: 1.65rem;
		right: 0.2rem;
		display: flex;
		align-items: center;
		justify-content: center;
		width: 1.3rem;
		height: 1.3rem;
		padding: 0;
		border: none;
		border-radius: 50%;
		background: color-mix(in srgb, var(--accent) 85%, transparent);
		color: white;
		cursor: pointer;
		font-size: 0.9rem;
		line-height: 1;
	}
	.dl:hover {
		background: var(--accent);
	}
	.rm {
		position: absolute;
		top: 0.2rem;
		right: 0.2rem;
		display: flex;
		align-items: center;
		justify-content: center;
		width: 1.3rem;
		height: 1.3rem;
		padding: 0;
		border: none;
		border-radius: 50%;
		background: color-mix(in srgb, var(--danger) 85%, transparent);
		color: white;
		cursor: pointer;
		font-size: 0.95rem;
		line-height: 1;
	}
</style>
