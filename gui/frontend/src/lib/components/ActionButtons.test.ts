// DEV-440: 취소 · 확인 단추 쌍 — 순서는 언제나 [취소][하는 일].

import { describe, it, expect, vi, afterEach } from 'vitest';
import { render, screen, fireEvent, cleanup } from '@testing-library/svelte';
import ActionButtons from './ActionButtons.svelte';
import ConfirmDialog from './ConfirmDialog.svelte';

afterEach(() => cleanup());

const order = () => screen.getAllByRole('button').map((b) => b.textContent?.trim());

describe('DEV-440 단추 순서', () => {
	it('취소가 왼쪽, 하는 일이 오른쪽', () => {
		render(ActionButtons, { props: { label: '저장', oncancel: () => {} } });
		expect(order()).toEqual(['취소', '저장']);
	});

	it('확인 창도 같은 순서 — 위험한 것이든 아니든', () => {
		render(ConfirmDialog, {
			props: { open: true, message: '지울까요?', confirmLabel: '삭제', danger: true }
		});
		expect(order()).toEqual(['취소', '삭제']);
	});

	it('각 단추가 제 일을 한다 · 일하는 중이면 둘 다 꺼진다', async () => {
		const onconfirm = vi.fn();
		const oncancel = vi.fn();
		render(ActionButtons, { props: { label: '만들기', onconfirm, oncancel } });
		await fireEvent.click(screen.getByText('만들기'));
		await fireEvent.click(screen.getByText('취소'));
		expect(onconfirm).toHaveBeenCalledOnce();
		expect(oncancel).toHaveBeenCalledOnce();
		cleanup();
		render(ActionButtons, { props: { label: '만들기', busy: true, oncancel } });
		expect(screen.getAllByRole('button').every((b) => (b as HTMLButtonElement).disabled)).toBe(
			true
		);
	});

	it('하는 일만 끌 수 있다 — 취소는 남는다', () => {
		render(ActionButtons, { props: { label: '만들기', disabled: true, oncancel: () => {} } });
		expect((screen.getByText('만들기') as HTMLButtonElement).disabled).toBe(true);
		expect((screen.getByText('취소') as HTMLButtonElement).disabled).toBe(false);
	});
});
