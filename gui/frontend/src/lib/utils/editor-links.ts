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
	startCompletion,
	type Completion,
	type CompletionContext,
	type CompletionResult
} from '@codemirror/autocomplete';
import { markdownLanguage, commonmarkLanguage } from '@codemirror/lang-markdown';
import type { Extension } from '@codemirror/state';
import { tooltips, type EditorView } from '@codemirror/view';
import {
	questIndex,
	loadQuestIndex,
	KIND_ALIASES,
	KIND_NAMESPACE,
	KIND_LABEL,
	type Kind
} from '$lib/stores/questIndex';
import { get } from 'svelte/store';

/** DEV-173: `[[` 바로 안(아직 안 닫힘)의 부분 slug — 규칙 포함 전체 인덱스 제안.
 *  규칙 slug 는 한글 등 비ASCII 가능 — 대괄호 제외 모든 문자 허용.
 *  BUG-156: 규칙 slug 에 **공백이 들어갈 수 있어**(`[[코딩 규칙]]`) 예전처럼
 *  `\s` 를 배제하면 띄어쓰기 순간 자동완성이 끊겼다. 공백은 허용하되,
 *  `[[` 를 닫지 않고 계속 타이핑할 때 팝업이 무한정 따라붙지 않도록 64자로
 *  상한(렌더러 CROSS_LINK_RE 의 토큰 상한과 동일). */
const BEFORE_CURSOR_WIKI = /\[\[([^[\]\n\r]{0,64})$/;

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

	// DEV-219: 사용자가 `[[rules:` 처럼 kind 접두를 이미 타이핑했으면 그 종류로만
	// 필터, 나머지를 query 로 매칭. 접두가 없거나 별칭에 없으면 기존처럼 전체.
	const ci = partial.indexOf(':');
	const typedPrefix = ci > 0 ? partial.slice(0, ci).toLowerCase() : null;
	const kindFilter = typedPrefix ? KIND_ALIASES[typedPrefix] : undefined;
	const query = kindFilter ? partial.slice(ci + 1) : partial;
	const upper = query.toUpperCase();
	const index = get(questIndex);
	const options: Completion[] = [];
	// DEV-219 후속(admin 보고): `[[q` 처럼 콜론 없이 타이핑 중이면 실제 ID 뿐
	// 아니라 "네임스페이스 자체"(`quest:` 등)도 후보로 보여준다 — 선택하면
	// 접두만 삽입되고(`]]` 로 안 닫음) 그 종류로 필터된 자동완성이 바로 재오픈.
	if (!kindFilter) {
		const lower = partial.toLowerCase();
		const seenKinds = new Set<Kind>();
		for (const [alias, kind] of Object.entries(KIND_ALIASES)) {
			if (seenKinds.has(kind)) continue;
			const canonical = KIND_NAMESPACE[kind];
			if (!canonical.startsWith(lower) && !alias.startsWith(lower)) continue;
			seenKinds.add(kind);
			const nsInsert = `${canonical}:`;
			options.push({
				label: nsInsert,
				// DEV-302: 라벨의 🏷️ 제거 — CodeMirror 자동완성은 문자열만 받으므로
				// 아이콘은 `type` 에 딸린 `.cm-completionIcon-namespace` 로 CSS 에서
				// 그린다(global.css). 예전 `type:'keyword'` 은 CM 기본 글리프가
				// 🔑(+U+FE0E) 라 OS 에 따라 컬러 이모지로 떴다.
				displayLabel: nsInsert,
				detail: `네임스페이스 — ${KIND_LABEL[kind]}만 보기`,
				apply: (view: EditorView, _c: Completion, from: number, to: number) => {
					view.dispatch({
						changes: { from, to, insert: nsInsert },
						selection: { anchor: from + nsInsert.length }
					});
					startCompletion(view);
				},
				type: 'namespace'
			});
		}
	}
	for (const [id, ref] of index) {
		if (kindFilter && ref.kind !== kindFilter) continue;
		// DEV-239: 도서관 문서는 "폴더/제목" 경로로 타이핑해도 찾을 수 있어야
		// 함 — 매칭만 경로 기준, 실제 삽입은 여전히 `[[library:BOOK-NNN]]`.
		const pathLabel = ref.kind === 'book' && ref.path ? `${ref.path}/${ref.title}` : null;
		const idMatch = id.startsWith(upper);
		const pathMatch = pathLabel != null && pathLabel.toUpperCase().startsWith(upper);
		if (!idMatch && !pathMatch) continue;
		// DEV-219(admin 결정): 자동완성은 항상 `kind:` 접두를 붙여 삽입 — 나중에
		// 같은 ID 가 다른 종류로 생겨도 이미 건 링크가 안전하도록 미리 예방.
		// 삽입은 규칙=원본 slug / quest·campaign·book=대문자 정규형.
		const insert = `${KIND_NAMESPACE[ref.kind]}:${ref.kind === 'rule' ? (ref.slug ?? id.toLowerCase()) : id}`;
		options.push({
			label: pathMatch && !idMatch ? pathLabel! : id,
			displayLabel: insert,
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
	options.sort((a, b) => {
		const ak = a.type === 'namespace' ? 0 : 1;
		const bk = b.type === 'namespace' ? 0 : 1;
		return ak !== bk ? ak - bk : a.label.localeCompare(b.label);
	});
	// DEV-239: label 이 id 또는 폴더 경로 둘 중 하나라 CodeMirror 기본 fuzzy
	// 필터(label 기준)가 경로 매칭 후보를 지워버림 — 필터링은 위에서 이미
	// 완료했으니 자체 필터를 끈다.
	return { from: context.pos - partial.length, to: context.pos, options, filter: false };
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
