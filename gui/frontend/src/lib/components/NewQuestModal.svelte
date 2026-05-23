<script lang="ts">
	import { questsApi } from '$lib/api/quests';
	import { metaApi } from '$lib/api/meta';
	import { adminApi } from '$lib/api/admin';
	import { onMount } from 'svelte';
	import { URGENCY_LABEL, type Quest, type QuestType, type QuestStatus } from '$lib/types';

	let {
		onclose,
		parentQuestId,
		oncreated
	}: {
		onclose: () => void;
		parentQuestId?: number;
		// 생성 성공 시 호출. 호출자가 뒤처리(navigate / reload / panTo 등) 결정.
		oncreated?: (quest: Quest) => void;
	} = $props();

	let types = $state<QuestType[]>([]);
	// DEV-048: API 가 slug 전용. 표시용 라벨만 별도 보관.
	let openStatusSlug = $state('');
	let openStatusLabel = $state('');
	let loading = $state(true);

	let typeId = $state(0);
	let title = $state('');
	let description = $state('');
	let urgency = $state(3);

	let saving = $state(false);
	let saveError = $state<string | null>(null);

	// DEV-014 후속: meta (type / status) 가 비어있을 때의 empty-state.
	// 0개면 quest 생성 자체가 불가 → 입력 폼 대신 안내 + 액션.
	let creatingDefaultStatus = $state(false);
	let bootstrapError = $state<string | null>(null);

	async function bootstrapDefaultStatus() {
		creatingDefaultStatus = true;
		bootstrapError = null;
		try {
			await adminApi.createStatus({
				name_en: 'Open',
				name_ko: '게시됨',
				color: '#8B95A1'
			});
			// 다시 fetch.
			const s = await metaApi.getQuestStatuses();
			const sorted = [...s].sort((a, b) => a.sort_order - b.sort_order);
			if (sorted.length > 0) {
				openStatusSlug = sorted[0].slug;
				openStatusLabel = sorted[0].name_en;
			}
			// 결과적으로 statuses.length > 0 → 폼이 표시됨 ($derived).
			queueMicrotask(() => titleInput?.focus());
		} catch (e) {
			bootstrapError = e instanceof Error ? e.message : String(e);
		} finally {
			creatingDefaultStatus = false;
		}
	}

	// 제목 input 자동 focus — autofocus 속성 대신 명시적 .focus() 호출
	// (a11y_autofocus 경고 회피 + 같은 UX).
	let titleInput: HTMLInputElement | undefined = $state();

	onMount(async () => {
		try {
			const [t, s] = await Promise.all([metaApi.getQuestTypes(), metaApi.getQuestStatuses()]);
			types = t;
			if (types.length > 0) typeId = types[0].id;
			// 신규 퀘스트 상태: sort_order 최소값. 길드마스터가 첫 상태를 어떻게
			// 이름 짓든 그게 "신규 진입점" 역할.
			const sorted = [...s].sort((a: QuestStatus, b: QuestStatus) => a.sort_order - b.sort_order);
			if (sorted.length > 0) {
				openStatusSlug = sorted[0].slug;
				openStatusLabel = sorted[0].name_en;
			}
		} finally {
			loading = false;
			// Loading 끝나고 input 이 DOM 에 마운트되면 focus.
			queueMicrotask(() => titleInput?.focus());
		}
	});

	async function create() {
		if (!title.trim()) { saveError = '제목을 입력해주세요.'; return; }
		// DEV-014 후속: 검증 메시지 분리 — 이전엔 두 조건이 한 메시지("타입을 선택")
		// 로 묶여서 status 가 0개인데 type 만 있는 경우에도 오해 메시지가 나왔음.
		if (!typeId) { saveError = '타입을 선택해주세요.'; return; }
		if (!openStatusSlug) {
			saveError = '상태가 없습니다. 먼저 상태를 추가하세요.';
			return;
		}
		saving = true;
		saveError = null;
		try {
			const quest = await questsApi.create({
				quest_type_id: typeId,
				title: title.trim(),
				description: description.trim() || undefined,
				status_slug: openStatusSlug,
				urgency,
				parent_quest_id: parentQuestId
			});
			onclose();
			oncreated?.(quest);
		} catch (e) {
			saveError = e instanceof Error ? e.message : 'create failed';
		} finally {
			saving = false;
		}
	}

	function onKeydown(e: KeyboardEvent) {
		if (e.key === 'Escape') onclose();
	}
</script>

<svelte:window onkeydown={onKeydown} />

