import { sveltekit } from '@sveltejs/kit/vite';
import { defineConfig } from 'vite';

export default defineConfig({
	plugins: [sveltekit()],
	build: {
		// DEV-026 / DEV-111: cytoscape (~645KB), mermaid (~594KB) 는 dynamic
		// import 로 별도 chunk 로 분리됨. lazy-load 라 첫 화면 영향 없음.
		// 두 라이브러리는 압축해도 500KB 넘는 게 정상 → 경고 threshold 만 ↑.
		chunkSizeWarningLimit: 700
	}
});
