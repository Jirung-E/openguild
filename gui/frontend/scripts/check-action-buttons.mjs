#!/usr/bin/env node
// DEV-440: 취소 · 확인 단추 쌍은 `ActionButtons` 하나로 — 순서를 한 곳에서 정한다.
//
// 화면마다 단추를 직접 쓰던 때, 확인 창(ConfirmDialog · 퀘스트 삭제)은 [취소][삭제] 인데 보드 상태 바꾸기 ·
// 만들기 창 · 편집 뒤 저장 줄은 [저장][취소] 라 같은 앱에서 손이 엇갈렸다(admin: "퀘스트 삭제 시도시에는
// 취소가 왼쪽인데, 보드에서 상태변경 시도시에는 취소가 오른쪽임").
//
// 이 검사는 `<button …>{t('….cancel', …)}` 처럼 **취소 글을 직접 단 단추**를 막는다. 취소 단추는
// `ActionButtons` 가 그린다(글을 바꾸려면 `cancelLabel`). 하나로 "추가 ↔ 취소" 를 오가는 토글 단추
// (`{open ? t('common.cancel') : t('…add')}`)와 닫기 X 의 `aria-label` 은 쌍이 아니라서 걸리지 않는다.
//
// DEV-440 후속: 모달 틀도 `Modal` 하나 — 창마다 막 · 상자 · 제목 · X 를 따로 그리던 것(X 만 세 가지)을 막는다.
// `aria-modal` 을 직접 단 요소가 `Modal.svelte` 밖에 있으면 실패한다.
//
// DEV-440 후속: 고르기도 `Select` 하나 — 꺾쇠를 글자색 아이콘으로 얹는 틀. select 를 직접 쓰면 꺾쇠가 없거나
// (브라우저 기본 모양) 화면마다 다른 꺾쇠가 된다(도구줄만 아이콘, 나머지는 회색 그라디언트였다).
// `<select` 가 `Select.svelte` 밖에 있으면 실패한다.
import { readdirSync, readFileSync, statSync } from 'node:fs';
import { join, relative } from 'node:path';
import { fileURLToPath } from 'node:url';

const SRC = fileURLToPath(new URL('../src', import.meta.url));
const CANON = 'lib/components/ActionButtons.svelte';
const MODAL = 'lib/components/Modal.svelte';
const DIRECT_MODAL = /aria-modal=/g;
const SELECT = 'lib/components/Select.svelte';
const DIRECT_SELECT = /<select\b/g;

/** 단추 여는 태그 바로 뒤의 글이 `{t('….cancel'` 이나 `{t('common.no'` 로 시작하면 직접 단 취소 단추다. */
const DIRECT_CANCEL = /<button\b[^>]*>\s*\{t\('(?:[\w.]*\.cancel|common\.no)'/g;

const violations = [];

function walk(dir) {
	for (const name of readdirSync(dir)) {
		const p = join(dir, name);
		if (statSync(p).isDirectory()) walk(p);
		else if (p.endsWith('.svelte')) check(p);
	}
}

function check(path) {
	const rel = relative(SRC, path).split('\\').join('/');
	if (rel === CANON) return;
	const text = readFileSync(path, 'utf8');
	for (const m of text.matchAll(DIRECT_CANCEL)) {
		const line = text.slice(0, m.index).split('\n').length;
		violations.push(`${rel}:${line}  취소 단추 → ActionButtons`);
	}
	if (rel !== MODAL) {
		for (const m of text.matchAll(DIRECT_MODAL)) {
			const line = text.slice(0, m.index).split('\n').length;
			violations.push(`${rel}:${line}  모달 틀 → Modal`);
		}
	}
	if (rel !== SELECT) {
		for (const m of text.matchAll(DIRECT_SELECT)) {
			const line = text.slice(0, m.index).split('\n').length;
			violations.push(`${rel}:${line}  고르기 → Select`);
		}
	}
}

walk(SRC);

if (violations.length > 0) {
	console.error(
		'공통 부품을 거치지 않았습니다 — 취소 단추는 `ActionButtons`(취소 왼쪽, 하는 일 오른쪽), 모달 틀은 `Modal`, 고르기는 `Select`:'
	);
	for (const v of violations) console.error(`  ${v}`);
	process.exit(1);
}
console.log(
	'check:actions — 취소 · 확인 단추는 ActionButtons, 모달 틀은 Modal, 고르기는 Select 로 그립니다.'
);
