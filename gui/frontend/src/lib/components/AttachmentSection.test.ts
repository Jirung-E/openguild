// DEV-321: 진행 바가 실제 %를 반영하는지 — 이게 깨지면 사용자에겐 예전처럼
// "돌고는 있는데 얼마나 남았는지 모르는" 상태로 조용히 되돌아간다.

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, waitFor } from '@testing-library/svelte';
import AttachmentSection from './AttachmentSection.svelte';
import { pickAndUploadAttachments, type AttachProgress } from '$lib/utils/editor-attach';

vi.mock('$lib/utils/editor-attach', () => ({
	pickAndUploadAttachments: vi.fn()
}));
vi.mock('$lib/api/client', () => ({
	api: { post: vi.fn(), delete: vi.fn() }
}));
vi.mock('$lib/utils/banner', () => ({
	guildFileUrl: vi.fn().mockResolvedValue('blob:preview')
}));

/** 업로드를 시작시키고, 테스트가 진행 상태를 직접 밀어 넣을 수 있게 한다. */
function drive(steps: (push: (p: AttachProgress | null) => void) => Promise<void> | void) {
	vi.mocked(pickAndUploadAttachments).mockImplementation(async (handlers) => {
		await steps((p) => handlers.onProgress?.(p));
	});
}

const bar = (c: HTMLElement) => c.querySelector<HTMLElement>('.up-fill');
const row = (c: HTMLElement) => c.querySelector<HTMLElement>('[role="progressbar"]');

describe('AttachmentSection 업로드 진행 표시', () => {
	beforeEach(() => {
		vi.clearAllMocks();
	});

	it('퍼센트를 알면 결정형 바로 채운다', async () => {
		let push!: (p: AttachProgress | null) => void;
		let release!: () => void;
		drive(async (p) => {
			push = p;
			await new Promise<void>((r) => (release = r));
		});
		const { container, getByText } = render(AttachmentSection, { props: { slug: 'DEV-001' } });
		getByText('+ 첨부').click();

		await waitFor(() => expect(push).toBeTypeOf('function'));
		push({ name: 'big.zip', index: 1, total: 1, phase: 'uploading', percent: 42.4 });
		await waitFor(() => {
			expect(bar(container)?.className).toContain('determinate');
			expect(bar(container)?.style.width).toBe('42.4%');
		});
		// 스크린리더에도 숫자가 가야 한다.
		expect(row(container)?.getAttribute('aria-valuenow')).toBe('42');
		expect(container.textContent).toContain('42%');

		release();
	});

	it('퍼센트를 모르면 불확정 바 + 준비 중 표시', async () => {
		let push!: (p: AttachProgress | null) => void;
		let release!: () => void;
		drive(async (p) => {
			push = p;
			await new Promise<void>((r) => (release = r));
		});
		const { container, getByText } = render(AttachmentSection, { props: { slug: 'DEV-001' } });
		getByText('+ 첨부').click();

		await waitFor(() => expect(push).toBeTypeOf('function'));
		push({ name: 'big.zip', index: 1, total: 1, phase: 'preparing', percent: null });
		await waitFor(() => expect(row(container)).not.toBeNull());
		expect(bar(container)?.className).not.toContain('determinate');
		expect(bar(container)?.style.width).toBe('');
		expect(container.textContent).toContain('준비 중');

		release();
	});

	it('끝나면 진행 표시가 사라진다', async () => {
		drive(async (push) => {
			push({ name: 'a.zip', index: 1, total: 1, phase: 'uploading', percent: 100 });
			push(null);
		});
		const { container, getByText } = render(AttachmentSection, { props: { slug: 'DEV-001' } });
		getByText('+ 첨부').click();
		await waitFor(() => expect(row(container)).toBeNull());
	});
});
