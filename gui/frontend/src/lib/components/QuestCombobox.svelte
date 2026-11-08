<!--
  QuestCombobox — 퀘스트를 검색해서 하나 고른다. 틀(검색 · 키 · 목록)은 `Combobox`.

  - 후보(quests)는 부모(또는 backend candidates API)가 거른 뒤 넘긴다 — 사이클 · 자기 자신 · 이미 부모 있는 것.
  - quest_id · 제목 양쪽으로 부분 일치. 고르면 onselect(questId), 취소는 oncancel().
-->
<script lang="ts">
	import type { Quest } from '$lib/types';
	import Combobox from './Combobox.svelte';
	import { locale, t } from '$lib/stores/locale';
	// DEV-015: status 표시 이름 — 언어 반응.
	import { questStatusLabel } from '$lib/utils/status-label';

	let {
		quests,
		placeholder,
		onselect,
		oncancel
	}: {
		quests: Quest[];
		placeholder?: string;
		onselect: (questId: number) => void;
		oncancel: () => void;
	} = $props();
</script>

<Combobox
	items={quests}
	key={(q) => q.id}
	matches={(q, s) => q.quest_id.toLowerCase().includes(s) || q.title.toLowerCase().includes(s)}
	placeholder={placeholder ?? t('combobox.questPlaceholder', $locale)}
	title={(q) => q.title}
	testid="quest-combobox"
	onselect={(q) => onselect(q.id)}
	{oncancel}
>
	{#snippet before(q: Quest)}
		<span class="pill mono sm" style:--c={q.type_color}>{q.quest_id}</span>
	{/snippet}
	{#snippet after(q: Quest)}
		<span class="pill sm" style:--c={q.status_color}>{questStatusLabel(q, $locale)}</span>
	{/snippet}
</Combobox>
