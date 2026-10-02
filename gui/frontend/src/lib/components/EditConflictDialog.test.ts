// DEV-435: 충돌 화면 — 구간마다 다 골라야 넣을 수 있고, 넣으면 고른 대로 이은 글과 "지금 저장된 본문" 을 돌려준다.
// 닫아도 아무것도 바꾸지 않는다(내 글은 편집기에 그대로).

import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/svelte';
import EditConflictDialog from './EditConflictDialog.svelte';
import type { EditConflict } from '$lib/api/editConflict';

const conflict: EditConflict = {
	reason: 'text',
	message: '같은 줄을 고쳤습니다',
	current: '하나\n셋 — 남\n다섯\n',
	segments: [
		{ kind: 'same', text: '하나\n' },
		{ kind: 'conflict', base: '셋\n', current: '셋 — 남\n', mine: '셋 — 나\n' },
		{ kind: 'same', text: '다섯\n' }
	]
};

describe('DEV-435 충돌 화면', () => {
	it('다 고르기 전에는 넣을 수 없고, 고르면 이은 글을 돌려준다', async () => {
		const onresolve = vi.fn();
		render(EditConflictDialog, { props: { conflict, mine: '하나\n셋 — 나\n다섯\n', onresolve } });
		const apply = screen.getByText('합친 글을 편집기에 넣기') as HTMLButtonElement;
		expect(apply.disabled).toBe(true);
		await fireEvent.click(screen.getByText('내 것'));
		expect(apply.disabled).toBe(false);
		await fireEvent.click(apply);
		expect(onresolve).toHaveBeenCalledWith('하나\n셋 — 나\n다섯\n', '하나\n셋 — 남\n다섯\n');
	});

	it('닫기는 아무것도 고치지 않는다', async () => {
		const onresolve = vi.fn();
		const onclose = vi.fn();
		render(EditConflictDialog, { props: { conflict, onresolve, onclose } });
		await fireEvent.click(screen.getByText('닫기 (내 글은 편집기에 그대로)'));
		expect(onclose).toHaveBeenCalled();
		expect(onresolve).not.toHaveBeenCalled();
	});

	it('번호가 바뀌었으면 새 번호로 여는 길을 준다', async () => {
		const onopen = vi.fn();
		render(EditConflictDialog, {
			props: {
				conflict: {
					reason: 'renamed',
					message: 'DEV-001 은 BUG-005 로 바뀌었습니다',
					segments: [],
					new_id: 'BUG-005'
				},
				onopen
			}
		});
		await fireEvent.click(screen.getByText('BUG-005 로 열기 (내 글은 클립보드에 복사)'));
		expect(onopen).toHaveBeenCalledWith('BUG-005');
	});
});
