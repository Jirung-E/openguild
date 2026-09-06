#!/usr/bin/env node
// DEV-372: 검사들이 **세 곳에 모두** 등록돼 있는지 검사한다.
//
// 가드를 하나 추가하려면 `package.json`(스크립트) / `justfile`(test-frontend)
// / `.github/workflows/check.yml`(gui-frontend job) 을 각각 고쳐야 한다. 한
// 곳을 빠뜨려도 **아무 말이 없다** — 그래서 실제로 두 번 빠졌다:
//
//   BUG-251  `check:no-emoji` 가 justfile 에 없었다.
//   DEV-372  `npm run build` 가 justfile 에 없었다(CI 에만 있었다). 그 바람에
//            `just test` 만 믿으면 프로덕션 빌드가 깨져도 로컬은 통과했다.
//
// justfile 의 주석은 두 번 다 "check.yml 과 같은 항목" 이라고 적혀 있었다.
// 주석이 아니라 검사가 지켜야 한다.
//
// 검사하는 것:
//   1. `scripts/check-*.mjs` 가 전부 `package.json` 의 스크립트로 등록돼 있다.
//   2. CI 의 gui-frontend job 이 돌리는 npm 명령 집합 == justfile 의
//      `test-frontend` 가 돌리는 집합.
import { readdirSync, readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';

const HERE = fileURLToPath(new URL('.', import.meta.url));
const FRONTEND = fileURLToPath(new URL('..', import.meta.url));
const ROOT = fileURLToPath(new URL('../../..', import.meta.url));

const read = (p) => readFileSync(p, 'utf8');
const problems = [];

// ── 1. 가드 스크립트 ↔ package.json ──────────────────────────────
const pkg = JSON.parse(read(FRONTEND + '/package.json'));
const scripts = pkg.scripts ?? {};
const registered = new Set(
	Object.values(scripts)
		.map((cmd) => /node scripts\/([\w.-]+\.mjs)/.exec(cmd)?.[1])
		.filter(Boolean)
);
for (const f of readdirSync(HERE).filter((n) => n.startsWith('check-') && n.endsWith('.mjs'))) {
	if (!registered.has(f)) {
		problems.push(`scripts/${f} 가 package.json 의 스크립트로 등록되지 않았습니다.`);
	}
}

/** `npm run x` / `npm test ...` 형태의 명령만 뽑아 정규화한다. */
function npmCommands(text) {
	const out = new Set();
	for (const m of text.matchAll(/npm (run [\w:-]+|test[^\n]*)/g)) {
		let cmd = m[1].trim();
		if (cmd.startsWith('test')) cmd = 'test'; // `test -- --run` 등 인자 차이는 무시
		out.add(cmd);
	}
	return out;
}

// ── 2. CI 의 gui-frontend job ↔ justfile 의 test-frontend ─────────
const yml = read(ROOT + '/.github/workflows/check.yml');
// 해당 job 만 잘라낸다 — 다른 job(cargo)의 npm 호출까지 세면 안 된다.
const jobStart = yml.indexOf('gui/frontend (svelte-kit)');
const jobEnd = yml.indexOf('\n  ', yml.indexOf('npm run build', jobStart));
const ciJob = jobStart < 0 ? '' : yml.slice(jobStart, jobEnd < 0 ? yml.length : jobEnd);

const just = read(ROOT + '/justfile');
const jfStart = just.indexOf('test-frontend:');
const jfEnd = just.indexOf('\n\n', jfStart);
const jfBody = jfStart < 0 ? '' : just.slice(jfStart, jfEnd < 0 ? just.length : jfEnd);

if (ciJob === '' || jfBody === '') {
	problems.push('CI 의 gui-frontend job 또는 justfile 의 test-frontend 를 찾지 못했습니다.');
} else {
	const ci = npmCommands(ciJob);
	const jf = npmCommands(jfBody);
	// `npm ci`(의존성 설치)는 로컬에 필요 없다 — 비교에서 뺀다.
	ci.delete('run ci');
	for (const c of ci) {
		if (!jf.has(c))
			problems.push(`justfile 의 test-frontend 에 \`npm ${c}\` 가 없습니다 (CI 에는 있음).`);
	}
	for (const c of jf) {
		if (!ci.has(c))
			problems.push(`CI 의 gui-frontend job 에 \`npm ${c}\` 가 없습니다 (justfile 에는 있음).`);
	}
}

if (problems.length > 0) {
	console.error(`✗ 검사 등록이 어긋난 곳 ${problems.length}건.`);
	console.error('  가드는 package.json / justfile / check.yml 세 곳에 모두 있어야 합니다.');
	console.error('  한 곳이라도 빠지면 그 검사는 조용히 안 돌아갑니다(BUG-251).\n');
	for (const p of problems) console.error('  ' + p);
	process.exit(1);
}
console.log('✓ 검사 등록 일치 — package.json / justfile / check.yml.');
