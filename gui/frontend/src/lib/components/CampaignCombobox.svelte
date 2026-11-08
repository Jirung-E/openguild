<!--
  BUG-030: 캠페인 고르기 — 퀘스트 상세의 "Campaigns" 에서 캠페인을 연결할 때. 틀은 `Combobox`(퀘스트 고르기와 같은 것).

  - 후보(campaigns)는 부모가 거른 뒤 넘긴다(이미 연결된 것 등).
  - campaign_slug · 제목 양쪽으로 부분 일치. 고르면 onselect(slug), 취소는 oncancel().
-->
<script lang="ts">
	import type { Campaign } from '$lib/types';
	import Combobox from './Combobox.svelte';
	import { locale, t } from '$lib/stores/locale';

	let {
		campaigns,
		placeholder,
		onselect,
		oncancel
	}: {
		campaigns: Campaign[];
		placeholder?: string;
		onselect: (slug: string) => void;
		oncancel: () => void;
	} = $props();

	// 캠페인 상태 색 — 퀘스트 상세의 캠페인 칩과 같다.
	const statusColor = (status: string) =>
		status === 'active' ? 'var(--success)' : 'var(--text-muted)';
</script>

<Combobox
	items={campaigns}
	key={(c) => c.id}
	matches={(c, s) => c.campaign_slug.toLowerCase().includes(s) || c.title.toLowerCase().includes(s)}
	placeholder={placeholder ?? t('combobox.campaignPlaceholder', $locale)}
	title={(c) => c.title}
	testid="campaign-combobox"
	onselect={(c) => onselect(c.campaign_slug)}
	{oncancel}
>
	{#snippet before(c: Campaign)}
		<!-- 캠페인은 타입이 없어 slug 칩은 accent 고정. -->
		<span class="pill mono sm" style:--c="var(--accent)">{c.campaign_slug}</span>
	{/snippet}
	{#snippet after(c: Campaign)}
		<span class="pill sm" style:--c={statusColor(c.status)}>{c.status}</span>
	{/snippet}
</Combobox>
