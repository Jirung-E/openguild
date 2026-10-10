<!--
  DEV-395: 새 캠페인 — 모달.

  예전엔 `/campaigns/new` 페이지였다([[DEV-011]] 이 캠페인 화면을 한꺼번에 만들 때 그렇게
  잡았고, 퀘스트 생성 모달은 나중에 따로 생겼다). 둘을 다르게 하기로 한 기록은 없다 — 시기가
  달랐을 뿐이다. 입력이 셋(제목·기간·본문)뿐이라 페이지 한 장을 쓸 이유가 없고, 만들다 그만두면
  보던 목록으로 그냥 돌아오는 편이 자연스럽다.

  문구는 페이지 시절 한국어로 박혀 있던 것을 옮기며 i18n 으로 바꿨다.
-->
<script lang="ts">
	import { modalScrollLock } from '$lib/actions/modal-scroll-lock';
	import { modalFocus } from '$lib/actions/modal-focus';
	import { campaignsApi } from '$lib/api/campaigns';
	// DEV-205: 언어 반응 날짜 입력(네이티브 date 대체).
	import DateField from '$lib/components/DateField.svelte';
	// DEV-130 #2: 본문 textarea Tab = 들여쓰기 (설정 반영).
	import { tabInsert } from '$lib/actions/tab-insert';
	import { locale, t } from '$lib/stores/locale';
	import type { Campaign } from '$lib/types';

	let {
		onclose,
		oncreated
	}: {
		onclose: () => void;
		/** 만들어진 캠페인. 안 넘기면 호출자가 알아서 한다(대개 상세로 이동). */
		oncreated?: (c: Campaign) => void;
	} = $props();

	let title = $state('');
	let startedAt = $state('');
	let endedAt = $state('');
	let description = $state('');
	let saving = $state(false);
	let error = $state<string | null>(null);

	async function submit() {
		const name = title.trim();
		if (!name) {
			error = t('ncm.titleRequired', $locale);
			return;
		}
		saving = true;
		error = null;
		try {
			const created = await campaignsApi.create({
				title: name,
				description: description.trim() || null,
				started_at: startedAt || null,
				ended_at: endedAt || null
			});
			// create 가 description 을 안 받는다 — 있으면 한 번 더.
			if (description.trim()) {
				await campaignsApi.update(created.campaign_slug, { description: description.trim() });
			}
			onclose();
			oncreated?.(created);
		} catch (e) {
			error = e instanceof Error ? e.message : t('ncm.createFailed', $locale);
			saving = false;
		}
	}

	function onKeydown(e: KeyboardEvent) {
		if (e.defaultPrevented) return;
		if (e.key === 'Escape' && !saving) onclose();
	}
</script>

<svelte:window onkeydown={onKeydown} />

<div class="overlay" role="dialog" aria-modal="true" use:modalScrollLock>
	<div class="modal-box modal" role="document" use:modalFocus>
		<div class="modal-head">
			<h2 class="modal-title">{t('ncm.newCampaign', $locale)}</h2>
			<button class="close-btn" onclick={onclose} aria-label={t('common.cancel', $locale)}
				>×</button
			>
		</div>

		<form
			onsubmit={(e) => {
				e.preventDefault();
				submit();
			}}
		>
			<label>
				<span class="lab">{t('ncm.title', $locale)}</span>
				<!-- svelte-ignore a11y_autofocus -->
				<input class="input"
					type="text"
					bind:value={title}
					placeholder={t('ncm.titlePlaceholder', $locale)}
					disabled={saving}
					autofocus
					data-autofocus
				/>
			</label>

			<div class="period-row">
				<label>
					<span class="lab">{t('ncm.start', $locale)}</span>
					<DateField bind:value={startedAt} disabled={saving} />
				</label>
				<label>
					<span class="lab">{t('ncm.end', $locale)}</span>
					<DateField bind:value={endedAt} disabled={saving} />
				</label>
			</div>

			<label>
				<span class="lab">{t('ncm.body', $locale)}</span>
				<textarea class="input"
					use:tabInsert
					bind:value={description}
					rows="8"
					disabled={saving}
					placeholder={t('ncm.bodyPlaceholder', $locale)}
				></textarea>
			</label>

			{#if error}<div class="error">{error}</div>{/if}

			<div class="actions">
				<button type="submit" class="btn btn-primary" disabled={saving || !title.trim()}>
					{saving ? t('ncm.creating', $locale) : t('ncm.create', $locale)}
				</button>
				<button type="button" class="btn" onclick={onclose} disabled={saving}>
					{t('common.cancel', $locale)}
				</button>
			</div>
		</form>
	</div>
</div>

<style>
	/* 새 퀘스트 모달과 같은 껍데기 — 긴 내용도 위부터 보이게(BUG-199). */
	.overlay {
		position: fixed;
		inset: 0;
		background: var(--backdrop);
		z-index: 200;
		display: flex;
		align-items: flex-start;
		justify-content: center;
		padding: var(--sp-16);
		overflow-y: auto;
		overscroll-behavior: contain;
	}
	.modal {
		margin: auto;
		width: 100%;
		max-width: calc(35rem * var(--popup-scale, 1));
		animation: modal-in var(--dur-mid) cubic-bezier(0.34, 1.3, 0.64, 1) forwards;
		transform-origin: top center;
	}
	@keyframes modal-in {
		from {
			opacity: 0;
			transform: translateY(-0.5rem) scale(0.98);
		}
		to {
			opacity: 1;
			transform: none;
		}
	}
	.modal-head {
		display: flex;
		align-items: center;
		justify-content: space-between;
		padding: var(--sp-16) var(--sp-16);
		border-bottom: var(--bw) solid var(--border);
	}
	.modal-title {
		margin: 0;
		font-size: var(--fs-lg);
		color: var(--text-strong);
	}
	.close-btn {
		background: none;
		border: none;
		color: var(--text-muted);
		font-size: 1.2rem; /* token-ok: × 글리프 크기 */
		line-height: var(--lh-tight);
		cursor: pointer;
	}
	.close-btn:hover {
		color: var(--text);
	}

	form {
		display: flex;
		flex-direction: column;
		gap: var(--sp-12);
		padding: var(--sp-16);
	}
	label {
		display: flex;
		flex-direction: column;
		gap: var(--sp-4);
	}
	.lab {
		font-size: var(--fs-sm);
		color: var(--text-muted);
	}
	input,
	textarea {
		font-family: inherit;
	}
	textarea {
		font-family: var(--font-mono);
		resize: vertical;
		min-height: 6.5rem;
	}
	.period-row {
		display: grid;
		grid-template-columns: 1fr 1fr;
		gap: var(--sp-12);
	}
	/* 좁은 화면에서는 두 날짜가 한 칸씩 — 나란히 두면 입력기가 눌린다. */
	@media (max-width: 480px) {
		.period-row {
			grid-template-columns: 1fr;
		}
	}
	.error {
		background: color-mix(in srgb, var(--danger) 18%, transparent);
		border: var(--bw) solid var(--danger);
		color: var(--danger);
		padding: var(--sp-6) var(--sp-12);
		border-radius: var(--r-sm);
		font-size: var(--fs-sm);
	}
	.actions {
		display: flex;
		gap: var(--sp-8);
	}
</style>
