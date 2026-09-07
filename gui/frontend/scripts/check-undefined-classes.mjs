#!/usr/bin/env node
// DEV-382: markup 이 쓰는 class 중 **아무 데도 정의가 없는 것** 차단.
//
// admin 지적("버튼 모양이 다른 ui들과 현저히 다름")의 원인이 이거였다.
// 플러그인 설정 화면에 `class="ghost"` / `class="primary"` 라고 썼는데 이
// 저장소에는 그런 클래스가 **없다.** 그래서 브라우저 기본 버튼이 그대로
// 나왔고, 회색 덩어리 하나만 다른 UI 와 따로 놀았다.
//
// Svelte 컴파일러는 반대 방향(정의했는데 안 쓰는 selector)만 경고한다.
// 쓰는데 정의가 없는 쪽은 아무도 안 잡아서, 조용히 "스타일이 안 먹은 화면"
// 으로만 드러난다 — 눈으로 보기 전까지 모른다.
//
// 판정: `class="a b c"` 와 `class:foo` 로 쓰인 이름이 그 파일의 <style> 이나
// `lib/styles/global.css` 에 `.이름` 으로 나와야 한다.
//
// **허용되는 예외** (ALLOW 에 등록): 부모가 `:global()` 로 칠하는 클래스,
// 의미만 싣고 칠하지는 않는 표식 등 — 이유를 함께 적을 것.
import { readdirSync, readFileSync, statSync } from 'node:fs';
import { join, relative } from 'node:path';
import { fileURLToPath } from 'node:url';

const SRC = fileURLToPath(new URL('../src', import.meta.url));
const CANON = 'lib/styles/global.css';

/**
 * DEV-382 도입 시점에 이미 있던 것들. **줄이되 늘리지 말 것** —
 * 하나씩 확인해 지우거나(죽은 클래스) 스타일을 붙이면 된다.
 */
const ALLOW = new Map([
	['lib/components/QuestBoard.svelte', new Set(['resolved'])],
	['lib/components/QuestListFilter.svelte', new Set(['chips-toggle'])],
	['lib/components/TagFilterRow.svelte', new Set(['open'])],
	['lib/components/admin/AdminStatusesSection.svelte', new Set(['dim', 'done-mark'])],
	[
		'routes/campaigns/[slug]/+page.svelte',
		new Set(['meta', 'meta-item', 'status', 'type'])
	],
	['routes/quests/[id]/+page.svelte', new Set(['status', 'urgency'])],
	['routes/settings/+page.svelte', new Set(['hint', 'theme-row'])],
	['routes/welcome/+page.svelte', new Set(['active'])],
	['routes/worklog/+page.svelte', new Set(['open', 'viewmode'])]
]);

function walk(dir, out = []) {
	for (const name of readdirSync(dir)) {
		const p = join(dir, name);
		if (statSync(p).isDirectory()) walk(p, out);
		else if (p.endsWith('.svelte')) out.push(p);
	}
	return out;
}

const selectors = (css) => new Set([...css.matchAll(/\.([a-zA-Z][\w-]*)/g)].map((m) => m[1]));

const globalClasses = selectors(readFileSync(join(SRC, CANON), 'utf8'));
const hits = [];

for (const file of walk(SRC)) {
	const rel = relative(SRC, file);
	const src = readFileSync(file, 'utf8');
	const style = src.match(/<style[^>]*>([\s\S]*?)<\/style>/);
	const local = style ? selectors(style[1]) : new Set();
	const allow = ALLOW.get(rel) ?? new Set();

	const used = new Set();
	// 정적 리터럴만 본다 — `class={expr}` 은 값을 알 수 없으므로 건너뛴다.
	for (const m of src.matchAll(/class="([^"{}]*)"/g)) {
		for (const t of m[1].split(/\s+/)) if (t) used.add(t);
	}
	for (const m of src.matchAll(/class:([a-zA-Z][\w-]*)/g)) used.add(m[1]);

	for (const c of [...used].sort()) {
		if (local.has(c) || globalClasses.has(c) || allow.has(c)) continue;
		hits.push(`${rel}  class="${c}"`);
	}
}

if (hits.length) {
	console.error(`\n✗ 정의가 없는 class ${hits.length}건 — 스타일이 안 먹습니다.`);
	console.error('  그 파일의 <style> 이나 lib/styles/global.css 에 규칙을 추가하세요.');
	console.error('  부모가 :global() 로 칠하는 등 정당한 예외면 스크립트의 ALLOW 에 이유와 함께 등록하세요.\n');
	for (const h of hits) console.error(`  ${h}`);
	process.exit(1);
}
console.log('✓ 정의 없는 class 없음.');
