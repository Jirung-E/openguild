<script lang="ts">
	import { questsApi } from '$lib/api/quests';
	import { metaApi } from '$lib/api/meta';
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
	let openStatusId = $state(0); // sort_order 가 가장 작은 상태 (신규 퀘스트는 항상 이 상태로)
	let openStatusLabel = $state('');
	let loading = $state(true);

	let typeId = $state(0);
	let title = $state('');
	let description = $state('');
	let urgency = $state(3);

	let saving = $state(false);
	let saveError = $state<string | null>(null);

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
				openStatusId = sorted[0].id;
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
		if (!typeId || !openStatusId) { saveError = '타입을 선택해주세요.'; return; }
		saving = true;
		saveError = null;
		try {
			const quest = await questsApi.create({
				quest_type_id: typeId,
				title: title.trim(),
				description: description.trim() || undefined,
				status_id: openStatusId,
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
</style>
