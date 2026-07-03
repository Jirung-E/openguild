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
import { questIndex, loadQuestIndex } from '$lib/stores/questIndex';
import { get } from 'svelte/store';

/** 커서 직전의 ID 토큰 (앞이 `[` 가 아니어야 — 위키링크 안은 제외). */
const BEFORE_CURSOR = /(^|[^[\w-])([A-Za-z]{2,}-\d+)$/;
/** DEV-173: `[[` 바로 안(아직 안 닫힘)의 부분 slug — 규칙 포함 전체 인덱스 제안. */
const BEFORE_CURSOR_WIKI = /\[\[([\w-]*)$/;

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
	// 빈 `[[` 는 명시 호출(Ctrl-Space)일 때만 전체 목록.
	if (!context.explicit && partial.length < 1) return null;

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
			apply: `${insert}]]`,
			type: 'reference'
		});
	}
	if (options.length === 0) return null;
	options.sort((a, b) => a.label.localeCompare(b.label));
	return { from: context.pos - partial.length, to: context.pos, options };
}

function questIdCompletion(context: CompletionContext): CompletionResult | null {
	// DEV-173: `[[` 안이면 위키 컨텍스트(규칙 포함)가 담당.
	const wiki = wikiContextCompletion(context);
	if (wiki) return wiki;
	// 커서 앞 텍스트에서 ID 토큰 추출.
	const line = context.state.doc.lineAt(context.pos);
	const before = context.state.sliceDoc(line.from, context.pos);
	const m = BEFORE_CURSOR.exec(before);
	if (!m) return null;

	const token = m[2];
	const from = context.pos - token.length;
	// 명시적 호출(Ctrl-Space)이 아니면, 토큰이 충분히 길 때만 (XXX-N 이상).
	if (!context.explicit && token.length < 3) return null;

	const upper = token.toUpperCase();
	const index = get(questIndex);

	// DEV-140 #7(2) / DEV-171 후속: 맨 위는 항상 '현재 입력값'(그대로 링크),
	// 그 아래로 prefix 매칭 실재 ID. label 을 실재 id 로 두어야 CM 필터가 타이핑한
	// bare 토큰과 정상 매칭 (#1: label 이 `[[..]]` 라 매칭 실패 → validFor 충돌로
	// 깜빡이던 버그 수정).
	const selfRef = index.get(upper);
	const options: Completion[] = [
		{
			label: upper,
			displayLabel: `🔗 ${upper}${selfRef ? '' : ' (미존재)'}`,
			detail: selfRef
				? `${KIND_LABEL[selfRef.kind]} · ${selfRef.title}`
				: '링크 생성 — 렌더 시 빨강',
			apply: `[[${upper}]]`,
			type: 'reference',
			// 현재 입력값을 항상 맨 위로.
			boost: 99
		}
	];
	const matches: Completion[] = [];
	for (const [id, ref] of index) {
		// DEV-173: bare 토큰 제안은 quest/campaign 만 — 규칙은 `[[ 컨텍스트 전용.
		if (ref.kind === 'rule') continue;
		if (id !== upper && id.startsWith(upper)) {
			matches.push({
				label: id,
				displayLabel: `🔗 ${id}`,
				detail: `${KIND_LABEL[ref.kind]} · ${ref.title}`,
				apply: `[[${id}]]`,
				type: 'reference'
			});
		}
	}
	matches.sort((a, b) => a.label.localeCompare(b.label));
	options.push(...matches);

	// DEV-140 #9: validFor 를 두지 않는다 — 두면 CM 이 최초 쿼리에서 slice 된 상위
	// N개만 재필터해, 더 좁혀도(예: DEV-1xx 후반) 안 뜨던 문제. 키마다 소스를 재실행해
	// 현재 prefix 의 최신 매칭을 다시 계산(댓글과 동일 동작).
	return {
		from,
		to: context.pos,
		options
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
		// DEV-140 후속(버그): 편집기는 `markdown()` = 기본 base **commonmark** 를
		// 쓰는데 이전엔 자동완성 소스를 GFM `markdownLanguage` 에만 등록해, 커서
		// 위치의 활성 언어(commonmark)와 불일치 → 소스가 안 잡혀 '🔗 링크 걸기' 가
		// 안 떴다. base 가 무엇이든 동작하도록 두 언어 모두에 등록.
		commonmarkLanguage.data.of({ autocomplete: questIdCompletion }),
		markdownLanguage.data.of({ autocomplete: questIdCompletion }),
		// basicSetup 가 이미 autocompletion 을 포함하지만, 타이핑 중 자동 활성화를
		// 보장하기 위해 명시 (config 는 병합됨).
		autocompletion({ activateOnTyping: true })
	];
}
