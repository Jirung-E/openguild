// BUG-272: 확인 대화상자의 포커스 — 열리면 안으로, Tab 은 안에서만, 스친 Enter 는 확인을 누르지 않는다, 닫으면 제자리로.

import { describe, it, expect, vi, afterEach } from 'vitest';
import { render, screen, fireEvent, cleanup } from '@testing-library/svelte';
import ConfirmDialog from './ConfirmDialog.svelte';

afterEach(() => {
	cleanup();
	document.body.innerHTML = '';
});

function outsideButton() {
	const b = document.createElement('button');
	b.textContent = '뒤 페이지 버튼';
	document.body.appendChild(b);
	b.focus();
	return b;
}

describe('BUG-272 확인 대화상자 포커스', () => {
	it('위험한 동작은 취소에서, 아니면 확인에서 시작한다', () => {
		render(ConfirmDialog, { props: { open: true, message: '지울까요?', confirmLabel: '삭제', danger: true } });
		expect(document.activeElement?.textContent?.trim()).toBe('취소');
		cleanup();
		render(ConfirmDialog, { props: { open: true, message: '할까요?', confirmLabel: '진행' } });
		expect(document.activeElement?.textContent?.trim()).toBe('진행');
	});

	it('뒤 페이지에 포커스가 있다가 Enter 가 와도 확인을 누르지 않는다', async () => {
		const back = outsideButton();
		const onconfirm = vi.fn();
		render(ConfirmDialog, { props: { open: true, message: '지울까요?', danger: true, onconfirm } });
		// 포커스는 대화상자 안으로 옮겨졌다.
		expect(document.activeElement).not.toBe(back);
		// 예전엔 window 에서 Enter 를 받아 어디서 눌러도 확인이었다.
		await fireEvent.keyDown(window, { key: 'Enter' });
		await fireEvent.keyDown(document.body, { key: 'Enter' });
		expect(onconfirm).not.toHaveBeenCalled();
	});

	it('Tab 은 대화상자 안에서만 돈다', async () => {
		render(ConfirmDialog, { props: { open: true, message: '할까요?', confirmLabel: '진행' } });
		const cancel = screen.getByText('취소');
		const ok = screen.getByText('진행');
		ok.focus();
		await fireEvent.keyDown(ok, { key: 'Tab' });
		expect(document.activeElement).toBe(cancel);
		await fireEvent.keyDown(cancel, { key: 'Tab', shiftKey: true });
		expect(document.activeElement).toBe(ok);
	});

	it('밖으로 나간 포커스는 다시 안으로 온다', () => {
		const back = outsideButton();
		render(ConfirmDialog, { props: { open: true, message: '할까요?' } });
		back.focus();
		expect(document.activeElement).not.toBe(back);
	});

	it('Esc 는 취소이고, 닫으면 열기 전 요소로 돌아간다', async () => {
		const back = outsideButton();
		const oncancel = vi.fn();
		const view = render(ConfirmDialog, { props: { open: true, message: '할까요?', oncancel } });
		await fireEvent.keyDown(screen.getByRole('alertdialog'), { key: 'Escape' });
		expect(oncancel).toHaveBeenCalledOnce();
		await view.rerender({ open: false, message: '할까요?', oncancel });
		expect(document.activeElement).toBe(back);
	});
});
