// DEV-321: 업로드 진행률을 관측하려면 이 경로만 fetch 대신 XHR 을 써야 한다
// (fetch 는 요청 본문 전송 진행을 알려주지 않는다). 배선이 깨지면 진행 바가
// 조용히 불확정으로 돌아가므로 계약을 테스트로 고정한다.

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { postWithUploadProgress } from './transport';

type Handler = (this: FakeXhr) => void;

/** 최소한의 XMLHttpRequest 대역 — send() 시 진행/완료를 즉시 재생한다. */
class FakeXhr {
	static last: FakeXhr | null = null;
	/** 다음 응답 — 테스트가 바꾼다. */
	static response: { status: number; text: string } = { status: 200, text: '"attachments/a.zip"' };
	/** send 시 진행 이벤트로 재생할 (loaded, total) 목록. */
	static progress: Array<[number, number, boolean]> = [];
	/** true 면 send 가 네트워크 오류로 끝난다. */
	static failNetwork = false;

	method = '';
	url = '';
	headers: Record<string, string> = {};
	body: unknown = null;
	status = 0;
	statusText = '';
	responseText = '';
	upload = { onprogress: null as null | ((e: ProgressEvent) => void) };
	onload: Handler | null = null;
	onerror: Handler | null = null;
	onabort: Handler | null = null;

	constructor() {
		FakeXhr.last = this;
	}
	open(method: string, url: string) {
		this.method = method;
		this.url = url;
	}
	setRequestHeader(k: string, v: string) {
		this.headers[k] = v;
	}
	send(body: unknown) {
		this.body = body;
		for (const [loaded, total, lengthComputable] of FakeXhr.progress) {
			this.upload.onprogress?.({ loaded, total, lengthComputable } as ProgressEvent);
		}
		if (FakeXhr.failNetwork) {
			this.onerror?.call(this);
			return;
		}
		this.status = FakeXhr.response.status;
		this.responseText = FakeXhr.response.text;
		this.onload?.call(this);
	}
}

describe('postWithUploadProgress (DEV-321)', () => {
	beforeEach(() => {
		FakeXhr.last = null;
		FakeXhr.response = { status: 200, text: '"attachments/a.zip"' };
		FakeXhr.progress = [];
		FakeXhr.failNetwork = false;
		vi.stubGlobal('XMLHttpRequest', FakeXhr as unknown as typeof XMLHttpRequest);
	});
	afterEach(() => {
		vi.unstubAllGlobals();
	});

	it('전송 진행을 바이트로 보고하고 응답을 파싱한다', async () => {
		FakeXhr.progress = [
			[0, 100, true],
			[40, 100, true],
			[100, 100, true]
		];
		const seen: Array<[number, number]> = [];
		const out = await postWithUploadProgress<string>(
			'/api/attachments',
			{ data_base64: 'AAA', ext: 'zip' },
			(sent, total) => seen.push([sent, total])
		);
		expect(out).toBe('attachments/a.zip');
		expect(seen).toEqual([
			[0, 100],
			[40, 100],
			[100, 100]
		]);
		expect(FakeXhr.last?.method).toBe('POST');
		expect(FakeXhr.last?.headers['Content-Type']).toBe('application/json');
		expect(JSON.parse(String(FakeXhr.last?.body))).toEqual({ data_base64: 'AAA', ext: 'zip' });
	});

	it('총량을 모르는 진행 이벤트는 보고하지 않는다 (0 나눗셈 방지)', async () => {
		FakeXhr.progress = [[10, 0, false]];
		const seen: number[] = [];
		await postWithUploadProgress('/api/attachments', {}, (sent) => seen.push(sent));
		expect(seen).toEqual([]);
	});

	it('서버 에러 본문의 메시지를 Error 로 전달한다', async () => {
		FakeXhr.response = { status: 413, text: '{"error":"첨부 파일이 너무 큽니다"}' };
		await expect(postWithUploadProgress('/api/attachments', {})).rejects.toThrow(
			'첨부 파일이 너무 큽니다'
		);
	});

	it('네트워크 오류도 reject 로 이어진다 (영원히 대기 금지)', async () => {
		FakeXhr.failNetwork = true;
		await expect(postWithUploadProgress('/api/attachments', {})).rejects.toThrow('network error');
	});

	it('204 는 빈 응답으로 처리한다', async () => {
		FakeXhr.response = { status: 204, text: '' };
		await expect(postWithUploadProgress('/api/attachments', {})).resolves.toBeUndefined();
	});
});
