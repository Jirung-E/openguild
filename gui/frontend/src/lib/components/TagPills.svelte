<!--
  DEV-243: 자유 태그 표시 + 인라인 편집 — 도서관 문서 / 길드 규칙에서 공용.
  quest 상세 페이지(quests/[id]/+page.svelte)의 태그 섹션과 동일 UX/마크업을
  재사용 가능한 컴포넌트로 추출. quest 자체는 이미 있던 inline 구현을 그대로
  둬(리스크 최소화) 이 컴포넌트는 library/rules 전용으로 시작.
-->
<script lang="ts">
	import type { QuestTagDef } from '$lib/types';
	// DEV-205(2차): i18n.
	import { locale, t } from '$lib/stores/locale';

	let {
		tags,
		tagDefs,
		editable = true,
		onSetTags
	}: {
		tags: string[];
		tagDefs: QuestTagDef[];
		editable?: boolean;
		onSetTags: (tags: string[]) => void | Promise<void>;
	} = $props();

	const tagDefMap = $derived(new Map(tagDefs.map((d) => [d.slug, d])));

	function tagStyle(tag: string): string {
		const d = tagDefMap.get(tag);
		if (!d || !d.color) return '';
		const c = d.color.trim();
		const hex = c.startsWith('#') ? c.slice(1) : c;
		if (!/^[0-9a-fA-F]{6}$/.test(hex)) return `color: ${c}`;
		const r = parseInt(hex.slice(0, 2), 16);
		const g = parseInt(hex.slice(2, 4), 16);
		const b = parseInt(hex.slice(4, 6), 16);
		return `background: rgba(${r},${g},${b},0.12); border-color: rgba(${r},${g},${b},0.4); color: ${c};`;
	}
	function tagTitle(tag: string): string {
		return tagDefMap.get(tag)?.description || tag;
	}

	let tagInputOpen = $state(false);
	let newTagText = $state('');

	async function addTagFromInput(e: Event) {
		e.preventDefault();
		const tokens = newTagText
			.split(/\s+/)
			.map((s) => s.trim())
			.filter((s) => s.length > 0);
		if (tokens.length === 0) return;
		const merged = [...tags];
		for (const tag of tokens) {
			if (!merged.includes(tag)) merged.push(tag);
		}
		await onSetTags(merged);
		newTagText = '';
		tagInputOpen = false;
	}
	async function removeTag(tag: string) {
		await onSetTags(tags.filter((x) => x !== tag));
	}
</script>

<section>
	<div class="section-head">
		<h2 class="section-title tag-label">Tags</h2>
		{#if editable}
			<button class="sec-add-btn" onclick={() => (tagInputOpen = !tagInputOpen)}>
				{tagInputOpen ? t('tagPills.cancel', $locale) : t('tagPills.add', $locale)}
			</button>
		{/if}
	</div>
	{#if tags.length > 0}
		<ul class="tag-pills">
			{#each tags as tag (tag)}
				<li>
					<span class="tag-pill" style={tagStyle(tag)} title={tagTitle(tag)}>
						{tag}
						{#if editable}
							<button
								class="tag-rm"
								title={t('tagPills.removeTitle', $locale)}
								onclick={() => removeTag(tag)}
								aria-label={`${tag} ${t('tagPills.removeTitle', $locale)}`}>×</button
							>
						{/if}
					</span>
				</li>
			{/each}
		</ul>
	{:else if !tagInputOpen}
		<p class="no-desc">{t('tagPills.none', $locale)}</p>
	{/if}
	{#if tagInputOpen && editable}
		<form class="tag-add-form" onsubmit={addTagFromInput}>
			<input
				type="text"
				bind:value={newTagText}
				placeholder={t('tagPills.newPlaceholder', $locale)}
				aria-label={t('tagPills.newAria', $locale)}
			/>
			<button type="submit" disabled={!newTagText.trim()}>{t('tagPills.add', $locale)}</button>
		</form>
	{/if}
</section>

<style>
	section {
		margin-top: 1rem;
		margin-bottom: 1.5rem;
	}
	.section-head {
		display: flex;
		align-items: center;
		gap: 0.75rem;
		margin-bottom: 0.5rem;
	}
	.section-title {
		font-size: 0.8rem;
		font-weight: 600;
		color: var(--text-muted);
		text-transform: uppercase;
		letter-spacing: 0.05em;
		margin: 0;
	}
	.section-title.tag-label {
		color: var(--warning);
	}
	.no-desc {
		color: var(--text-faint);
		font-size: 0.9rem;
		margin: 0;
	}
	.tag-pills {
		list-style: none;
		padding: 0;
		margin: 0;
		display: flex;
		flex-wrap: wrap;
		gap: 0.35rem;
	}
	.tag-pill {
		display: inline-flex;
		align-items: center;
		gap: 0.25rem;
		padding: 0.15rem 0.6rem;
		background: color-mix(in srgb, var(--warning) 12%, transparent);
		border: var(--bw) solid color-mix(in srgb, var(--warning) 40%, transparent);
		border-radius: var(--r-pill);
		font-size: 0.75rem;
		color: var(--warning);
		font-family: var(--font-mono);
		letter-spacing: 0.02em;
	}
	.tag-rm {
		border: none;
		background: none;
		color: var(--text-muted);
		cursor: pointer;
		font-size: 1rem;
		line-height: 1;
		padding: 0 0 0 0.125rem;
	}
	.tag-rm:hover {
		color: var(--danger);
	}
	.tag-add-form {
		display: flex;
		gap: 0.4rem;
		margin-top: 0.5rem;
	}
	.tag-add-form input {
		flex: 1;
		padding: 0.3rem 0.6rem;
		background: var(--bg);
		border: var(--bw) solid var(--border);
		border-radius: var(--r-md);
		color: var(--text);
		font-size: 0.85rem;
	}
	.tag-add-form button {
		padding: 0.3rem 0.85rem;
		background: var(--bg-subtle);
		border: var(--bw) solid var(--border);
		border-radius: var(--r-md);
		color: var(--text);
		font-size: 0.8rem;
		cursor: pointer;
	}
	.tag-add-form button:disabled {
		opacity: 0.4;
		cursor: not-allowed;
	}
	.tag-add-form button:hover:not(:disabled) {
		background: var(--border);
	}
	.sec-add-btn {
		padding: 0.15rem 0.6rem;
		border: var(--bw) solid var(--border);
		border-radius: var(--r-sm);
		background: transparent;
		color: var(--text-muted);
		font-size: 0.72rem;
		cursor: pointer;
	}
	.sec-add-btn:hover {
		background: var(--bg-subtle);
		color: var(--text);
	}
</style>
