<!--
  DEV-435: 같은 본문 동시 편집 — 저장이 그사이 남이 한 변경과 부딪혔을 때.

  - text: 같은 줄을 다르게 고친 구간마다 "지금 저장된 것 / 내가 쓴 것" 을 나란히 보여 고르게 한다. 다 고르면 합친 글을
    편집기에 넣는다 — 저장은 사람이 한 번 더 누른다(넣은 글을 보고 고칠 수 있게).
  - renamed / deleted: 저장할 곳이 없다 — 안내만. 새 번호로 열 때는 내 글을 클립보드에 복사해 둔다.
  - stale: 번호로만 안 경우(앱에서는 보통 안 생긴다) — 지금 내용을 보여 준다.

  **어느 경우에도 편집기의 내 글을 지우지 않는다** — 닫으면 그대로 남는다(REQ-024).
-->
<script lang="ts">
	import { modalScrollLock } from '$lib/actions/modal-scroll-lock';
	import { locale, t } from '$lib/stores/locale';
	import {
		assemble,
		type ConflictChoice,
		type ConflictSegment,
		type EditConflict
	} from '$lib/api/editConflict';

	type Props = {
		conflict: EditConflict | null;
		/** 지금 편집기에 있는 내 글 — 새 번호로 옮길 때 복사해 둔다. */
		mine?: string;
		/** 고른 대로 합친 글과, 다음 저장의 "시작할 때 본 것"(= 지금 저장된 본문). */
		onresolve?: (text: string, base: string) => void;
		onclose?: () => void;
		/** renamed — 새 번호로 연다. */
		onopen?: (newId: string) => void;
	};

	let { conflict, mine = '', onresolve, onclose, onopen }: Props = $props();

	const conflicts = $derived(
		(conflict?.segments ?? []).filter(
			(s): s is Extract<ConflictSegment, { kind: 'conflict' }> => s.kind === 'conflict'
		)
	);
	let choice = $state<(ConflictChoice | undefined)[]>([]);
	$effect(() => {
		// 충돌이 새로 오면 고른 것을 비운다.
		void conflict;
		choice = [];
	});
	const allChosen = $derived(
		conflicts.length > 0 && conflicts.every((_, i) => choice[i] !== undefined)
	);

	function pick(i: number, c: ConflictChoice) {
		const next = [...choice];
		next[i] = c;
		choice = next;
	}

	function resolve() {
		if (!conflict || !allChosen) return;
		onresolve?.(assemble(conflict.segments, choice as ConflictChoice[]), conflict.current ?? '');
	}

	async function openNew() {
		if (!conflict?.new_id) return;
		try {
			await navigator.clipboard.writeText(mine);
		} catch {
			// 클립보드를 못 써도 연다 — 편집기 글은 닫기로도 남길 수 있다.
		}
		onopen?.(conflict.new_id);
	}

	/** 같은 구간은 앞뒤 두 줄만 — 어디인지 알 만큼. */
	function context(text: string, where: 'before' | 'after'): string {
		const lines = text.replace(/\n$/, '').split('\n');
		if (lines.length <= 4) return lines.join('\n');
		return where === 'before'
			? '…\n' + lines.slice(-2).join('\n')
			: lines.slice(0, 2).join('\n') + '\n…';
	}

	function onkeydown(e: KeyboardEvent) {
		if (conflict && e.key === 'Escape') {
			e.preventDefault();
			onclose?.();
		}
	}

	const title = $derived(conflict ? t(`editConflict.title.${conflict.reason}`, $locale) : '');
</script>

<svelte:window {onkeydown} />

