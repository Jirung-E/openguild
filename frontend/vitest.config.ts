import { sveltekit } from '@sveltejs/kit/vite';
import { defineConfig } from 'vitest/config';

export default defineConfig({
	plugins: [sveltekit()],
	resolve: {
		// Svelte 5 + jsdom: 클라이언트 entry 사용 (index-server.js 막기)
		conditions: ['browser']
	},
	test: {
		environment: 'jsdom',
		include: ['src/**/*.{test,spec}.{js,ts,svelte}'],
		setupFiles: ['./src/test-setup.ts'],
		globals: true
	}
});
