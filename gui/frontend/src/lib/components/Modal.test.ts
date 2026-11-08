// DEV-440 후속: 모달 틀 — 막 · Esc · X 로 닫고, 일하는 중에는 안 닫히고, 첫 포커스가 X 로 가지 않는다.

import { describe, it, expect, vi, afterEach } from 'vitest';
import { render, screen, fireEvent, cleanup } from '@testing-library/svelte';
import { createRawSnippet } from 'svelte';
import Modal from './Modal.svelte';

afterEach(() => {
	cleanup();
	document.body.innerHTML = '';
});

const body = createRawSnippet(() => ({ render: () => '<input aria-label="이름" />' }));

describe('DEV-440 Modal', () => {
	it('제목을 그리고, 첫 포커스는 X 가 아니라 내용으로 간다', () => {
		render(Modal, { props: { title: '새 타입', onclose: () => {}, children: body } });
		expect(screen.getByText('새 타입')).toBeTruthy();
		expect(document.activeElement).toBe(screen.getByLabelText('이름'));
	});

	it('Esc · X · 막으로 닫는다', async () => {
		const onclose = vi.fn();
		render(Modal, { props: { title: '창', onclose, children: body } });
		await fireEvent.keyDown(screen.getByRole('dialog'), { key: 'Escape' });
		await fireEvent.click(screen.getByLabelText('닫기'));
		await fireEvent.click(document.querySelector('.modal-ov')!);
		expect(onclose).toHaveBeenCalledTimes(3);
	});

	it('일하는 중에는 닫히지 않는다', async () => {
		const onclose = vi.fn();
		render(Modal, { props: { title: '창', busy: true, onclose, children: body } });
		await fireEvent.keyDown(screen.getByRole('dialog'), { key: 'Escape' });
		await fireEvent.click(document.querySelector('.modal-ov')!);
		expect(onclose).not.toHaveBeenCalled();
	});
});
