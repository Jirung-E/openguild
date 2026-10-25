<!--
  DEV-453: [보드 | 목록] — 퀘스트 보드와 목록은 한 메뉴이고 여기서 고른다. 두 보기 모두 오른쪽 위
  '+ 새 퀘스트' 바로 왼쪽, 같은 자리에 둔다(보기를 바꿔도 손이 같은 곳에 간다).
-->
<script lang="ts">
	import { goto } from '$app/navigation';
	import { locale, t } from '$lib/stores/locale';
	import { saveQuestView, type QuestView } from '$lib/utils/quest-view';

	let { current }: { current: QuestView } = $props();

	function pick(view: QuestView) {
		saveQuestView(view);
		if (view !== current) goto(`/?view=${view}`);
	}
</script>

<div class="seg sm" role="radiogroup" aria-label={t('nav.questViewAria', $locale)}>
	<button
		type="button"
		role="radio"
		aria-checked={current === 'board'}
		class:on={current === 'board'}
		onclick={() => pick('board')}
		data-testid="quest-view-board">{t('nav.viewBoard', $locale)}</button
	>
	<button
		type="button"
		role="radio"
		aria-checked={current === 'list'}
		class:on={current === 'list'}
		onclick={() => pick('list')}
		data-testid="quest-view-list">{t('nav.viewList', $locale)}</button
	>
</div>
