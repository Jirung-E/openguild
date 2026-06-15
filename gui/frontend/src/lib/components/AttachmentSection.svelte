<!--
  DEV-156: 본문 아래 첨부파일 섹션 (Jira 식). quest/campaign 상세에서 본문 아래에
  표시. 이미지는 썸네일, 동영상은 플레이어, 그 외는 파일 칩. '+ 첨부' 로 추가,
  × 로 목록에서 제거(파일 자체는 self-heal 정책상 유지). 진리원은 sidecar —
  add/remove 커맨드가 갱신된 목록을 반환한다. Tauri 전용.
-->
<script lang="ts">
	import { guildFileUrl } from '$lib/utils/banner';
	import { uploadAttachmentFile } from '$lib/utils/editor-attach';
	import { detectEnvironment } from '$lib/api/transport';

	interface Attachment {
		path: string;
		name: string;
	}
	// attachments 는 bindable — 섹션과 편집기 onAttach 가 같은 부모 상태
	// (detail.attachments)를 단일 소스로 갱신.
	let {
		slug,
		scope = 'quest',
		attachments = $bindable([])
	}: { slug: string; scope?: 'quest' | 'campaign'; attachments?: Attachment[] } = $props();

	const list = $derived(attachments ?? []);

	let busy = $state(false);
	let error = $state<string | null>(null);
	// 이미지/동영상 썸네일용 resolved asset URL (path → url).
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

	const addCmd = $derived(
		scope === 'campaign' ? 'add_campaign_attachment' : 'add_quest_attachment'
	);
	const rmCmd = $derived(
		scope === 'campaign' ? 'remove_campaign_attachment' : 'remove_quest_attachment'
	);

	async function pickAndAdd() {
		if (detectEnvironment() !== 'tauri') {
			error = '첨부는 데스크탑 앱에서만 지원됩니다.';
			return;
		}
		const input = document.createElement('input');
		input.type = 'file';
		input.multiple = true;
		input.style.display = 'none';
		input.onchange = async () => {
			const files = Array.from(input.files ?? []);
			input.remove();
			if (files.length === 0) return;
			busy = true;
			error = null;
			try {
				const { invoke } = await import('@tauri-apps/api/core');
				for (const file of files) {
					const { rel, name } = await uploadAttachmentFile(file);
					attachments = await invoke<Attachment[]>(addCmd, { slug, path: rel, name });
				}
			} catch (e) {
				error = e instanceof Error ? e.message : String(e);
			} finally {
				busy = false;
			}
		};
		document.body.appendChild(input);
		input.click();
	}

	async function remove(path: string) {
		busy = true;
		error = null;
		try {
			const { invoke } = await import('@tauri-apps/api/core');
			attachments = await invoke<Attachment[]>(rmCmd, { slug, path });
		} catch (e) {
			error = e instanceof Error ? e.message : String(e);
		} finally {
			busy = false;
		}
	}

	async function openFile(path: string) {
		try {
			const url = urls[path] ?? (await guildFileUrl(path));
			const { openUrl } = await import('@tauri-apps/plugin-opener');
			await openUrl(url);
		} catch {
			/* 무시 */
		}
	}
</script>

<section class="attachments">
	<div class="head">
		<h3>첨부파일 {#if list.length > 0}<span class="count">({list.length})</span>{/if}</h3>
		<button type="button" class="add" onclick={pickAndAdd} disabled={busy}>
			{busy ? '업로드 중…' : '+ 첨부'}
		</button>
	</div>
	{#if error}<p class="err">{error}</p>{/if}
	{#if list.length === 0}
		<p class="empty">첨부 없음. '+ 첨부' 로 이미지·동영상·파일을 추가하세요.</p>
	{:else}
		<ul class="grid">
			{#each list as a (a.path)}
				<li class="item">
					<button type="button" class="thumb" onclick={() => openFile(a.path)} title={a.name}>
						{#if isImage(a.path) && urls[a.path]}
							<img src={urls[a.path]} alt={a.name} />
						{:else if isVideo(a.path) && urls[a.path]}
							<video src={urls[a.path]} muted></video>
						{:else}
							<span class="file-ico">📄</span>
						{/if}
					</button>
					<span class="name" title={a.name}>{a.name}</span>
					<button type="button" class="rm" title="목록에서 제거" onclick={() => remove(a.path)}>×</button>
				</li>
			{/each}
		</ul>
	{/if}
</section>

<style>
	.attachments {
		margin-top: 1rem;
		border-top: 1px solid var(--border);
		padding-top: 0.75rem;
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
	.count {
		color: var(--text-muted);
		font-weight: 400;
	}
	.add {
		font-size: 0.8rem;
		padding: 0.25rem 0.7rem;
		border-radius: 6px;
		border: 1px solid var(--border);
		background: var(--bg-subtle);
		color: var(--text);
		cursor: pointer;
	}
	.add:hover:not(:disabled) {
		background: var(--bg-elevated);
	}
	.add:disabled {
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
	.rm {
		position: absolute;
		top: 0.15rem;
		right: 0.15rem;
		width: 1.2rem;
		height: 1.2rem;
		line-height: 1;
		border: none;
		border-radius: 50%;
		background: color-mix(in srgb, var(--danger) 80%, transparent);
		color: white;
		cursor: pointer;
		font-size: 0.85rem;
	}
</style>
