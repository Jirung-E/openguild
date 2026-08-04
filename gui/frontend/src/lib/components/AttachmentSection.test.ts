// DEV-321: 진행 바가 실제 %를 반영하는지 — 이게 깨지면 사용자에겐 예전처럼
// "돌고는 있는데 얼마나 남았는지 모르는" 상태로 조용히 되돌아간다.

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, waitFor } from '@testing-library/svelte';
import AttachmentSection from './AttachmentSection.svelte';
import { pickAndUploadAttachments, type AttachQueueItem } from '$lib/utils/editor-attach';

vi.mock('$lib/utils/editor-attach', () => ({
	pickAndUploadAttachments: vi.fn()
}));
vi.mock('$lib/api/client', () => ({
	api: { post: vi.fn(), delete: vi.fn() }
}));
vi.mock('$lib/utils/banner', () => ({
	guildFileUrl: vi.fn().mockResolvedValue('blob:preview')
}));

/** 업로드를 시작시키고, 테스트가 대기열 상태를 직접 밀어 넣을 수 있게 한다. */
function drive(steps: (push: (q: AttachQueueItem[] | null) => void) => Promise<void> | void) {
	vi.mocked(pickAndUploadAttachments).mockImplementation(async (handlers) => {
		await steps((q) => handlers.onQueue?.(q));
	});
}

/** 한 줄짜리 대기열 — 기존 단일 파일 케이스. */
const one = (p: Partial<AttachQueueItem>): AttachQueueItem[] => [
	{ id: 0, name: 'big.zip', status: 'uploading', phase: 'uploading', percent: null, ...p }
];

const bar = (c: HTMLElement) => c.querySelector<HTMLElement>('.up-fill');
const row = (c: HTMLElement) => c.querySelector<HTMLElement>('[role="progressbar"]');

