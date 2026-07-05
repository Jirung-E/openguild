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
import { markdownLanguage, commonmarkLanguage } from '@codemirror/lang-markdown';
import type { Extension } from '@codemirror/state';
import { tooltips, type EditorView } from '@codemirror/view';
import { questIndex, loadQuestIndex } from '$lib/stores/questIndex';
import { get } from 'svelte/store';

/** DEV-173: `[[` 바로 안(아직 안 닫힘)의 부분 slug — 규칙 포함 전체 인덱스 제안.
 *  규칙 slug 는 한글 등 비ASCII 가능 — 공백/대괄호 제외 모든 문자 허용. */
const BEFORE_CURSOR_WIKI = /\[\[([^[\]\s]*)$/;

const KIND_LABEL = { quest: '퀘스트', campaign: '캠페인', rule: '규칙' } as const;

/**
 * DEV-173: `[[` 컨텍스트 자동완성 — 규칙 slug 포함 전체 인덱스에서 prefix 매칭.
 * bare 토큰 인식(questIdCompletion)은 quest/campaign 만 유지 — 규칙 slug 는
 * 일반 단어와 구분이 안 돼 `[[ 안에서만` 제안한다 (퀘스트 본문 결정 사항).
 */
function wikiContextCompletion(context: CompletionContext): CompletionResult | null {
	const line = context.state.doc.lineAt(context.pos);
	const before = context.state.sliceDoc(line.from, context.pos);
	const m = BEFORE_CURSOR_WIKI.exec(before);
	if (!m) return null;
	const partial = m[1];
	// DEV-223: 빈 `[[` 에서도 전체 후보 표시 (사용자 결정 — [[ 입력 즉시 팝업).

	const upper = partial.toUpperCase();
	const index = get(questIndex);
	const options: Completion[] = [];
	for (const [id, ref] of index) {
		if (!id.startsWith(upper)) continue;
		// 삽입은 규칙=원본 slug / quest·campaign=대문자 정규형. 이미 열린 `[[` 뒤라
		// 나머지 + `]]` 만 삽입.
		const insert = ref.kind === 'rule' ? (ref.slug ?? id.toLowerCase()) : id;
		options.push({
			label: id,
			displayLabel: `🔗 ${insert}`,
			detail: `${KIND_LABEL[ref.kind]} · ${ref.title}`,
			// DEV-223: closeBrackets 가 `[[` 입력 시 `]]` 를 자동 삽입해두므로,
			// 문자열 apply(`insert]]`)를 쓰면 [[slug]]]] 로 중복된다 — 커서 뒤에
			// 이미 `]]` 가 있으면 재사용하고 커서만 그 뒤로 이동.
			apply: (view: EditorView, _c: Completion, from: number, to: number) => {
				const alreadyClosed = view.state.sliceDoc(to, to + 2) === ']]';
				view.dispatch({
					changes: { from, to, insert: alreadyClosed ? insert : `${insert}]]` },
					selection: { anchor: from + insert.length + 2 }
				});
			},
			type: 'reference'
		});
	}
	if (options.length === 0) return null;
	options.sort((a, b) => a.label.localeCompare(b.label));
	return { from: context.pos - partial.length, to: context.pos, options };
}

// DEV-220(사용자 결정): bare 토큰(XXX-NNN 그냥 타이핑) 자동완성 트리거 제거 —
// 일반 문자열 입력 중 팝업이 뜨는 게 불편. 자동완성은 `[[` 컨텍스트에서만.
function questIdCompletion(context: CompletionContext): CompletionResult | null {
	return wikiContextCompletion(context);
}

/**
 * quest / campaign 상세 편집기에 추가하는 확장. markdown 언어 데이터로 등록해
 * basicSetup 의 autocompletion 과 공존한다 (override 아님).
 */
export function crossLinkAutocomplete(): Extension {
	// 후보 detail(제목) 표시를 위해 인덱스 미리 적재.
	loadQuestIndex();
	return [
		// DEV-140 후속(버그): 편집기는 `markdown()` = 기본 base **commonmark** 를
		// 쓰는데 이전엔 자동완성 소스를 GFM `markdownLanguage` 에만 등록해, 커서
		// 위치의 활성 언어(commonmark)와 불일치 → 소스가 안 잡혀 '🔗 링크 걸기' 가
		// 안 떴다. base 가 무엇이든 동작하도록 두 언어 모두에 등록.
		commonmarkLanguage.data.of({ autocomplete: questIdCompletion }),
		markdownLanguage.data.of({ autocomplete: questIdCompletion }),
		// basicSetup 가 이미 autocompletion 을 포함하지만, 타이핑 중 자동 활성화를
		// 보장하기 위해 명시 (config 는 병합됨).
		//
		// BUG-115 후속: 진짜 원인은 overflow 클리핑이 아니라 CodeMirror
		// autocompletion 의 기본 maxRenderedOptions(100) 이었다 — 빈 `[[` 는
		// 길드 전체 quest/campaign/rule(수백 개)을 다 후보로 주는데, 알파벳
		// 정렬(BUG < DEV < REQ) 상 100번째에서 잘려 "BUG-1xx 까지만 보임"으로
		// 나타났다. 길드가 커도 다 보이게 넉넉히 올림.
		autocompletion({ activateOnTyping: true, maxRenderedOptions: 1000 }),
		// BUG-115: 편집기를 감싼 `.editor-wrap` 이 리사이즈 핸들/모서리 때문에
		// `overflow: hidden` 이라, 기본 위치(에디터 DOM 내부)로 뜨는 자동완성
		// 툴팁이 그 경계에서 잘릴 수 있다(조상에 transform 이 있으면 CodeMirror
		// 가 absolute 로 폴백). 툴팁을 document.body 에 붙여 우회.
		tooltips({ parent: document.body })
	];
}
