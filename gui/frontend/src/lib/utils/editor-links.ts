/**
 * DEV-140: 본문 편집기 cross-link 자동완성 — CodeMirror 확장.
 *
 * 사용자 결정: 위키문법 `[[DEV-033]]` 를 직접 외우지 않아도 되도록, 편집기에서
 * 추적번호(`XXX-NNN`) 를 타이핑하면 자동완성으로 "🔗 링크 걸기" 후보가 떠서,
 * 선택하면 그 자리에서 `[[DEV-033]]` 로 변환된다. 실재하는 ID 면 제목을 detail
 * 로 보여주고, 미존재여도 링크는 걸 수 있다 (렌더 시 빨강).
 *
 * 이미 `[[...]]` 로 감싼 토큰은 후보를 띄우지 않는다 (이중 래핑 방지).
 */

import {
	autocompletion,
	type Completion,
	type CompletionContext,
	type CompletionResult
} from '@codemirror/autocomplete';
import { markdownLanguage } from '@codemirror/lang-markdown';
import type { Extension } from '@codemirror/state';
import { questIndex, loadQuestIndex, type IndexedRef } from '$lib/stores/questIndex';
import { get } from 'svelte/store';

/** 커서 직전의 ID 토큰 (앞이 `[` 가 아니어야 — 위키링크 안은 제외). */
const BEFORE_CURSOR = /(^|[^[\w-])([A-Za-z]{2,}-\d+)$/;

function questIdCompletion(context: CompletionContext): CompletionResult | null {
	// 커서 앞 텍스트에서 ID 토큰 추출.
	const line = context.state.doc.lineAt(context.pos);
	const before = context.state.sliceDoc(line.from, context.pos);
	const m = BEFORE_CURSOR.exec(before);
	if (!m) return null;

	const token = m[2];
	const from = context.pos - token.length;
	// 명시적 호출(Ctrl-Space)이 아니면, 타이핑 중에만 — 토큰 경계에서.
	if (!context.explicit && token.length < 3) return null;

	const id = token.toUpperCase();
	const ref: IndexedRef | undefined = get(questIndex).get(id);
	const kindLabel = ref
		? ref.kind === 'campaign'
			? '캠페인'
			: '퀘스트'
		: '미존재';

	const option: Completion = {
		label: `[[${id}]]`,
		displayLabel: `🔗 ${id} 링크 걸기`,
		detail: ref ? `${kindLabel} · ${ref.title}` : `${kindLabel} — 링크는 생성 (빨강 표시)`,
		apply: `[[${id}]]`,
		type: 'reference',
		// 실재 ID 를 위로.
		boost: ref ? 1 : -1
	};

	return {
		from,
		to: context.pos,
		options: [option],
		// 토큰이 더 길어지면 다시 매칭.
		validFor: /^[A-Za-z]{2,}-\d+$/
	};
}

/**
 * quest / campaign 상세 편집기에 추가하는 확장. markdown 언어 데이터로 등록해
 * basicSetup 의 autocompletion 과 공존한다 (override 아님).
 */
export function crossLinkAutocomplete(): Extension {
	// 후보 detail(제목) 표시를 위해 인덱스 미리 적재.
	loadQuestIndex();
	return [
		markdownLanguage.data.of({ autocomplete: questIdCompletion }),
		// basicSetup 가 이미 autocompletion 을 포함하지만, 타이핑 중 자동 활성화를
		// 보장하기 위해 명시 (config 는 병합됨).
		autocompletion({ activateOnTyping: true })
	];
}
