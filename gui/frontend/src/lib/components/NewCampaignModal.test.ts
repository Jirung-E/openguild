// DEV-395: 새 캠페인이 페이지가 아니라 **모달**이다(새 퀘스트와 통일).
//
// 옮기면서 깨지기 쉬운 것 셋을 본다 — 제목 없이 못 만든다, 본문이 있으면 생성 뒤 한 번 더
// 저장한다(`create` 가 본문을 안 받는다), 실패해도 모달이 닫히지 않고 이유가 남는다.

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/svelte';
import NewCampaignModal from './NewCampaignModal.svelte';

const create = vi.fn();
const update = vi.fn();
vi.mock('$lib/api/campaigns', () => ({
	campaignsApi: {
		create: (b: unknown) => create(b),
		update: (s: string, b: unknown) => update(s, b)
	}
}));

const created = { id: 1, campaign_slug: 'C-007', title: '베타' };

function open() {
	const onclose = vi.fn();
	const oncreated = vi.fn();
	render(NewCampaignModal, { props: { onclose, oncreated } });
	return { onclose, oncreated };
}

const titleBox = () => screen.getByPlaceholderText('예: v1.0 출시') as HTMLInputElement;
const submit = () => screen.getByText('생성');

describe('DEV-395 새 캠페인 모달', () => {
	beforeEach(() => {
		create.mockReset().mockResolvedValue(created);
		update.mockReset().mockResolvedValue(created);
	});

	it('제목이 비면 만들 수 없다', async () => {
		open();
		expect((submit() as HTMLButtonElement).disabled).toBe(true);
		await fireEvent.input(titleBox(), { target: { value: '베타' } });
		expect((submit() as HTMLButtonElement).disabled).toBe(false);
	});

	it('만들면 닫고 만들어진 캠페인을 넘긴다', async () => {
		const { onclose, oncreated } = open();
		await fireEvent.input(titleBox(), { target: { value: '베타' } });
		await fireEvent.click(submit());
		await waitFor(() => expect(oncreated).toHaveBeenCalledWith(created));
		expect(onclose).toHaveBeenCalled();
		expect(create).toHaveBeenCalledWith(expect.objectContaining({ title: '베타' }));
		// 본문이 없으면 두 번째 저장은 없다.
		expect(update).not.toHaveBeenCalled();
	});

	it('본문이 있으면 생성 뒤 한 번 더 저장한다 — create 가 본문을 안 받는다', async () => {
		open();
		await fireEvent.input(titleBox(), { target: { value: '베타' } });
		await fireEvent.input(screen.getByPlaceholderText('기획 내용 / 메모…'), {
			target: { value: '## 계획' }
		});
		await fireEvent.click(submit());
		await waitFor(() => expect(update).toHaveBeenCalledWith('C-007', { description: '## 계획' }));
	});

	it('실패하면 닫지 않고 이유를 남긴다', async () => {
		create.mockRejectedValue(new Error('서버가 거절'));
		const { onclose, oncreated } = open();
		await fireEvent.input(titleBox(), { target: { value: '베타' } });
		await fireEvent.click(submit());
		await waitFor(() => expect(screen.getByText('서버가 거절')).toBeTruthy());
		expect(onclose).not.toHaveBeenCalled();
		expect(oncreated).not.toHaveBeenCalled();
	});
});
