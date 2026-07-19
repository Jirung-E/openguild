import { sveltekit } from '@sveltejs/kit/vite';
import { defineConfig } from 'vite';

export default defineConfig({
	plugins: [sveltekit()],
	server: {
		// BUG-147: 브라우저 dev(`npm run dev`, :5173)에서 상대경로 `/api` 요청을
		// openguild-server(:3000)로 프록시. 이러면 `VITE_API_URL`/`.env.development`
		// 없이도 `just dev-frontend` + `just dev-server` 조합이 바로 붙는다
		// (이전엔 env 파일이 gitignore 라 신규 클론에서 API 가 전부 404 였음).
		// `VITE_API_URL` 을 명시하면 프론트가 절대 URL 로 요청해 이 프록시를
		// 우회(원격 서버 지정 등) — 두 방식 공존.
		proxy: {
			'/api': 'http://localhost:3000'
		}
	},
	build: {
		// DEV-026 / DEV-111: cytoscape (~645KB), mermaid (~594KB) 는 dynamic
		// import 로 별도 chunk 로 분리됨. lazy-load 라 첫 화면 영향 없음.
		// 두 라이브러리는 압축해도 500KB 넘는 게 정상 → 경고 threshold 만 ↑.
		chunkSizeWarningLimit: 700
	}
});