<!-- 배경 오버레이 -->
<div class="overlay" role="dialog" aria-modal="true">
	<div class="modal" role="document">
		<div class="modal-head">
			<h2 class="modal-title">{parentQuestId ? 'New Sub-Quest' : 'New Quest'}</h2>
			<button class="close-btn" onclick={onclose} aria-label="닫기">×</button>
		</div>

		{#if loading}
			<div class="loading">Loading…</div>
		{:else if types.length === 0}
			<!-- DEV-014 후속: type 0개 — admin 으로 안내 (prefix 정책 결정이 필요해
			     여기서 자동 생성 안 함). -->
			<div class="empty-state">
				<p class="empty-title">퀘스트 타입이 없습니다</p>
				<p class="empty-msg">
					Quest type (DEV / BUG / REQ 같은 prefix) 이 하나도 정의되어 있지 않아
					새 퀘스트를 만들 수 없습니다. 먼저 Admin 페이지에서 type 을 추가하세요.
				</p>
				<div class="form-actions">
					<a class="btn-create" href="/admin" onclick={onclose}>Admin 으로 가기</a>
					<button class="btn-cancel" onclick={onclose}>닫기</button>
				</div>
			</div>
		{:else if !openStatusSlug}
			<!-- DEV-014 후속: status 0개 — "기본 Open 만들고 계속" 한 번에 처리. -->
			<div class="empty-state">
				<p class="empty-title">퀘스트 상태가 없습니다</p>
				<p class="empty-msg">
					Quest status 가 하나도 정의되어 있지 않아 새 퀘스트를 만들 수 없습니다.
					기본 <strong>'Open'</strong> (게시됨, 회색) 을 만들고 계속할까요?
					필요하면 그 뒤 Admin 페이지에서 색 / 이름을 바꾸거나 다른 상태를
					추가할 수 있습니다.
				</p>
				{#if bootstrapError}
					<p class="save-error">{bootstrapError}</p>
				{/if}
				<div class="form-actions">
					<button
						class="btn-create"
						onclick={bootstrapDefaultStatus}
						disabled={creatingDefaultStatus}
					>
						{creatingDefaultStatus ? '추가 중…' : "기본 'Open' 추가하고 계속"}
					</button>
					<a class="btn-cancel" href="/admin" onclick={onclose}>Admin 으로 가기</a>
					<button class="btn-cancel" onclick={onclose}>닫기</button>
				</div>
			</div>
		{:else}
			<div class="form">
				<div class="field-row">
					<div class="field">
						<label class="field-label">
							<span>타입</span>
							<select class="sel" bind:value={typeId}>
								{#each types as t}
									<option value={t.id} style:color={t.color}>{t.prefix}</option>
								{/each}
							</select>
						</label>
					</div>
					<div class="field" style="flex:1">
						<label class="field-label">
							<span>긴급도</span>
							<select class="sel" bind:value={urgency}>
								{#each [1, 2, 3, 4] as u}
									<option value={u}>{URGENCY_LABEL[u]}</option>
								{/each}
							</select>
						</label>
					</div>
					<div class="field" style="flex:1">
						<span class="field-label">상태</span>
						<span class="status-fixed" data-testid="new-quest-status">{openStatusLabel}</span>
					</div>
				</div>

				<div class="field">
					<label class="field-label">
						<span>제목 *</span>
						<input
							bind:this={titleInput}
							class="inp"
							type="text"
							placeholder="퀘스트 제목을 입력하세요"
							bind:value={title}
						/>
					</label>
				</div>

				<div class="field">
					<label class="field-label">
						<span>설명 (선택)</span>
						<textarea
							class="ta"
							rows="5"
							placeholder="Markdown 형식으로 작성할 수 있습니다"
							bind:value={description}
						></textarea>
					</label>
				</div>

				{#if saveError}
					<p class="save-error">{saveError}</p>
				{/if}

				<div class="form-actions">
					<button class="btn-create" onclick={create} disabled={saving || !title.trim()}>
						{saving ? '생성 중…' : '퀘스트 생성'}
					</button>
					<button class="btn-cancel" onclick={onclose} disabled={saving}>취소</button>
				</div>
			</div>
		{/if}
	</div>
</div>

<style>
	.overlay {
		position: fixed;
		inset: 0;
		background: rgba(0, 0, 0, 0.65);
		z-index: 200;
		display: flex;
		align-items: center;
		justify-content: center;
		padding: 1rem;
	}

	.modal {
		background: #161b22;
		border: 1px solid #30363d;
		border-radius: 12px;
		width: 100%;
		max-width: 560px;
		box-shadow: 0 16px 48px rgba(0, 0, 0, 0.6);
		animation: modal-in 0.18s cubic-bezier(0.34, 1.3, 0.64, 1) forwards;
		transform-origin: top center;
	}
	@keyframes modal-in {
		from { opacity: 0; transform: scale(0.9) translateY(-10px); }
		to   { opacity: 1; transform: scale(1) translateY(0); }
	}

	.modal-head {
		display: flex;
		align-items: center;
		justify-content: space-between;
		padding: 1rem 1.25rem 0.75rem;
		border-bottom: 1px solid #21262d;
	}
	.modal-title {
		margin: 0;
		font-size: 1rem;
		font-weight: 600;
		color: #e6edf3;
	}
	.close-btn {
		background: none;
		border: none;
		color: #484f58;
		font-size: 1.3rem;
		line-height: 1;
		cursor: pointer;
		padding: 0 4px;
		transition: color 0.1s;
	}
	.close-btn:hover { color: #c9d1d9; }

	.loading {
		padding: 2rem;
		text-align: center;
		color: #484f58;
		font-size: 0.9rem;
	}

	.form {
		padding: 1rem 1.25rem 1.25rem;
		display: flex;
		flex-direction: column;
		gap: 0.85rem;
	}

	.field-row {
		display: flex;
		gap: 0.75rem;
		align-items: flex-end;
	}
	.field {
		display: flex;
		flex-direction: column;
		gap: 0.3rem;
	}
	/* BUG-010: uppercase / letter-spacing 은 라벨 텍스트 span 에만 적용 —
	   label 전체에 두면 자식 input / textarea / select 까지 대문자로 표시됨. */
	.field-label {
		font-size: 0.72rem;
		font-weight: 600;
		color: #8b949e;
	}
	.field-label > span:first-child,
	span.field-label {
		text-transform: uppercase;
		letter-spacing: 0.05em;
	}

	.sel {
		padding: 0.4rem 0.6rem;
		background: #0d1117;
		border: 1px solid #30363d;
		border-radius: 6px;
		color: #c9d1d9;
		font-size: 0.875rem;
		outline: none;
		min-width: 80px;
	}
	.sel:focus { border-color: #58a6ff; }

	.status-fixed {
		display: inline-flex;
		align-items: center;
		padding: 0.4rem 0.6rem;
		background: #0d1117;
		border: 1px solid #21262d;
		border-radius: 6px;
		color: #8b949e;
		font-size: 0.875rem;
		min-width: 80px;
		min-height: calc(0.875rem + 0.8rem + 2px);
		box-sizing: border-box;
	}

	.inp {
		padding: 0.5rem 0.75rem;
		background: #0d1117;
		border: 1px solid #30363d;
		border-radius: 6px;
		color: #e6edf3;
		font-size: 0.9rem;
		outline: none;
		width: 100%;
		box-sizing: border-box;
	}
	.inp:focus { border-color: #58a6ff; }

	.ta {
		padding: 0.5rem 0.75rem;
		background: #0d1117;
		border: 1px solid #30363d;
		border-radius: 6px;
		color: #c9d1d9;
		font-size: 0.85rem;
		outline: none;
		width: 100%;
		box-sizing: border-box;
		resize: vertical;
		font-family: 'SFMono-Regular', Consolas, monospace;
		line-height: 1.5;
	}
	.ta:focus { border-color: #58a6ff; }

	.save-error {
		color: #e94f4f;
		font-size: 0.8rem;
		margin: 0;
	}

	.form-actions {
		display: flex;
		gap: 0.5rem;
		margin-top: 0.25rem;
	}
	.btn-create {
		padding: 0.45rem 1.25rem;
		background: #238636;
		border: 1px solid #2ea043;
		border-radius: 6px;
		color: #fff;
		font-size: 0.875rem;
		cursor: pointer;
		transition: background 0.1s;
	}
	.btn-create:hover:not(:disabled) { background: #2ea043; }
	.btn-create:disabled { opacity: 0.5; cursor: default; }
	.btn-cancel {
		padding: 0.45rem 1rem;
		background: transparent;
		border: 1px solid #30363d;
		border-radius: 6px;
		color: #8b949e;
		font-size: 0.875rem;
		cursor: pointer;
	}
	.btn-cancel:hover:not(:disabled) { background: #21262d; }

	/* DEV-014 후속: empty-state — type / status 가 0개일 때 폼 대신 안내. */
	.empty-state {
		padding: 1.5rem 0.25rem 0.5rem;
		display: flex;
		flex-direction: column;
		gap: 0.85rem;
	}
	.empty-title {
		margin: 0;
		font-size: 1.05rem;
		font-weight: 600;
		color: #e6edf3;
	}
	.empty-msg {
		margin: 0;
		font-size: 0.9rem;
		line-height: 1.5;
		color: #c9d1d9;
	}
	.empty-msg strong { color: #e6edf3; }
	/* href 인 .btn-create / .btn-cancel 도 동일 패딩. */
	a.btn-create,
	a.btn-cancel {
		text-decoration: none;
		display: inline-flex;
		align-items: center;
	}
</style>