{#if conflict}
	<div class="ov" use:modalScrollLock role="presentation">
		<div
			class="modal"
			role="alertdialog"
			aria-modal="true"
			aria-labelledby="ec-title"
			tabindex="-1"
		>
			<h3 class="modal-title" id="ec-title">{title}</h3>
			<p class="modal-msg">{conflict.message}</p>

			{#if conflict.reason === 'text'}
				<p class="hint">{t('editConflict.hint', $locale)}</p>
				<div class="segments">
					{#each conflict.segments as seg, si (si)}
						{#if seg.kind === 'same'}
							{@const prevConflict = si > 0}
							{@const nextConflict = si < conflict.segments.length - 1}
							{#if prevConflict || nextConflict}
								<pre class="same">{prevConflict && nextConflict
										? context(seg.text, 'after') + '\n' + context(seg.text, 'before')
										: prevConflict
											? context(seg.text, 'after')
											: context(seg.text, 'before')}</pre>
							{/if}
						{:else}
							{@const ci = conflicts.indexOf(seg)}
							<div class="clash" class:done={choice[ci] !== undefined}>
								<div class="side" class:chosen={choice[ci] === 'current' || choice[ci] === 'both'}>
									<div class="side-label">{t('editConflict.current', $locale)}</div>
									<pre>{seg.current || t('editConflict.empty', $locale)}</pre>
								</div>
								<div class="side" class:chosen={choice[ci] === 'mine' || choice[ci] === 'both'}>
									<div class="side-label">{t('editConflict.mine', $locale)}</div>
									<pre>{seg.mine || t('editConflict.empty', $locale)}</pre>
								</div>
								<div class="pick" role="group" aria-label={t('editConflict.pick', $locale)}>
									<button class:on={choice[ci] === 'current'} onclick={() => pick(ci, 'current')}
										>{t('editConflict.useCurrent', $locale)}</button
									>
									<button class:on={choice[ci] === 'mine'} onclick={() => pick(ci, 'mine')}
										>{t('editConflict.useMine', $locale)}</button
									>
									<button class:on={choice[ci] === 'both'} onclick={() => pick(ci, 'both')}
										>{t('editConflict.useBoth', $locale)}</button
									>
								</div>
							</div>
						{/if}
					{/each}
				</div>
			{:else if conflict.reason === 'stale' && conflict.current != null}
				<div class="side-label">{t('editConflict.current', $locale)}</div>
				<pre class="current-all">{conflict.current}</pre>
			{/if}

			<div class="modal-actions">
				<button class="btn-no" onclick={() => onclose?.()}
					>{t('editConflict.keepEditing', $locale)}</button
				>
				{#if conflict.reason === 'text'}
					<button class="btn-yes" disabled={!allChosen} onclick={resolve}
						>{t('editConflict.apply', $locale)}</button
					>
				{:else if conflict.reason === 'renamed' && conflict.new_id}
					<button class="btn-yes" onclick={openNew}
						>{t('editConflict.openNew', $locale).replace('{id}', conflict.new_id)}</button
					>
				{/if}
			</div>
		</div>
	</div>
{/if}

<style>
	.ov {
		position: fixed;
		inset: 0;
		background: rgba(0, 0, 0, 0.55);
		z-index: 200;
		display: flex;
		align-items: flex-start;
		overflow-y: auto;
		overscroll-behavior: contain;
		justify-content: center;
		padding: 1rem;
	}
	.modal {
		margin: auto;
		background: var(--bg-elevated);
		border: var(--bw) solid var(--border);
		border-radius: var(--r-xl);
		width: 100%;
		max-width: calc(52rem * var(--popup-scale, 1));
		padding: 1.1rem 1.4rem 1.2rem;
		box-shadow: 0 16px 40px rgba(0, 0, 0, 0.55);
		color: var(--text);
	}
	.modal-title {
		margin: 0 0 0.5rem;
		font-size: 1rem;
		font-weight: 600;
		color: var(--text-strong);
	}
	.modal-msg,
	.hint {
		margin: 0 0 0.75rem;
		font-size: 0.875rem;
		white-space: pre-wrap;
		word-break: break-word;
	}
	.hint {
		color: var(--text-muted);
	}
	.segments {
		display: flex;
		flex-direction: column;
		gap: 0.5rem;
		margin-bottom: 1rem;
	}
	pre {
		margin: 0;
		font-size: 0.8125rem;
		white-space: pre-wrap;
		word-break: break-word;
		font-family: inherit;
	}
	.same {
		color: var(--text-muted);
		padding: 0.25rem 0.5rem;
	}
	.clash {
		display: grid;
		grid-template-columns: 1fr 1fr;
		gap: 0.5rem;
		padding: 0.5rem;
		border: var(--bw) solid var(--warning, var(--border));
		border-radius: var(--r-md);
	}
	.clash.done {
		border-color: var(--border);
	}
	.side {
		padding: 0.4rem 0.5rem;
		border-radius: var(--r-md);
		background: var(--bg-subtle);
		opacity: 0.85;
	}
	.side.chosen {
		opacity: 1;
		outline: var(--bw) solid var(--accent, var(--border));
	}
	.side-label {
		font-size: 0.75rem;
		color: var(--text-muted);
		margin-bottom: 0.25rem;
	}
	.pick {
		grid-column: 1 / -1;
		display: flex;
		gap: 0.4rem;
		justify-content: flex-end;
	}
	.pick button {
		padding: 0.25rem 0.7rem;
		background: transparent;
		border: var(--bw) solid var(--border);
		border-radius: var(--r-md);
		color: var(--text);
		font-size: 0.8125rem;
		cursor: pointer;
	}
	.pick button.on {
		background: var(--btn-primary-bg);
		border-color: var(--btn-primary-border);
		color: var(--btn-primary-text);
	}
	.current-all {
		padding: 0.5rem;
		border-radius: var(--r-md);
		background: var(--bg-subtle);
		margin-bottom: 1rem;
		max-height: 20rem;
		overflow: auto;
	}
	.modal-actions {
		display: flex;
		gap: 0.5rem;
		justify-content: flex-end;
	}
	.btn-no {
		padding: 0.4rem 1rem;
		background: transparent;
		border: var(--bw) solid var(--border);
		border-radius: var(--r-md);
		color: var(--text-muted);
		font-size: 0.875rem;
		cursor: pointer;
	}
	.btn-no:hover {
		background: var(--bg-subtle);
	}
	.btn-yes {
		padding: 0.4rem 1.1rem;
		background: var(--btn-primary-bg);
		border: var(--bw) solid var(--btn-primary-border);
		border-radius: var(--r-md);
		color: var(--btn-primary-text);
		font-size: 0.875rem;
		cursor: pointer;
	}
	.btn-yes:disabled {
		opacity: 0.5;
		cursor: default;
	}
	.btn-yes:not(:disabled):hover {
		background: var(--btn-primary-bg-hover);
		border-color: var(--btn-primary-border-hover);
	}
</style>