describe('AttachmentSection 업로드 진행 표시', () => {
	beforeEach(() => {
		vi.clearAllMocks();
	});

	it('퍼센트를 알면 결정형 바로 채운다', async () => {
		let push!: (q: AttachQueueItem[] | null) => void;
		let release!: () => void;
		drive(async (p) => {
			push = p;
			await new Promise<void>((r) => (release = r));
		});
		const { container, getByText } = render(AttachmentSection, { props: { slug: 'DEV-001' } });
		getByText('+ 첨부').click();

		await waitFor(() => expect(push).toBeTypeOf('function'));
		push(one({ percent: 42.4 }));
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
		let push!: (q: AttachQueueItem[] | null) => void;
		let release!: () => void;
		drive(async (p) => {
			push = p;
			await new Promise<void>((r) => (release = r));
		});
		const { container, getByText } = render(AttachmentSection, { props: { slug: 'DEV-001' } });
		getByText('+ 첨부').click();

		await waitFor(() => expect(push).toBeTypeOf('function'));
		push(one({ phase: 'preparing', percent: null }));
		await waitFor(() => expect(row(container)).not.toBeNull());
		expect(bar(container)?.className).not.toContain('determinate');
		expect(bar(container)?.style.width).toBe('');
		expect(container.textContent).toContain('준비 중');

		release();
	});

	// DEV-322: 여러 개를 고르면 **전체 목록**이 상태와 함께 보여야 한다.
	it('고른 파일 전체를 대기열로 보여준다', async () => {
		let push!: (q: AttachQueueItem[] | null) => void;
		let release!: () => void;
		drive(async (p) => {
			push = p;
			await new Promise<void>((r) => (release = r));
		});
		const { container, getByText } = render(AttachmentSection, { props: { slug: 'DEV-001' } });
		getByText('+ 첨부').click();
		await waitFor(() => expect(push).toBeTypeOf('function'));

		push([
			{ id: 0, name: 'a.zip', status: 'done', phase: null, percent: 100 },
			{ id: 1, name: 'b.zip', status: 'uploading', phase: 'uploading', percent: 30 },
			{ id: 2, name: 'c.zip', status: 'pending', phase: null, percent: null }
		]);

		await waitFor(() => expect(container.querySelectorAll('.upq-item')).toHaveLength(3));
		const text = container.textContent ?? '';
		for (const n of ['a.zip', 'b.zip', 'c.zip']) expect(text).toContain(n);
		// 아직 시작 안 한 항목은 '대기' 로 알린다(진행 바 없음).
		expect(text).toContain('대기');
		expect(container.querySelectorAll('[role="progressbar"]')).toHaveLength(2);

		release();
	});

	// DEV-322: 실패한 항목은 목록에 남아야 한다 — 배너 한 줄로는 어떤 파일이
	// 실패했는지 알 수 없다.
	it('실패한 항목을 목록에 표시한다', async () => {
		let release!: () => void;
		drive(async (push) => {
			push([
				{ id: 0, name: 'ok.zip', status: 'done', phase: null, percent: 100 },
				{ id: 1, name: 'bad.zip', status: 'error', phase: null, percent: null, error: '용량 초과' }
			]);
			await new Promise<void>((r) => (release = r));
		});
		const { container, getByText } = render(AttachmentSection, { props: { slug: 'DEV-001' } });
		getByText('+ 첨부').click();

		await waitFor(() => expect(container.querySelector('.upq-item.failed')).not.toBeNull());
		expect(container.textContent).toContain('bad.zip');
		expect(container.textContent).toContain('용량 초과');

		release();
	});

	// DEV-323: 취소 버튼은 업로드가 도는 동안만 보이고, 누르면 손잡이가 호출된다.
	it('취소 버튼이 취소 손잡이를 부른다', async () => {
		const cancelAll = vi.fn();
		let release!: () => void;
		vi.mocked(pickAndUploadAttachments).mockImplementation(async (handlers) => {
			handlers.onCancelHandle?.(cancelAll);
			handlers.onQueue?.(one({ percent: 10 }));
			await new Promise<void>((r) => (release = r));
		});
		const { container, getByText } = render(AttachmentSection, { props: { slug: 'DEV-001' } });
		getByText('+ 첨부').click();

		await waitFor(() => expect(container.querySelector('.upq-cancel')).not.toBeNull());
		container.querySelector<HTMLButtonElement>('.upq-cancel')!.click();
		expect(cancelAll).toHaveBeenCalledOnce();

		release();
	});

	// DEV-323: 취소된 항목은 실패가 아니라 '취소됨' 으로 남는다 — 무엇이 올라갔고
	// 무엇이 안 올라갔는지가 사용자에게 필요한 정보다.
	it('취소된 항목을 취소됨으로 표시한다', async () => {
		let release!: () => void;
		drive(async (push) => {
			push([
				{ id: 0, name: 'done.zip', status: 'done', phase: null, percent: 100 },
				{ id: 1, name: 'stopped.zip', status: 'cancelled', phase: null, percent: null },
				{ id: 2, name: 'never.zip', status: 'cancelled', phase: null, percent: null }
			]);
			await new Promise<void>((r) => (release = r));
		});
		const { container, getByText } = render(AttachmentSection, { props: { slug: 'DEV-001' } });
		getByText('+ 첨부').click();

		await waitFor(() => expect(container.querySelectorAll('.upq-item.cancelled')).toHaveLength(2));
		expect(container.textContent).toContain('취소됨');
		// 취소는 실패가 아니므로 실패 표시가 붙으면 안 된다.
		expect(container.querySelector('.upq-item.failed')).toBeNull();

		release();
	});

	it('끝나면 진행 표시가 사라진다', async () => {
		drive(async (push) => {
			push(one({ name: 'a.zip', status: 'done', percent: 100 }));
			push(null);
		});
		const { container, getByText } = render(AttachmentSection, { props: { slug: 'DEV-001' } });
		getByText('+ 첨부').click();
		await waitFor(() => expect(row(container)).toBeNull());
	});
});
