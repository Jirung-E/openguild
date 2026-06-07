import { sveltekit } from '@sveltejs/kit/vite';
import { defineConfig } from 'vite';

export default defineConfig({
	plugins: [sveltekit()],
	build: {
		// DEV-026 / DEV-111: cytoscape (~645KB), mermaid (~594KB) 는 dynamic
		// import 로 별도 chunk 로 분리됨. lazy-load 라 첫 화면 영향 없음.
		// 두 라이브러리는 압축해도 500KB 넘는 게 정상 → 경고 threshold 만 ↑.
		chunkSizeWarningLimit: 700,
		// Rolldown 의 [PLUGIN_TIMINGS] 경고 비활성. vite-plugin-sveltekit-guard
		// (SvelteKit 의 보안 검증 플러그인 — client 가 $lib/server/* / $env/static/private
		// 같은 server 전용 모듈을 들이지 않는지 import 그래프 분석) 가 큰 codebase
		// 에서 시간 비중 80%+ 차지하지만 외부 통신 X (순수 CPU). 정보성 경고라
		// 정상 동작에 영향 없음.
		rollupOptions: {
			// @ts-expect-error — rolldown-vite 의 rollupOptions 는 rolldown InputOptions
			// 로 그대로 forward 됨. `checks.pluginTimings: false` 가 해당 경고 끔.
			checks: { pluginTimings: false }
		}
	}
});
