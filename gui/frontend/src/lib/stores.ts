import { writable } from 'svelte/store';

/**
 * 방금 생성된 퀘스트 ID. Nav 의 New Quest 모달이 set 하고
 * QuestBoard 가 구독해서 해당 노드로 panTo + 펄스 표시 후 null 로 reset.
 */
export const flashQuestId = writable<number | null>(null);
