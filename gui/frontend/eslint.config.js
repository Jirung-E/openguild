import prettier from 'eslint-config-prettier';
import path from 'node:path';
import { includeIgnoreFile } from '@eslint/compat';
import js from '@eslint/js';
import svelte from 'eslint-plugin-svelte';
import { defineConfig } from 'eslint/config';
import globals from 'globals';
import ts from 'typescript-eslint';
import svelteConfig from './svelte.config.js';

const gitignorePath = path.resolve(import.meta.dirname, '.gitignore');

export default defineConfig(
	includeIgnoreFile(gitignorePath),
	js.configs.recommended,
	ts.configs.recommended,
	svelte.configs.recommended,
	prettier,
	svelte.configs.prettier,
	{
		languageOptions: { globals: { ...globals.browser, ...globals.node } },
		rules: {
			// typescript-eslint strongly recommend that you do not use the no-undef lint rule on TypeScript projects.
			// see: https://typescript-eslint.io/troubleshooting/faqs/eslint/#i-get-errors-from-the-no-undef-rule-about-global-variables-not-being-defined-even-though-there-are-no-typescript-errors
			'no-undef': 'off',
			// `const { [k]: _drop, ...rest } = obj` — 키 하나를 뺀 사본을 만드는 관용구. 뺀 값은
			// 받을 수밖에 없고 쓰지 않는다. 기본값(false)이면 이것을 미사용 변수로 잡는다.
			// DEV-472: `_` 로 시작하는 이름은 **일부러 안 쓴다**는 표시다(`{#each xs as _x, i}` · `use:` 동작의 `_node`).
			'@typescript-eslint/no-unused-vars': [
				'error',
				{
					ignoreRestSiblings: true,
					argsIgnorePattern: '^_',
					varsIgnorePattern: '^_',
					caughtErrorsIgnorePattern: '^_'
				}
			],
			// DEV-472: 앱은 base path 없이 루트에서 돈다(데스크톱 asset 프로토콜 · 서버 둘 다). 주소도 쿼리를 붙여
			// 그때그때 만든다(`/quests/${id}?from=list`) — `resolve()` 는 글자 그대로의 경로 id 만 받아 이득 없이
			// 모든 링크를 바꿔야 한다.
			'svelte/no-navigation-without-resolve': 'off',
			// DEV-472: 이 코드의 관례는 **복사해서 통째로 갈아 끼우기**다(`const next = new Set(x); …; x = next`) —
			// `$state` 가 새 값을 본다. 함수 안의 계산용 Set · Map 과, 일부러 반응형이 아니게 둔 것(보드의
			// 저장 위치 · 드래그 시작점 — BUG-054, 첨부 URL 캐시)까지 SvelteSet · SvelteMap 으로 바꾸라고 한다.
			// `$state` 로 들고 `.add()` 하는 실수는 없었다(2026-11-01 전수 확인).
			'svelte/prefer-svelte-reactivity': 'off',
			// `message={'첫 줄\n둘째 줄'}` — 따옴표 속성으로 바꾸면 `\n` 이 줄바꿈이 아니게 된다.
			'svelte/no-useless-mustaches': ['error', { ignoreStringEscape: true }]
		}
	},
	{
		files: ['**/*.svelte', '**/*.svelte.ts', '**/*.svelte.js'],
		languageOptions: {
			parserOptions: {
				projectService: true,
				extraFileExtensions: ['.svelte'],
				parser: ts.parser,
				svelteConfig
			}
		}
	},
	{
		// Override or add rule settings here, such as:
		// 'svelte/button-has-type': 'error'
		rules: {}
	}
);
